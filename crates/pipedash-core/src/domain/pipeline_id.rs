use super::error::{
    DomainError,
    DomainResult,
};

pub const SEPARATOR: &str = "__";

pub fn compose(provider_type: &str, provider_id: i64, local: &str) -> String {
    format!("{provider_type}{SEPARATOR}{provider_id}{SEPARATOR}{local}")
}

pub fn strip<'a>(provider_type: &str, provider_id: i64, id: &'a str) -> DomainResult<&'a str> {
    let prefix = compose(provider_type, provider_id, "");
    id.strip_prefix(prefix.as_str())
        .filter(|local| !local.is_empty())
        .ok_or_else(|| {
            DomainError::InvalidConfig(format!(
                "Invalid pipeline ID format: '{id}'. Expected format: '{prefix}<id>'"
            ))
        })
}

#[cfg(test)]
mod tests;
