use std::collections::BTreeSet;
use std::fs;
use std::path::{
    Path,
    PathBuf,
};

use inquire::autocompletion::Replacement;
use inquire::validator::Validation;
use inquire::{
    Autocomplete,
    Confirm,
    CustomUserError,
    InquireError,
    MultiSelect,
    Select,
    Text,
};

use crate::args::{
    Archetype,
    Args,
    Capability,
    IdShape,
};
use crate::spec::{
    self,
    Spec,
};

const ARCHETYPES: [(Archetype, &str); 2] = [
    (
        Archetype::Rest,
        "rest — HTTP API with a base URL and a bearer token",
    ),
    (
        Archetype::Kubernetes,
        "kubernetes — kubeconfig file or pasted content, kube client",
    ),
];
const ID_SHAPES: [(IdShape, &str); 3] = [
    (IdShape::Numeric, "numeric — e.g. 42"),
    (IdShape::Path, "path — owner/repo/workflow"),
    (IdShape::Namespaced, "namespaced — namespace__name"),
];
const CAPABILITIES: [(Capability, &str); 3] = [
    (Capability::Trigger, "trigger — start runs from Pipedash"),
    (Capability::Cancel, "cancel — cancel running runs"),
    (Capability::Logs, "logs — show run logs"),
];

fn ask<T>(result: Result<T, InquireError>) -> Result<T, String> {
    result.map_err(|error| match error {
        InquireError::OperationCanceled | InquireError::OperationInterrupted => {
            "cancelled".to_string()
        }
        other => other.to_string(),
    })
}

fn validator(
    check: impl Fn(&str) -> Result<(), String> + Clone,
) -> impl Fn(&str) -> Result<Validation, CustomUserError> + Clone {
    move |input| {
        Ok(match check(input) {
            Ok(()) => Validation::Valid,
            Err(message) => Validation::Invalid(message.into()),
        })
    }
}

fn split_list(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}

fn text(
    message: &str, default: &str, check: impl Fn(&str) -> Result<(), String> + Clone,
) -> Result<String, String> {
    ask(Text::new(message)
        .with_default(default)
        .with_validator(validator(check))
        .prompt())
}

fn list(
    message: &str, check: impl Fn(&[String]) -> Result<(), String> + Clone,
) -> Result<Vec<String>, String> {
    let answer = ask(Text::new(message)
        .with_validator(validator(move |input: &str| check(&split_list(input))))
        .prompt())?;
    Ok(split_list(&answer))
}

fn select<T: Copy>(message: &str, options: &[(T, &str)], cursor: usize) -> Result<T, String> {
    let labels: Vec<&str> = options.iter().map(|(_, label)| *label).collect();
    let chosen = ask(Select::new(message, labels)
        .with_starting_cursor(cursor)
        .raw_prompt())?;
    Ok(options[chosen.index].0)
}

fn ask_id(taken: &BTreeSet<String>) -> Result<String, String> {
    let taken = taken.clone();
    ask(Text::new("Provider id")
        .with_help_message("kebab-case; creates crates/pipedash-plugin-<id>")
        .with_validator(validator(move |id: &str| spec::validate_id(id, &taken)))
        .prompt())
}

fn ask_capabilities() -> Result<Vec<Capability>, String> {
    let labels: Vec<&str> = CAPABILITIES.iter().map(|(_, label)| *label).collect();
    let chosen = ask(MultiSelect::new("Capabilities", labels)
        .with_all_selected_by_default()
        .raw_prompt())?;
    Ok(chosen
        .iter()
        .map(|option| CAPABILITIES[option.index].0)
        .collect())
}

fn ask_icon() -> Result<Option<PathBuf>, String> {
    let check = |input: &str| match input.trim() {
        "" => Ok(()),
        path => spec::validate_icon(Path::new(path)).map(drop),
    };
    let answer = ask(Text::new("Icon SVG path (empty for a placeholder)")
        .with_autocomplete(SvgPaths)
        .with_validator(validator(check))
        .prompt())?;
    Ok(Some(answer.trim())
        .filter(|path| !path.is_empty())
        .map(PathBuf::from))
}

fn fill_rest(filled: &mut Args, name: &str) -> Result<(), String> {
    if filled.ping_path.is_none() {
        let path = text(
            "Credential check endpoint (GET)",
            spec::DEFAULT_PING_PATH,
            spec::validate_ping_path,
        )?;
        filled.ping_path = Some(path);
    }
    if filled.token_label.is_none() {
        let check = |value: &str| spec::validate_text("token label", value);
        filled.token_label = Some(text("Token field label", spec::DEFAULT_TOKEN_LABEL, check)?);
    }
    if filled.token_help.is_none() {
        let check = |value: &str| spec::validate_text("token help", value);
        filled.token_help = Some(text(
            "Token field help",
            &spec::default_token_help(name),
            check,
        )?);
    }
    if filled.token_prefixes.is_none() {
        let message = "Token prefixes (comma-separated, optional)";
        filled.token_prefixes = Some(list(message, spec::validate_token_prefixes)?);
    }
    Ok(())
}

fn fill_args(args: &Args, taken: &BTreeSet<String>) -> Result<Args, String> {
    let mut filled = args.clone();
    let id = match &args.id {
        Some(id) => {
            spec::validate_id(id, taken)?;
            id.clone()
        }
        None => ask_id(taken)?,
    };
    let name = match &args.name {
        Some(name) => name.clone(),
        None => text("Display name", &spec::title_case(&id), |value: &str| {
            spec::validate_text("name", value)
        })?,
    };
    let archetype = match args.archetype {
        Some(archetype) => archetype,
        None => select("Archetype", &ARCHETYPES, 0)?,
    };
    spec::check_rest_only(args, archetype)?;
    if args.id_shape.is_none() {
        let default = spec::default_id_shape(archetype);
        let cursor = ID_SHAPES
            .iter()
            .position(|(shape, _)| *shape == default)
            .unwrap_or(0);
        filled.id_shape = Some(select("Pipeline id shape", &ID_SHAPES, cursor)?);
    }
    if args.capabilities.is_none() && !args.read_only {
        filled.capabilities = Some(ask_capabilities()?);
    }
    if args.description.is_none() {
        let default = spec::default_description(&name, &spec::capabilities(&filled));
        let check = |value: &str| spec::validate_text("description", value);
        filled.description = Some(text("Description (shown in the UI)", &default, check)?);
    }
    if archetype == Archetype::Rest {
        fill_rest(&mut filled, &name)?;
    }
    if args.aliases.is_none() {
        let (alias_id, alias_taken) = (id.clone(), taken.clone());
        let check =
            move |aliases: &[String]| spec::validate_aliases(aliases, &alias_id, &alias_taken);
        filled.aliases = Some(list("Aliases (comma-separated, optional)", check)?);
    }
    if args.icon.is_none() {
        filled.icon = ask_icon()?;
    }
    filled.id = Some(id);
    filled.name = Some(name);
    filled.archetype = Some(archetype);
    Ok(filled)
}

fn show(items: &[String]) -> String {
    if items.is_empty() {
        return "none".to_string();
    }
    items.join(", ")
}

fn summary(spec: &Spec, icon: Option<&Path>) {
    let capabilities = [
        ("trigger", spec.capabilities.trigger),
        ("cancel", spec.capabilities.cancel),
        ("logs", spec.capabilities.logs),
    ]
    .into_iter()
    .filter(|(_, on)| *on)
    .map(|(name, _)| name.to_string())
    .collect::<Vec<_>>();
    println!();
    println!("  crate          crates/{}", spec.crate_name);
    println!("  archetype      {:?}", spec.archetype);
    println!("  id shape       {:?}", spec.id_shape);
    println!("  capabilities   {}", show(&capabilities));
    println!("  aliases        {}", show(&spec.aliases));
    println!("  token prefixes {}", show(&spec.token_prefixes));
    let icon = icon.map_or("placeholder".to_string(), |path| path.display().to_string());
    println!("  icon           {icon}");
    println!();
}

pub(crate) fn fill(args: &Args, taken: &BTreeSet<String>) -> Result<Spec, String> {
    let filled = fill_args(args, taken)?;
    let spec = spec::from_args(&filled, taken)?;
    summary(&spec, filled.icon.as_deref());
    if !args.yes && !ask(Confirm::new("Generate?").with_default(true).prompt())? {
        return Err("cancelled".to_string());
    }
    Ok(spec)
}

#[derive(Clone)]
struct SvgPaths;

impl Autocomplete for SvgPaths {
    fn get_suggestions(&mut self, input: &str) -> Result<Vec<String>, CustomUserError> {
        let (dir, prefix) = match input.rfind('/') {
            Some(slash) => (&input[..=slash], &input[slash + 1..]),
            None => ("", input),
        };
        let Ok(entries) = fs::read_dir(if dir.is_empty() { "." } else { dir }) else {
            return Ok(Vec::new());
        };
        let mut suggestions: Vec<String> = entries
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                if !name.starts_with(prefix) {
                    return None;
                }
                if entry.path().is_dir() {
                    return Some(format!("{dir}{name}/"));
                }
                name.ends_with(".svg").then(|| format!("{dir}{name}"))
            })
            .collect();
        suggestions.sort();
        suggestions.truncate(15);
        Ok(suggestions)
    }

    fn get_completion(
        &mut self, _input: &str, highlighted: Option<String>,
    ) -> Result<Replacement, CustomUserError> {
        Ok(highlighted)
    }
}
