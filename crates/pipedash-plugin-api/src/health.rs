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

#[cfg(test)]
mod tests {
    use super::*;

    fn source_for(url: &str) -> PluginResult<Option<StatusSource>> {
        let mut config = HashMap::new();
        config.insert(STATUS_PAGE_URL_KEY.to_string(), url.to_string());
        custom_status_source(&config)
    }

    #[test]
    fn health_levels_order_by_severity() {
        assert!(HealthLevel::Operational < HealthLevel::Maintenance);
        assert!(HealthLevel::Maintenance < HealthLevel::Degraded);
        assert!(HealthLevel::Degraded < HealthLevel::PartialOutage);
        assert!(HealthLevel::PartialOutage < HealthLevel::MajorOutage);
    }

    #[test]
    fn custom_source_is_absent_when_unset_or_blank() {
        assert_eq!(custom_status_source(&HashMap::new()).unwrap(), None);
        assert_eq!(source_for("   ").unwrap(), None);
    }

    #[test]
    fn custom_source_accepts_https_and_normalizes_trailing_slash() {
        let source = source_for("https://status.example.com/team")
            .unwrap()
            .unwrap();
        assert_eq!(source.name, "status.example.com");
        assert_eq!(source.page_url.as_str(), "https://status.example.com/team/");
        assert_eq!(
            source.feed_url.as_str(),
            "https://status.example.com/team/api/v2/summary.json"
        );
        assert_eq!(source.format, FeedFormat::Statuspage);
        assert_eq!(source.components, ComponentFilter::All);

        let same = source_for("https://status.example.com/team/")
            .unwrap()
            .unwrap();
        assert_eq!(same.feed_url, source.feed_url);
    }

    #[test]
    fn custom_source_accepts_literal_loopback_http() {
        let v4 = source_for("http://127.0.0.1:8765").unwrap().unwrap();
        assert_eq!(
            v4.feed_url.as_str(),
            "http://127.0.0.1:8765/api/v2/summary.json"
        );
        assert!(source_for("http://[::1]:8765/").unwrap().is_some());
    }

    #[test]
    fn custom_source_rejects_unsafe_urls() {
        for url in [
            "https://user:pass@status.example.com",
            "https://user@status.example.com",
            "https://status.example.com/?x=1",
            "https://status.example.com/#frag",
            "javascript:alert(1)",
            "http://10.0.0.1",
            "http://169.254.169.254",
            "http://localhost:8765",
            "http://status.example.com",
            "https://",
            "file:///etc/passwd",
            "not a url",
        ] {
            assert!(
                matches!(source_for(url), Err(PluginError::InvalidConfig(_))),
                "{url} should be rejected"
            );
        }
    }
}
