use std::collections::BTreeSet;

use indexmap::IndexMap;
use toml_edit::{
    value,
    Array,
    ArrayOfTables,
    DocumentMut,
    Item,
    Table,
};

use crate::domain::application::Stage;
use crate::infrastructure::config::schema::applications::TargetFileConfig;

#[cfg(test)]
mod tests;

pub(super) struct TomlMember<'a> {
    pub provider: &'a str,
    pub local_id: &'a str,
    pub stage: Stage,
}

pub(super) struct TomlTarget<'a> {
    pub cluster: &'a str,
    pub namespace: Option<&'a str>,
    pub name: &'a str,
    pub group_key: &'a str,
    pub group_label: &'a str,
    pub short_lived: bool,
}

pub(super) struct TomlApplication<'a> {
    pub name: &'a str,
    pub hidden: bool,
    pub members: Vec<TomlMember<'a>>,
    pub targets: Vec<TomlTarget<'a>>,
    pub directions: Vec<(&'a str, &'a str)>,
}

struct TargetTable {
    key: String,
    group_key: String,
    file: TargetFileConfig,
}

pub(super) fn slug(name: &str) -> String {
    let key: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_lowercase().collect::<String>()
            } else {
                "-".to_string()
            }
        })
        .collect();
    if key.is_empty() {
        "-".to_string()
    } else {
        key
    }
}

fn pattern(literal: &str) -> String {
    glob::Pattern::escape(literal)
}

fn matches(file: &TargetFileConfig, target: &TomlTarget<'_>) -> bool {
    file.matches(target.cluster, target.namespace.unwrap_or(""), target.name)
}

fn distinct<'a>(values: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut values: Vec<String> = values.map(pattern).collect();
    values.sort();
    values.dedup();
    values
}

fn unique_key(taken: &BTreeSet<String>, label: &str) -> String {
    let base = slug(label);
    let mut key = base.clone();
    let mut n = 2;
    while taken.contains(&key) {
        key = format!("{base}-{n}");
        n += 1;
    }
    key
}

fn file_of(name: &str, group: Option<&str>, targets: &[&TomlTarget<'_>]) -> TargetFileConfig {
    TargetFileConfig {
        name: name.to_string(),
        clusters: distinct(targets.iter().map(|t| t.cluster)),
        namespaces: distinct(targets.iter().map(|t| t.namespace.unwrap_or(""))),
        after: Vec::new(),
        group: group.map(str::to_string),
    }
}

fn target_tables(application: &TomlApplication<'_>) -> Vec<TargetTable> {
    let mut standing: Vec<&TomlTarget<'_>> = application
        .targets
        .iter()
        .filter(|target| !target.short_lived)
        .collect();
    standing.sort_by_key(|target| (target.cluster, target.namespace));
    let mut groups: IndexMap<&str, Vec<&TomlTarget<'_>>> = IndexMap::new();
    for target in standing {
        groups.entry(target.group_key).or_default().push(target);
    }
    let mut taken = BTreeSet::new();
    let mut tables = Vec::new();
    for (group_key, members) in groups {
        let label = members[0].group_label;
        let owner = unique_key(&taken, label);
        taken.insert(owner.clone());
        let whole = file_of(label, None, &members);
        let exact = application
            .targets
            .iter()
            .all(|t| t.group_key == group_key || !matches(&whole, t));
        if exact || members.len() == 1 {
            tables.push(TargetTable {
                key: owner,
                group_key: group_key.to_string(),
                file: whole,
            });
            continue;
        }
        for (index, target) in members.iter().enumerate() {
            let (key, file) = if index == 0 {
                (owner.clone(), file_of(label, None, &[target]))
            } else {
                let key = unique_key(&taken, &format!("{label} {}", target.name));
                taken.insert(key.clone());
                (key, file_of("", Some(&owner), &[target]))
            };
            tables.push(TargetTable {
                key,
                group_key: group_key.to_string(),
                file,
            });
        }
    }
    tables
}

fn strings(items: &[String]) -> Item {
    value(items.iter().map(String::as_str).collect::<Array>())
}

fn member_tables(application: &TomlApplication<'_>) -> ArrayOfTables {
    let mut members = ArrayOfTables::new();
    for member in &application.members {
        let mut table = Table::new();
        table["provider"] = value(member.provider);
        table["pipeline"] = value(pattern(member.local_id));
        table["stage"] = value(member.stage.as_str());
        members.push(table);
    }
    members
}

fn targets_table(application: &TomlApplication<'_>) -> Table {
    let tables = target_tables(application);
    let mut targets = Table::new();
    targets.set_implicit(true);
    for table in &tables {
        let file = &table.file;
        let mut out = Table::new();
        if !file.name.is_empty() {
            out["name"] = value(file.name.as_str());
        }
        if let Some(group) = &file.group {
            out["group"] = value(group.as_str());
        }
        if !file.clusters.is_empty() {
            out["clusters"] = strings(&file.clusters);
        }
        if !file.namespaces.is_empty() {
            out["namespaces"] = strings(&file.namespaces);
        }
        let after: Vec<String> = tables
            .iter()
            .filter(|from| from.file.group.is_none() && file.group.is_none())
            .filter(|from| {
                application
                    .directions
                    .iter()
                    .any(|(a, b)| *a == from.group_key && *b == table.group_key)
            })
            .map(|from| from.key.clone())
            .collect();
        if !after.is_empty() {
            out["after"] = strings(&after);
        }
        targets.insert(&table.key, Item::Table(out));
    }
    targets
}

pub(super) fn render(application: &TomlApplication<'_>) -> String {
    let mut app = Table::new();
    app["name"] = value(application.name);
    if application.hidden {
        app["hidden"] = value(true);
    }
    if !application.members.is_empty() {
        app.insert("members", Item::ArrayOfTables(member_tables(application)));
    }
    let targets = targets_table(application);
    if !targets.is_empty() {
        app.insert("targets", Item::Table(targets));
    }
    let mut applications = Table::new();
    applications.set_implicit(true);
    applications.insert(&slug(application.name), Item::Table(app));
    let mut doc = DocumentMut::new();
    doc.insert("applications", Item::Table(applications));
    doc.to_string()
}
