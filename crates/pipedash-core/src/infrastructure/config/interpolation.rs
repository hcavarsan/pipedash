use std::path::PathBuf;

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

    #[error("Recursive interpolation limit exceeded")]
    RecursionLimit,
}

pub type InterpolationResult<T> = Result<T, InterpolationError>;

const MAX_RECURSION_DEPTH: usize = 10;

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

fn references(input: &str) -> impl Iterator<Item = &str> {
    let mut rest = input;
    std::iter::from_fn(move || {
        while let Some(start) = rest.find("${") {
            let body = &rest[start + 2..];
            let Some((name, _, consumed)) = parse_reference(body) else {
                rest = body;
                continue;
            };
            rest = &body[consumed..];
            return Some(name);
        }
        None
    })
}

pub fn has_variables(input: &str) -> bool {
    references(input).next().is_some()
}

#[cfg(test)]
mod tests;
