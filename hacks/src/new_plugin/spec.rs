use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use serde::Serialize;

use crate::args::{
    Archetype,
    Args,
    Capability,
    IdShape,
};

pub(crate) const DEFAULT_TOKEN_LABEL: &str = "API Token";
pub(crate) const DEFAULT_PING_PATH: &str = "/api/user";

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Capabilities {
    pub trigger: bool,
    pub cancel: bool,
    pub logs: bool,
}

impl Capabilities {
    pub(crate) fn from_list(list: &[Capability]) -> Self {
        Self {
            trigger: list.contains(&Capability::Trigger),
            cancel: list.contains(&Capability::Cancel),
            logs: list.contains(&Capability::Logs),
        }
    }
}

#[derive(Serialize, Debug)]
pub(crate) struct Spec {
    pub id: String,
    pub crate_name: String,
    pub pascal: String,
    pub name: String,
    pub description: String,
    pub archetype: Archetype,
    pub kubernetes: bool,
    pub id_shape: IdShape,
    pub capabilities: Capabilities,
    pub aliases: Vec<String>,
    pub token_prefixes: Vec<String>,
    pub token_label: String,
    pub token_help: String,
    pub ping_path: String,
    pub icon_svg: Option<String>,
    pub initial: String,
}

fn is_kebab(id: &str) -> bool {
    id.starts_with(|c: char| c.is_ascii_lowercase())
        && id.split('-').all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
}

pub(crate) fn validate_id(id: &str, taken: &BTreeSet<String>) -> Result<(), String> {
    if !is_kebab(id) {
        return Err("use lowercase kebab-case, e.g. my-ci".to_string());
    }
    if taken.contains(id) {
        return Err(format!(
            "{id} is already a provider id, alias or plugin crate"
        ));
    }
    Ok(())
}

pub(crate) fn validate_aliases(
    aliases: &[String], id: &str, taken: &BTreeSet<String>,
) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for alias in aliases {
        validate_id(alias, taken).map_err(|error| format!("alias {alias}: {error}"))?;
        if alias == id {
            return Err(format!("alias {alias} is the provider id"));
        }
        if !seen.insert(alias) {
            return Err(format!("alias {alias} is listed twice"));
        }
    }
    Ok(())
}

pub(crate) fn validate_text(field: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{field} must not be empty"));
    }
    if value.contains(['"', '\\', '{', '}', '\n', '\r']) {
        return Err(format!(
            "{field} must not contain quotes, backslashes, braces or line breaks"
        ));
    }
    Ok(())
}

pub(crate) fn validate_token_prefixes(prefixes: &[String]) -> Result<(), String> {
    for prefix in prefixes {
        if prefix.is_empty()
            || prefix.contains(|c: char| c.is_whitespace() || c == '"' || c == '\\')
        {
            return Err(format!(
                "token prefix {prefix:?} must be non-empty without spaces, quotes or backslashes"
            ));
        }
    }
    Ok(())
}

pub(crate) fn validate_ping_path(path: &str) -> Result<(), String> {
    if !path.starts_with('/') || path.contains(|c: char| c.is_whitespace() || c == '"') {
        return Err(
            "use an absolute API path without spaces or quotes, e.g. /api/user".to_string(),
        );
    }
    Ok(())
}

pub(crate) fn validate_icon(path: &Path) -> Result<String, String> {
    let content = fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    if !content.contains("<svg") {
        return Err(format!("{} is not an SVG file", path.display()));
    }
    Ok(content)
}

fn words(id: &str) -> impl Iterator<Item = String> + '_ {
    id.split('-').map(|word| {
        let mut chars = word.chars();
        chars.next().map_or_else(String::new, |first| {
            first.to_ascii_uppercase().to_string() + chars.as_str()
        })
    })
}

pub(crate) fn title_case(id: &str) -> String {
    words(id).collect::<Vec<_>>().join(" ")
}

pub(crate) fn pascal_case(id: &str) -> String {
    words(id).collect()
}

pub(crate) fn default_id_shape(archetype: Archetype) -> IdShape {
    match archetype {
        Archetype::Rest => IdShape::Numeric,
        Archetype::Kubernetes => IdShape::Namespaced,
    }
}

pub(crate) fn default_description(name: &str, capabilities: &Capabilities) -> String {
    if capabilities.trigger {
        format!("Monitor and trigger {name} pipelines")
    } else {
        format!("Monitor {name} pipelines")
    }
}

pub(crate) fn default_token_help(name: &str) -> String {
    format!("API token from your {name} account settings")
}

pub(crate) fn capabilities(args: &Args) -> Capabilities {
    match (&args.capabilities, args.read_only) {
        (_, true) => Capabilities::from_list(&[]),
        (Some(list), false) => Capabilities::from_list(list),
        (None, false) => {
            Capabilities::from_list(&[Capability::Trigger, Capability::Cancel, Capability::Logs])
        }
    }
}

pub(crate) fn check_rest_only(args: &Args, archetype: Archetype) -> Result<(), String> {
    if archetype == Archetype::Rest {
        return Ok(());
    }
    let given = [
        ("token-prefixes", args.token_prefixes.is_some()),
        ("token-label", args.token_label.is_some()),
        ("token-help", args.token_help.is_some()),
        ("ping-path", args.ping_path.is_some()),
    ];
    match given.iter().find(|(_, set)| *set) {
        Some((flag, _)) => Err(format!("--{flag} only applies to --archetype rest")),
        None => Ok(()),
    }
}

fn required(args: &Args) -> Result<(&str, &str), String> {
    if let (Some(id), Some(description)) = (&args.id, &args.description) {
        return Ok((id, description));
    }
    let missing: Vec<&str> = [
        ("--id", args.id.is_none()),
        ("--description", args.description.is_none()),
    ]
    .into_iter()
    .filter_map(|(flag, missing)| missing.then_some(flag))
    .collect();
    Err(format!(
        "missing required flags: {} (pass them, or run in a terminal without --no-input)",
        missing.join(", ")
    ))
}

pub(crate) fn from_args(args: &Args, taken: &BTreeSet<String>) -> Result<Spec, String> {
    let (id, description) = required(args)?;
    validate_id(id, taken)?;
    let archetype = args.archetype.unwrap_or(Archetype::Rest);
    check_rest_only(args, archetype)?;
    let name = args.name.clone().unwrap_or_else(|| title_case(id));
    validate_text("name", &name)?;
    validate_text("description", description)?;
    let aliases = args.aliases.clone().unwrap_or_default();
    validate_aliases(&aliases, id, taken)?;
    let token_prefixes = args.token_prefixes.clone().unwrap_or_default();
    validate_token_prefixes(&token_prefixes)?;
    let token_label = args
        .token_label
        .clone()
        .unwrap_or(DEFAULT_TOKEN_LABEL.to_string());
    validate_text("token label", &token_label)?;
    let token_help = args
        .token_help
        .clone()
        .unwrap_or_else(|| default_token_help(&name));
    validate_text("token help", &token_help)?;
    let ping_path = args
        .ping_path
        .clone()
        .unwrap_or(DEFAULT_PING_PATH.to_string());
    validate_ping_path(&ping_path)?;
    let icon_svg = args.icon.as_deref().map(validate_icon).transpose()?;

    Ok(Spec {
        id: id.to_string(),
        crate_name: format!("pipedash-plugin-{id}"),
        pascal: pascal_case(id),
        name,
        description: description.to_string(),
        archetype,
        kubernetes: archetype == Archetype::Kubernetes,
        id_shape: args.id_shape.unwrap_or(default_id_shape(archetype)),
        capabilities: capabilities(args),
        aliases,
        token_prefixes,
        token_label,
        token_help,
        ping_path,
        icon_svg,
        initial: id[..1].to_ascii_uppercase(),
    })
}

#[cfg(test)]
mod tests;
