use std::collections::HashMap;

use reqwest::Url;
use serde::{
    Deserialize,
    Serialize,
};

use crate::error::{
    PluginError,
    PluginResult,
};
use crate::schema::{
    ConfigField,
    ConfigFieldType,
    ConfigSection,
};

pub const STATUS_PAGE_URL_KEY: &str = "status_page_url";
pub const CONTROLLER_HEALTH_FEATURE: &str = "controller_health";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum HealthLevel {
    Operational,
    Maintenance,
    Degraded,
    PartialOutage,
    MajorOutage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentHealth {
    pub name: String,
    pub level: HealthLevel,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ComponentFilter {
    All,
    Ids(Vec<String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FeedFormat {
    Statuspage,
    StatusIo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusSource {
    pub name: String,
    pub page_url: Url,
    pub feed_url: Url,
    pub format: FeedFormat,
    pub components: ComponentFilter,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClientOptions {
    pub origin: Option<Url>,
    pub accept_invalid_certs: bool,
    pub same_host_redirects: bool,
}

pub fn status_page_field() -> ConfigField {
    ConfigField {
        key: STATUS_PAGE_URL_KEY.to_string(),
        label: "Status page URL (optional)".to_string(),
        description: Some(
            "Statuspage address for this instance, for example https://status.example.com. \
             Pipedash reads its /api/v2/summary.json."
                .to_string(),
        ),
        field_type: ConfigFieldType::Text,
        required: false,
        default_value: None,
        options: None,
        validation_regex: Some(
            r"^(https://[^\s/?#@]+|http://(127\.0\.0\.1|\[::1\])(:[0-9]+)?)(/[^\s?#]*)?$"
                .to_string(),
        ),
        validation_message: Some(
            "Must be an https URL without credentials, query or fragment".to_string(),
        ),
        section: ConfigSection::Settings,
        auth_modes: Vec::new(),
    }
}

pub fn custom_status_source(
    config: &HashMap<String, String>,
) -> PluginResult<Option<StatusSource>> {
    let Some(raw) = config
        .get(STATUS_PAGE_URL_KEY)
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
    else {
        return Ok(None);
    };

    let invalid = || {
        PluginError::InvalidConfig(
            "status_page_url must be an https URL with a host and no credentials, query or \
             fragment"
                .to_string(),
        )
    };

    let mut page_url = Url::parse(raw).map_err(|_| invalid())?;
    let allowed_scheme = match page_url.scheme() {
        "https" => true,
        "http" => is_literal_loopback(&page_url),
        _ => false,
    };
    if !allowed_scheme
        || page_url.host_str().is_none_or(str::is_empty)
        || !page_url.username().is_empty()
        || page_url.password().is_some()
        || page_url.query().is_some()
        || page_url.fragment().is_some()
    {
        return Err(invalid());
    }

    if !page_url.path().ends_with('/') {
        let path = format!("{}/", page_url.path());
        page_url.set_path(&path);
    }

    let feed_url = page_url
        .join("api/v2/summary.json")
        .map_err(|_| invalid())?;
    let name = page_url.host_str().unwrap_or_default().to_string();

    Ok(Some(StatusSource {
        name,
        page_url,
        feed_url,
        format: FeedFormat::Statuspage,
        components: ComponentFilter::All,
    }))
}

fn is_literal_loopback(url: &Url) -> bool {
    matches!(url.host_str(), Some("127.0.0.1" | "[::1]"))
}

pub fn deployment_health(
    name: &str, desired: Option<impl Into<i64>>, available: Option<impl Into<i64>>,
    unavailable: bool,
) -> Option<ComponentHealth> {
    let (desired, available) = (desired.map(Into::into), available.map(Into::into));
    let (level, detail) = match (desired, available) {
        (Some(0), _) => (HealthLevel::MajorOutage, "scaled to zero".to_string()),
        (_, Some(0)) => (HealthLevel::MajorOutage, "not running".to_string()),
        (Some(desired), Some(available)) if available < desired => (
            HealthLevel::Degraded,
            format!("{available}/{desired} replicas available"),
        ),
        (Some(_), Some(_)) => return None,
        _ if unavailable => (HealthLevel::MajorOutage, "not available".to_string()),
        _ => return None,
    };

    Some(ComponentHealth {
        name: name.to_string(),
        level,
        detail: Some(detail),
    })
}

pub fn collect_controller_health(
    checks: Vec<PluginResult<Option<ComponentHealth>>>,
) -> PluginResult<Vec<ComponentHealth>> {
    let mut components = Vec::new();
    let mut denied = None;
    let mut missing = None;
    let total = checks.len();
    let mut missing_count = 0;

    for check in checks {
        match check {
            Ok(component) => components.extend(component),
            Err(PluginError::PermissionDenied(message)) => {
                denied.get_or_insert(message);
            }
            Err(PluginError::PipelineNotFound(message)) => {
                missing_count += 1;
                missing.get_or_insert(message);
            }
            Err(error) => return Err(error),
        }
    }

    if let Some(message) = denied {
        return Err(PluginError::PermissionDenied(message));
    }
    match missing {
        Some(message) if missing_count == total => Err(PluginError::PipelineNotFound(message)),
        _ => Ok(components),
    }
}

#[cfg(test)]
mod tests;
