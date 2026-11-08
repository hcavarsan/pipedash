use pipedash_plugin_api::*;

use super::GitHubPlugin;

const FINE_GRAINED_FEATURE_MAPPINGS: &[(&str, &[&str])] = &[
    ("list_monitor_workflows", &["Repository Metadata"]),
    ("view_run_history", &["Repository Metadata"]),
    ("view_run_logs", &["Actions (Read)"]),
    ("monitor_status", &["Actions (Read)"]),
    ("trigger_dispatch", &["Actions (Write)", "Contents (Read)"]),
    ("cancel_workflows", &["Actions (Write)"]),
    ("rerun_failed_jobs", &["Actions (Write)"]),
    ("enable_workflows", &["Actions (Write)"]),
    (
        "access_org_repos",
        &["Organization members and teams (Read)"],
    ),
];

fn map_feature_permissions(
    token_type: &str, feature_id: &str, classic_permissions: &[String],
) -> Vec<String> {
    match token_type {
        "fine_grained" => FINE_GRAINED_FEATURE_MAPPINGS
            .iter()
            .find(|(id, _)| *id == feature_id)
            .map(|(_, permissions)| {
                permissions
                    .iter()
                    .map(|permission| permission.to_string())
                    .collect()
            })
            .unwrap_or_default(),
        _ => classic_permissions.to_vec(),
    }
}

impl GitHubPlugin {
    pub(super) fn get_feature_availability_impl(
        &self, status: &PermissionStatus,
    ) -> Vec<FeatureAvailability> {
        let features = &self.metadata().features;

        let token_type = status
            .metadata
            .get("token_type")
            .map(|s| s.as_str())
            .unwrap_or("classic_pat");

        let granted_perms: std::collections::HashSet<String> = status
            .permissions
            .iter()
            .filter(|p| p.granted)
            .map(|p| p.permission.name.clone())
            .collect();

        features
            .iter()
            .map(|feature| {
                let mapped_required =
                    map_feature_permissions(token_type, &feature.id, &feature.required_permissions);

                let missing: Vec<String> = mapped_required
                    .iter()
                    .filter(|perm| !granted_perms.contains(*perm))
                    .cloned()
                    .collect();

                let transformed_feature = Feature {
                    id: feature.id.clone(),
                    name: feature.name.clone(),
                    description: feature.description.clone(),
                    required_permissions: mapped_required,
                };

                FeatureAvailability {
                    feature: transformed_feature,
                    available: missing.is_empty(),
                    missing_permissions: missing,
                }
            })
            .collect()
    }
}
