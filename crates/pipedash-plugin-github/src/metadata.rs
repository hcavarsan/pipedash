//! GitHub Actions plugin metadata
//!
//! This module contains plugin metadata, configuration schema, and
//! capabilities.

use pipedash_plugin_api::*;

use crate::schema;

/// Creates the plugin metadata for GitHub Actions
///
/// This includes:
/// - Basic plugin information (name, version, description)
/// - Configuration schema (API token field)
/// - Table schema (from schema module)
/// - Plugin capabilities
/// - Required permissions
/// - Available features
pub fn create_metadata() -> PluginMetadata {
    PluginMetadata {
        name: "GitHub Actions".to_string(),
        provider_type: "github".to_string(),
        version: "0.1.0".to_string(),
        description: "Monitor and trigger GitHub Actions workflows".to_string(),
        author: Some("Pipedash Team".to_string()),
        icon: Some("https://cdn.simpleicons.org/github/white".to_string()),
        config_schema: create_config_schema(),
        table_schema: schema::create_table_schema(),
        capabilities: create_capabilities(),
        required_permissions: create_required_permissions(),
        features: create_features(),
    }
}

/// Creates the configuration schema for GitHub Actions
///
/// Defines a single required field:
/// - `token`: GitHub Personal Access Token or Fine-grained token
fn create_config_schema() -> ConfigSchema {
    ConfigSchema::new().add_field(ConfigField {
        key: "token".to_string(),
        label: "API Token".to_string(),
        description: Some(
            "GitHub Personal Access Token or Fine-grained token with repo and workflow permissions"
                .to_string(),
        ),
        field_type: ConfigFieldType::Password,
        required: true,
        default_value: None,
        options: None,
        validation_regex: None,
        validation_message: None,
    })
}

/// Creates the plugin capabilities
///
/// GitHub Actions supports:
/// - Pipelines (workflows)
/// - Pipeline runs (workflow runs)
/// - Triggering workflows
fn create_capabilities() -> PluginCapabilities {
    PluginCapabilities {
        pipelines: true,
        pipeline_runs: true,
        trigger: true,
        agents: false,
        artifacts: false,
        queues: false,
        custom_tables: false,
    }
}

/// Creates the required permissions for GitHub Actions
///
/// **Required (Base Level):**
/// - `repo` OR `public_repo`: Repository access for viewing workflows and runs
///   - `repo`: Full access to private and public repositories
///   - `public_repo`: Access to public repositories only
///
/// **Optional (Enhanced Features):**
/// - `workflow`: Enables triggering and canceling workflows (write access)
/// - `read:org`: Enables organization filtering (without it, only personal
///   repos available)
///
/// For classic Personal Access Tokens:
/// - `repo` or `public_repo`: Required for workflow viewing
///   - `repo` gives full access (private + public repos)
///   - `public_repo` gives public-only access
/// - `workflow`: Optional, only for trigger/cancel features
/// - `read:org`: Optional, only for organization filtering
///
/// For fine-grained tokens:
/// - Repository: Metadata (Read) - required
/// - Actions: Read - required for viewing
/// - Actions: Write - optional, only for trigger/cancel
/// - Organization: Members (Read) - optional, only for org filtering
///
/// Note: The permission checker accepts higher-level scopes.
/// For example, `admin:org` satisfies the `read:org` requirement.
fn create_required_permissions() -> Vec<Permission> {
    vec![
        Permission {
            name: "repo".to_string(),
            description: "Repository access - 'repo' scope for private repositories, or 'public_repo' scope for public repositories only".to_string(),
            required: true,
        },
        Permission {
            name: "workflow".to_string(),
            description: "Write to GitHub Actions API - dispatch workflow events & cancel runs. Optional for read-only.".to_string(),
            required: false,
        },
        Permission {
            name: "read:org".to_string(),
            description: "List organizations - for filtering repos by org. Optional, only needed for org features.".to_string(),
            required: false,
        },
    ]
}

/// Creates the feature list for GitHub Actions
///
/// Features are mapped to specific permission combinations:
///
/// **Base Features (repo only):**
/// - View workflows: List all workflows in configured repositories
/// - View runs: See workflow run history, status, and details
///
/// **Enhanced Features (repo + workflow):**
/// - Trigger workflows: Manually start workflow runs with parameters
/// - Cancel runs: Stop running or queued workflows
///
/// **Organization Feature (read:org):**
/// - Filter by organization: View and filter repositories by org membership
fn create_features() -> Vec<Feature> {
    vec![
        Feature {
            id: "list_monitor_workflows".to_string(),
            name: "List & monitor workflows".to_string(),
            description: "GET /repos/{owner}/{repo}/actions/workflows - View all workflows in your repos".to_string(),
            required_permissions: vec!["repo".to_string()],
        },
        Feature {
            id: "view_run_history".to_string(),
            name: "View run history & logs".to_string(),
            description: "GET /repos/{owner}/{repo}/actions/workflows/{id}/runs - See run status, logs, commit info".to_string(),
            required_permissions: vec!["repo".to_string()],
        },
        Feature {
            id: "monitor_status".to_string(),
            name: "Monitor pipeline status".to_string(),
            description: "Real-time status updates for workflow runs in your dashboard".to_string(),
            required_permissions: vec!["repo".to_string()],
        },
        Feature {
            id: "trigger_dispatch".to_string(),
            name: "Trigger workflow dispatch".to_string(),
            description: "POST /repos/{owner}/{repo}/actions/workflows/{id}/dispatches - Start workflows manually".to_string(),
            required_permissions: vec!["workflow".to_string()],
        },
        Feature {
            id: "cancel_workflows".to_string(),
            name: "Cancel running workflows".to_string(),
            description: "POST /repos/{owner}/{repo}/actions/runs/{id}/cancel - Stop queued or running workflows".to_string(),
            required_permissions: vec!["workflow".to_string()],
        },
        Feature {
            id: "filter_by_org".to_string(),
            name: "Filter repos by organization".to_string(),
            description: "List and filter repositories by organization. Without this permission, you can still access all your personal repositories.".to_string(),
            required_permissions: vec!["read:org".to_string()],
        },
    ]
}
