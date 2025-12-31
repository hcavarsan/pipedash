use std::collections::HashMap;

use super::{
    DomainError,
    DomainResult,
};

const MAX_KEY_LENGTH: usize = 256;

const MAX_VALUE_LENGTH: usize = 4096;

const MAX_NAME_LENGTH: usize = 128;

const MAX_PIPELINE_ID_LENGTH: usize = 512;

pub fn validate_config(config: &HashMap<String, String>) -> DomainResult<()> {
    for (key, value) in config {
        validate_config_key(key)?;
        validate_config_value(key, value)?;
    }
    Ok(())
}

fn validate_config_key(key: &str) -> DomainResult<()> {
    if key.is_empty() {
        return Err(DomainError::InvalidConfig(
            "Config key cannot be empty".to_string(),
        ));
    }

    if key.len() > MAX_KEY_LENGTH {
        return Err(DomainError::InvalidConfig(format!(
            "Config key '{}...' exceeds maximum length of {} characters",
            &key[..32.min(key.len())],
            MAX_KEY_LENGTH
        )));
    }

    if !key
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
        return Err(DomainError::InvalidConfig(format!(
            "Config key '{}' contains invalid characters (only alphanumeric, underscore, hyphen allowed)",
            key
        )));
    }

    Ok(())
}

fn validate_config_value(key: &str, value: &str) -> DomainResult<()> {
    if value.len() > MAX_VALUE_LENGTH {
        return Err(DomainError::InvalidConfig(format!(
            "Value for key '{}' exceeds maximum length of {} characters",
            key, MAX_VALUE_LENGTH
        )));
    }
    Ok(())
}

pub fn validate_provider_name(name: &str) -> DomainResult<()> {
    if name.is_empty() {
        return Err(DomainError::InvalidConfig(
            "Provider name cannot be empty".to_string(),
        ));
    }

    if name.len() > MAX_NAME_LENGTH {
        return Err(DomainError::InvalidConfig(format!(
            "Provider name exceeds maximum length of {} characters",
            MAX_NAME_LENGTH
        )));
    }

    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || c == ' ' || c == '_' || c == '-')
    {
        return Err(DomainError::InvalidConfig(
            "Provider name contains invalid characters".to_string(),
        ));
    }

    Ok(())
}

pub fn validate_provider_type(provider_type: &str) -> DomainResult<()> {
    const VALID_TYPES: &[&str] = &["github", "gitlab", "bitbucket", "circleci", "azure"];

    if !VALID_TYPES.contains(&provider_type.to_lowercase().as_str()) {
        return Err(DomainError::InvalidProviderType(format!(
            "Unknown provider type '{}'. Valid types: {:?}",
            provider_type, VALID_TYPES
        )));
    }

    Ok(())
}

pub fn validate_pipeline_id(pipeline_id: &str) -> DomainResult<()> {
    if pipeline_id.is_empty() {
        return Err(DomainError::InvalidConfig(
            "Pipeline ID cannot be empty".to_string(),
        ));
    }

    if pipeline_id.len() > MAX_PIPELINE_ID_LENGTH {
        return Err(DomainError::InvalidConfig(format!(
            "Pipeline ID exceeds maximum length of {} characters",
            MAX_PIPELINE_ID_LENGTH
        )));
    }

    Ok(())
}

pub fn validate_trigger_params(
    workflow_id: &str, inputs: &Option<HashMap<String, String>>,
) -> DomainResult<()> {
    validate_pipeline_id(workflow_id)?;

    if let Some(inputs) = inputs {
        validate_config(inputs)?;
    }

    Ok(())
}

pub fn validate_pagination(page: usize, page_size: usize) -> DomainResult<()> {
    if page == 0 {
        return Err(DomainError::InvalidConfig(
            "Page number must be at least 1".to_string(),
        ));
    }

    if page_size == 0 {
        return Err(DomainError::InvalidConfig(
            "Page size must be at least 1".to_string(),
        ));
    }

    if page_size > 1000 {
        return Err(DomainError::InvalidConfig(
            "Page size cannot exceed 1000".to_string(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_config_valid() {
        let mut config = HashMap::new();
        config.insert("owner".to_string(), "my-org".to_string());
        config.insert("repo".to_string(), "my-repo".to_string());
        config.insert("base_url".to_string(), "https://github.com".to_string());

        assert!(validate_config(&config).is_ok());
    }

    #[test]
    fn test_validate_config_empty_key() {
        let mut config = HashMap::new();
        config.insert("".to_string(), "value".to_string());

        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn test_validate_config_key_too_long() {
        let mut config = HashMap::new();
        config.insert("a".repeat(300), "value".to_string());

        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn test_validate_config_value_too_long() {
        let mut config = HashMap::new();
        config.insert("key".to_string(), "a".repeat(5000));

        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn test_validate_config_invalid_key_chars() {
        let mut config = HashMap::new();
        config.insert("key!@#".to_string(), "value".to_string());

        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn test_validate_provider_name_valid() {
        assert!(validate_provider_name("My GitHub Provider").is_ok());
        assert!(validate_provider_name("github-actions").is_ok());
        assert!(validate_provider_name("gitlab_ci").is_ok());
    }

    #[test]
    fn test_validate_provider_name_empty() {
        assert!(validate_provider_name("").is_err());
    }

    #[test]
    fn test_validate_provider_type_valid() {
        assert!(validate_provider_type("github").is_ok());
        assert!(validate_provider_type("GitHub").is_ok());
        assert!(validate_provider_type("gitlab").is_ok());
        assert!(validate_provider_type("bitbucket").is_ok());
    }

    #[test]
    fn test_validate_provider_type_invalid() {
        assert!(validate_provider_type("unknown").is_err());
        assert!(validate_provider_type("jenkins").is_err());
    }

    #[test]
    fn test_validate_pagination_valid() {
        assert!(validate_pagination(1, 20).is_ok());
        assert!(validate_pagination(100, 50).is_ok());
    }

    #[test]
    fn test_validate_pagination_invalid() {
        assert!(validate_pagination(0, 20).is_err());
        assert!(validate_pagination(1, 0).is_err());
        assert!(validate_pagination(1, 2000).is_err());
    }
}
