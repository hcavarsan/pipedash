use std::collections::HashSet;

use indexmap::IndexMap;

use super::{
    ConfigError,
    ConfigErrorCode,
    ConfigValidator,
    ValidationResult,
};
use crate::infrastructure::config::schema::applications::{
    ApplicationFileConfig,
    PipelineSelector,
    TargetFileConfig,
};
use crate::infrastructure::config::schema::PipedashConfig;

impl ConfigValidator {
    pub(super) fn validate_applications(config: &PipedashConfig, result: &mut ValidationResult) {
        let providers: HashSet<&str> = config.providers.keys().map(String::as_str).collect();
        for (index, selector) in config.correlation.ignore.iter().enumerate() {
            let field = format!("correlation.ignore[{index}]");
            check_selector(
                &field,
                &selector.provider,
                &selector.pipeline,
                &providers,
                result,
            );
        }
        for (key, application) in &config.applications {
            validate_application(key, application, &providers, result);
        }
    }
}

fn invalid(field: String, message: String) -> ConfigError {
    ConfigError {
        field,
        message,
        code: ConfigErrorCode::InvalidValue,
    }
}

fn missing(field: String, message: &str) -> ConfigError {
    ConfigError {
        field,
        message: message.to_string(),
        code: ConfigErrorCode::MissingRequired,
    }
}

fn validate_application(
    key: &str, application: &ApplicationFileConfig, providers: &HashSet<&str>,
    result: &mut ValidationResult,
) {
    let prefix = format!("applications.{key}");
    if key.is_empty() {
        result.add_error(missing(
            "applications".to_string(),
            "Application ID (table key) cannot be empty",
        ));
    }
    if application.name.trim().is_empty() {
        result.add_error(missing(
            format!("{prefix}.name"),
            "Application name is required",
        ));
    }
    if application.members.is_empty() {
        result.add_error(invalid(
            format!("{prefix}.members"),
            "Config application needs at least one member".to_string(),
        ));
    }
    for (index, member) in application.members.iter().enumerate() {
        let field = format!("{prefix}.members[{index}]");
        check_selector(
            &field,
            &member.provider,
            &member.pipeline,
            providers,
            result,
        );
    }
    for (index, PipelineSelector { provider, pipeline }) in application.exclude.iter().enumerate() {
        let field = format!("{prefix}.exclude[{index}]");
        check_selector(&field, provider, pipeline, providers, result);
    }
    validate_targets(&prefix, &application.targets, result);
}

fn check_selector(
    field: &str, provider: &str, pipeline: &str, providers: &HashSet<&str>,
    result: &mut ValidationResult,
) {
    if !providers.contains(provider) {
        result.add_error(invalid(
            format!("{field}.provider"),
            format!("Unknown provider \"{provider}\": it is not a key under [providers]"),
        ));
    }
    check_glob(format!("{field}.pipeline"), pipeline, result);
}

fn check_glob(field: String, pattern: &str, result: &mut ValidationResult) {
    if let Err(error) = glob::Pattern::new(pattern) {
        result.add_error(invalid(
            field,
            format!("Invalid pattern \"{pattern}\": {error}"),
        ));
    }
}

fn validate_targets(
    prefix: &str, targets: &IndexMap<String, TargetFileConfig>, result: &mut ValidationResult,
) {
    for (key, target) in targets {
        let field = format!("{prefix}.targets.{key}");
        check_group(&field, key, target, targets, result);
        for pattern in &target.clusters {
            check_glob(format!("{field}.clusters"), pattern, result);
        }
        for pattern in &target.namespaces {
            check_glob(format!("{field}.namespaces"), pattern, result);
        }
        for after in &target.after {
            if !targets.contains_key(after) {
                result.add_error(invalid(
                    format!("{field}.after"),
                    format!("Unknown target \"{after}\""),
                ));
            }
        }
    }
    if let Some(key) = cyclic_target(targets) {
        result.add_error(invalid(
            format!("{prefix}.targets.{key}.after"),
            format!("Target \"{key}\" is part of a cycle in `after`"),
        ));
    }
}

fn check_group(
    field: &str, key: &str, target: &TargetFileConfig,
    targets: &IndexMap<String, TargetFileConfig>, result: &mut ValidationResult,
) {
    let Some(group) = target.group.as_deref() else {
        if target.name.trim().is_empty() {
            result.add_error(missing(format!("{field}.name"), "Target name is required"));
        }
        return;
    };
    let problem = if group == key {
        Some(format!("Target \"{key}\" cannot be in its own group"))
    } else {
        match targets.get(group) {
            None => Some(format!("Unknown group table \"{group}\"")),
            Some(owner) if owner.group.is_some() => Some(format!(
                "Group table \"{group}\" is itself in a group; groups do not chain"
            )),
            Some(_) => None,
        }
    };
    if let Some(message) = problem {
        result.add_error(invalid(format!("{field}.group"), message));
    }
    if !target.after.is_empty() {
        result.add_error(invalid(
            format!("{field}.after"),
            format!("A table in a group cannot set `after`; set it on \"{group}\""),
        ));
    }
}

fn cyclic_target(targets: &IndexMap<String, TargetFileConfig>) -> Option<&str> {
    let mut done: HashSet<&str> = HashSet::new();
    targets.keys().find_map(|key| {
        let mut path: Vec<&str> = Vec::new();
        visit(key, targets, &mut path, &mut done)
    })
}

fn visit<'a>(
    key: &'a str, targets: &'a IndexMap<String, TargetFileConfig>, path: &mut Vec<&'a str>,
    done: &mut HashSet<&'a str>,
) -> Option<&'a str> {
    let key = targets.get(key).map_or(key, |target| target.owner(key));
    if path.contains(&key) {
        return Some(key);
    }
    if done.contains(key) {
        return None;
    }
    path.push(key);
    let found = targets.get(key).and_then(|target| {
        target
            .after
            .iter()
            .find_map(|after| visit(after, targets, path, done))
    });
    path.pop();
    done.insert(key);
    found
}
