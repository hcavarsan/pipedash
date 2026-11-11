use std::collections::{
    BTreeMap,
    BTreeSet,
};

use toml_edit::{
    value,
    Array,
    ArrayOfTables,
    DocumentMut,
    Item,
    Table,
};

use crate::domain::application::Stage;
use crate::infrastructure::config::schema::applications::glob_matches;

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
    name: String,
    clusters: Vec<String>,
    namespaces: Vec<String>,
    group_key: String,
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

fn matches(table: &TargetTable, target: &TomlTarget<'_>) -> bool {
    let cluster = table.clusters.is_empty()
        || table
            .clusters
            .iter()
            .any(|pattern| glob_matches(pattern, target.cluster));
    let namespace = table.namespaces.is_empty()
        || target.namespace.is_some_and(|namespace| {
            table
                .namespaces
                .iter()
                .any(|pattern| glob_matches(pattern, namespace))
        });
    cluster && namespace
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

fn table_of(key: String, name: &str, group_key: &str, targets: &[&TomlTarget<'_>]) -> TargetTable {
    TargetTable {
        key,
        name: name.to_string(),
        clusters: distinct(targets.iter().map(|t| t.cluster)),
        namespaces: distinct(targets.iter().filter_map(|t| t.namespace)),
        group_key: group_key.to_string(),
    }
}

fn target_tables(application: &TomlApplication<'_>) -> Vec<TargetTable> {
    let standing: Vec<&TomlTarget<'_>> = application
        .targets
        .iter()
        .filter(|target| !target.short_lived)
        .collect();
    let mut groups: BTreeMap<&str, Vec<&TomlTarget<'_>>> = BTreeMap::new();
    for target in &standing {
        groups.entry(target.group_key).or_default().push(target);
    }
    let mut taken = BTreeSet::new();
    let mut tables = Vec::new();
    for (group_key, members) in groups {
        let label = members[0].group_label;
        let whole = table_of(unique_key(&taken, label), label, group_key, &members);
        let exact = standing
            .iter()
            .all(|t| matches(&whole, t) == (t.group_key == group_key));
        let parts = if exact || members.len() == 1 {
            vec![whole]
        } else {
            members
                .iter()
                .map(|t| {
                    let key = unique_key(&taken, &format!("{label} {}", t.name));
                    taken.insert(key.clone());
                    table_of(key, t.name, group_key, &[t])
                })
                .collect()
        };
        for part in parts {
            taken.insert(part.key.clone());
            tables.push(part);
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
        let mut out = Table::new();
        out["name"] = value(table.name.as_str());
        if !table.clusters.is_empty() {
            out["clusters"] = strings(&table.clusters);
        }
        if !table.namespaces.is_empty() {
            out["namespaces"] = strings(&table.namespaces);
        }
        let after: Vec<String> = application
            .directions
            .iter()
            .filter(|(_, to)| *to == table.group_key)
            .flat_map(|(from, _)| tables.iter().filter(move |t| t.group_key == *from))
            .map(|t| t.key.clone())
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
