use std::{
    path::PathBuf,
    sync::LazyLock,
};

use regex::Regex;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum InterpolationError {
    #[error("Required environment variable not found: {0} (set {0} or {0}_FILE)")]
    RequiredVarNotFound(String),

    #[error("Failed to read {var} file {}: {source}", .path.display())]
    SecretFile {
        var: String,
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("Invalid variable syntax: {0}")]
    InvalidSyntax(String),

    #[error("Recursive interpolation limit exceeded")]
    RecursionLimit,
}

pub type InterpolationResult<T> = Result<T, InterpolationError>;

const MAX_RECURSION_DEPTH: usize = 10;

static VAR_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\$\{([A-Za-z_][A-Za-z0-9_]*)(?::-([^}]*))?\}").expect("Invalid regex pattern")
});

pub fn interpolate(input: &str) -> InterpolationResult<String> {
    interpolate_with_depth(input, 0)
}

fn interpolate_with_depth(input: &str, depth: usize) -> InterpolationResult<String> {
    if depth > MAX_RECURSION_DEPTH {
        return Err(InterpolationError::RecursionLimit);
    }

    let mut result = String::with_capacity(input.len());
    let mut rest = input;

    while let Some(start) = rest.find("${") {
        result.push_str(&rest[..start]);
        let body = &rest[start + 2..];

        let Some((var_name, default, consumed)) = parse_reference(body) else {
            result.push_str("${");
            rest = body;
            continue;
        };

        match lookup_var(var_name)? {
            Some(value) => result.push_str(&value),
            None => match default {
                Some(def) => result.push_str(&interpolate_with_depth(def, depth + 1)?),
                None => {
                    return Err(InterpolationError::RequiredVarNotFound(
                        var_name.to_string(),
                    ));
                }
            },
        }

        rest = &body[consumed..];
    }

    result.push_str(rest);
    Ok(result)
}

fn lookup_var(name: &str) -> InterpolationResult<Option<String>> {
    if let Ok(value) = std::env::var(name) {
        return Ok(Some(value));
    }

    let file_var = format!("{name}_FILE");
    let Some(path) = std::env::var_os(&file_var).map(PathBuf::from) else {
        return Ok(None);
    };

    match std::fs::read_to_string(&path) {
        Ok(content) => Ok(Some(content.trim_end_matches(['\r', '\n']).to_string())),
        Err(source) => Err(InterpolationError::SecretFile {
            var: file_var,
            path,
            source,
        }),
    }
}

fn parse_reference(body: &str) -> Option<(&str, Option<&str>, usize)> {
    let name_len = body
        .char_indices()
        .find(|&(i, c)| !(c == '_' || c.is_ascii_alphabetic() || (i > 0 && c.is_ascii_digit())))
        .map_or(body.len(), |(i, _)| i);
    if name_len == 0 {
        return None;
    }

    let name = &body[..name_len];
    let after_name = &body[name_len..];

    if after_name.starts_with('}') {
        return Some((name, None, name_len + 1));
    }

    let default_body = after_name.strip_prefix(":-")?;
    let default_start = name_len + 2;
    let mut open = 0usize;
    let mut chars = default_body.char_indices().peekable();

    while let Some((i, c)) = chars.next() {
        match c {
            '$' if chars.peek().is_some_and(|&(_, next)| next == '{') => {
                chars.next();
                open += 1;
            }
            '}' if open == 0 => {
                return Some((name, Some(&default_body[..i]), default_start + i + 1));
            }
            '}' => open -= 1,
            _ => {}
        }
    }

    None
}

pub fn interpolate_toml(value: &mut toml::Value) -> InterpolationResult<()> {
    match value {
        toml::Value::String(s) => {
            *s = interpolate(s)?;
        }
        toml::Value::Array(arr) => {
            for item in arr {
                interpolate_toml(item)?;
            }
        }
        toml::Value::Table(table) => {
            for (_, v) in table.iter_mut() {
                interpolate_toml(v)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn has_variables(input: &str) -> bool {
    VAR_PATTERN.is_match(input)
}

pub fn extract_variable_names(input: &str) -> Vec<String> {
    VAR_PATTERN
        .captures_iter(input)
        .map(|cap| cap.get(1).unwrap().as_str().to_string())
        .collect()
}

#[derive(Debug, Clone)]
pub struct InterpolatedValue {
    pub value: String,
    pub original: Option<String>,
    pub resolved_vars: Vec<String>,
    pub defaulted_vars: Vec<String>,
}

impl InterpolatedValue {
    pub fn was_interpolated(&self) -> bool {
        self.original.is_some()
    }
}

pub fn interpolate_with_tracking(input: &str) -> InterpolationResult<InterpolatedValue> {
    let mut resolved_vars = Vec::new();
    let mut defaulted_vars = Vec::new();

    for cap in VAR_PATTERN.captures_iter(input) {
        let var_name = cap.get(1).unwrap().as_str().to_string();
        let has_default = cap.get(2).is_some();

        if std::env::var(&var_name).is_ok()
            || std::env::var_os(format!("{var_name}_FILE")).is_some()
        {
            resolved_vars.push(var_name);
        } else if has_default {
            defaulted_vars.push(var_name);
        }
    }

    let value = interpolate(input)?;
    let original = if value != input {
        Some(input.to_string())
    } else {
        None
    };

    Ok(InterpolatedValue {
        value,
        original,
        resolved_vars,
        defaulted_vars,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_env::EnvGuard;

    #[test]
    fn test_simple_var() {
        std::env::set_var("TEST_VAR_SIMPLE", "hello");
        let result = interpolate("${TEST_VAR_SIMPLE}").unwrap();
        assert_eq!(result, "hello");
        std::env::remove_var("TEST_VAR_SIMPLE");
    }

    #[test]
    fn test_var_with_text() {
        std::env::set_var("TEST_VAR_TEXT", "world");
        let result = interpolate("Hello ${TEST_VAR_TEXT}!").unwrap();
        assert_eq!(result, "Hello world!");
        std::env::remove_var("TEST_VAR_TEXT");
    }

    #[test]
    fn test_multiple_vars() {
        std::env::set_var("TEST_VAR_A", "foo");
        std::env::set_var("TEST_VAR_B", "bar");
        let result = interpolate("${TEST_VAR_A}-${TEST_VAR_B}").unwrap();
        assert_eq!(result, "foo-bar");
        std::env::remove_var("TEST_VAR_A");
        std::env::remove_var("TEST_VAR_B");
    }

    #[test]
    fn test_missing_var_error() {
        let result = interpolate("${THIS_VAR_DOES_NOT_EXIST_12345}");
        assert!(matches!(
            result,
            Err(InterpolationError::RequiredVarNotFound(_))
        ));
    }

    #[test]
    fn test_default_value() {
        let result = interpolate("${NONEXISTENT_VAR_123:-default_value}").unwrap();
        assert_eq!(result, "default_value");
    }

    #[test]
    fn test_empty_default() {
        let result = interpolate("prefix${NONEXISTENT_VAR_456:-}suffix").unwrap();
        assert_eq!(result, "prefixsuffix");
    }

    #[test]
    fn test_var_overrides_default() {
        std::env::set_var("TEST_VAR_OVERRIDE", "actual");
        let result = interpolate("${TEST_VAR_OVERRIDE:-default}").unwrap();
        assert_eq!(result, "actual");
        std::env::remove_var("TEST_VAR_OVERRIDE");
    }

    #[test]
    fn test_nested_default() {
        std::env::set_var("TEST_NESTED_INNER", "inner_value");
        let result = interpolate("${NONEXISTENT:-${TEST_NESTED_INNER}}").unwrap();
        assert_eq!(result, "inner_value");
        std::env::remove_var("TEST_NESTED_INNER");
    }

    #[test]
    fn test_no_interpolation() {
        let result = interpolate("plain text").unwrap();
        assert_eq!(result, "plain text");
    }

    #[test]
    fn test_has_variables() {
        assert!(has_variables("${VAR}"));
        assert!(has_variables("prefix${VAR}suffix"));
        assert!(has_variables("${VAR:-default}"));
        assert!(!has_variables("plain text"));
        assert!(!has_variables("$VAR")); // Missing braces
    }

    #[test]
    fn test_extract_variable_names() {
        let names = extract_variable_names("${FOO}-${BAR:-default}");
        assert_eq!(names, vec!["FOO", "BAR"]);
    }

    #[test]
    fn test_interpolate_with_tracking() {
        std::env::set_var("TEST_TRACKED", "value");
        let result = interpolate_with_tracking("${TEST_TRACKED} ${MISSING:-default}").unwrap();

        assert!(result.was_interpolated());
        assert_eq!(result.value, "value default");
        assert!(result.resolved_vars.contains(&"TEST_TRACKED".to_string()));
        assert!(result.defaulted_vars.contains(&"MISSING".to_string()));
        std::env::remove_var("TEST_TRACKED");
    }

    #[test]
    fn test_interpolate_toml() {
        std::env::set_var("TEST_TOML_VAR", "toml_value");

        let toml_str = r#"
            key = "${TEST_TOML_VAR}"
            nested = { inner = "${TEST_TOML_VAR:-fallback}" }
            array = ["${TEST_TOML_VAR}", "static"]
        "#;

        let mut value: toml::Value = toml::from_str(toml_str).unwrap();
        interpolate_toml(&mut value).unwrap();

        assert_eq!(value["key"].as_str().unwrap(), "toml_value");
        assert_eq!(value["nested"]["inner"].as_str().unwrap(), "toml_value");
        assert_eq!(value["array"][0].as_str().unwrap(), "toml_value");
        assert_eq!(value["array"][1].as_str().unwrap(), "static");

        std::env::remove_var("TEST_TOML_VAR");
    }

    #[test]
    fn test_complex_address() {
        std::env::set_var("TEST_HOST", "localhost");
        std::env::set_var("TEST_PORT", "9000");

        let result = interpolate("${TEST_HOST:-127.0.0.1}:${TEST_PORT:-8080}").unwrap();
        assert_eq!(result, "localhost:9000");

        std::env::remove_var("TEST_HOST");
        std::env::remove_var("TEST_PORT");

        let result = interpolate("${TEST_HOST:-127.0.0.1}:${TEST_PORT:-8080}").unwrap();
        assert_eq!(result, "127.0.0.1:8080");
    }

    #[test]
    fn test_env_values_are_inserted_verbatim() {
        std::env::set_var("TEST_VERBATIM_SET", "expanded");
        std::env::set_var("TEST_VERBATIM_A", "tok${TEST_VERBATIM_SET}en");
        std::env::set_var("TEST_VERBATIM_B", "pa${TEST_VERBATIM_UNSET}ss");
        std::env::set_var("TEST_VERBATIM_SELF", "x${TEST_VERBATIM_SELF}");

        assert_eq!(
            interpolate("${TEST_VERBATIM_A}").unwrap(),
            "tok${TEST_VERBATIM_SET}en"
        );
        assert_eq!(
            interpolate("${TEST_VERBATIM_B}").unwrap(),
            "pa${TEST_VERBATIM_UNSET}ss"
        );
        assert_eq!(
            interpolate("${TEST_VERBATIM_SELF}").unwrap(),
            "x${TEST_VERBATIM_SELF}"
        );
    }

    #[test]
    fn test_file_fallback_when_var_unset() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("postgres_url");
        std::fs::write(
            &path,
            "postgres://app:pa${NOT_EXPANDED}ss@db/pipedash\r\n\n",
        )
        .unwrap();
        let _env = EnvGuard::set(&[
            ("TEST_SECRET_URL", None),
            ("TEST_SECRET_URL_FILE", path.to_str()),
        ]);

        assert_eq!(
            interpolate("${TEST_SECRET_URL:-sqlite}").unwrap(),
            "postgres://app:pa${NOT_EXPANDED}ss@db/pipedash"
        );
    }

    #[test]
    fn test_env_var_wins_over_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token");
        std::fs::write(&path, "from-file\n").unwrap();
        let _env = EnvGuard::set(&[
            ("TEST_SECRET_TOKEN", Some("from-env")),
            ("TEST_SECRET_TOKEN_FILE", path.to_str()),
        ]);

        assert_eq!(interpolate("${TEST_SECRET_TOKEN}").unwrap(), "from-env");
    }

    #[test]
    fn test_unreadable_file_fails_instead_of_default() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing");
        let _env = EnvGuard::set(&[
            ("TEST_SECRET_MISSING", None),
            ("TEST_SECRET_MISSING_FILE", missing.to_str()),
        ]);

        let err = interpolate("${TEST_SECRET_MISSING:-fallback}").unwrap_err();
        assert!(matches!(
            &err,
            InterpolationError::SecretFile { var, path, .. }
                if var == "TEST_SECRET_MISSING_FILE" && path == &missing
        ));
    }
}
