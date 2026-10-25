use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HapticKind {
    Selection,
    Light,
    Medium,
    Heavy,
    Soft,
    Rigid,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    Light,
    Dark,
    System,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Accessibility {
    pub reduce_transparency: bool,
    pub increase_contrast: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyCover {
    pub armed: bool,
    pub hold_for_frontend: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NotificationResponse {
    pub action_id: String,
    pub key: Option<String>,
    pub pipeline_id: Option<String>,
    pub provider_id: Option<String>,
    pub run_number: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn haptic_kinds_and_appearances_use_the_lowercase_names_the_frontend_sends() {
        assert_eq!(
            serde_json::from_value::<HapticKind>(json!("selection")).unwrap(),
            HapticKind::Selection
        );
        assert_eq!(
            serde_json::from_value::<HapticKind>(json!("rigid")).unwrap(),
            HapticKind::Rigid
        );
        assert!(serde_json::from_value::<HapticKind>(json!("buzz")).is_err());
        assert_eq!(
            serde_json::from_value::<Appearance>(json!("system")).unwrap(),
            Appearance::System
        );
        assert!(serde_json::from_value::<Appearance>(json!("auto")).is_err());
    }

    #[test]
    fn accessibility_flags_serialize_in_camel_case() {
        assert_eq!(
            serde_json::to_value(Accessibility {
                reduce_transparency: true,
                increase_contrast: false,
            })
            .unwrap(),
            json!({ "reduceTransparency": true, "increaseContrast": false })
        );
    }

    #[test]
    fn privacy_cover_arguments_match_the_swift_decoder() {
        assert_eq!(
            serde_json::to_value(PrivacyCover {
                armed: true,
                hold_for_frontend: false,
            })
            .unwrap(),
            json!({ "armed": true, "holdForFrontend": false })
        );
    }

    #[test]
    fn a_notification_response_tolerates_missing_fields() {
        let response: NotificationResponse = serde_json::from_value(json!({
            "actionId": "open-run",
            "pipelineId": "github__1__a/b/c.yml",
            "runNumber": "7",
        }))
        .unwrap();

        assert_eq!(response.action_id, "open-run");
        assert_eq!(
            response.pipeline_id.as_deref(),
            Some("github__1__a/b/c.yml")
        );
        assert_eq!(response.run_number.as_deref(), Some("7"));
        assert_eq!(response.key, None);
        assert_eq!(response.provider_id, None);
    }
}
