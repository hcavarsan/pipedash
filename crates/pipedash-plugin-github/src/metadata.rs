use pipedash_plugin_api::*;

use crate::{
    presentation,
    schema,
};

pub fn create_metadata() -> PluginMetadata {
    PluginMetadata {
        presentation: presentation::presentation(),
        token_prefixes: ["ghp_", "gho_", "ghu_", "ghs_", "github_pat_"]
            .map(str::to_string)
            .to_vec(),
        ..PluginMetadata::new(
            "GitHub Actions",
            create_config_schema(),
            schema::create_table_schema(),
            PluginCapabilities {
                force_cancel: true,
                ..PluginCapabilities::standard()
            },
        )
        .with_permissions(create_required_permissions())
        .with_features(create_features())
    }
}

fn create_config_schema() -> ConfigSchema {
    ConfigSchema::new()
        .add_field(ConfigField::token(
            "API Token",
            "Classic PAT: repo (private repositories) or public_repo (public only), plus workflow. Fine-grained token: Actions read/write and Contents read on each selected repository.",
        ))
        .add_field(status_page_field())
}

fn create_required_permissions() -> Vec<Permission> {
    vec![
        Permission::new(
            "repo",
            "Repository access - 'repo' scope for private repositories, or 'public_repo' scope for public repositories only",
            true,
        ),
        Permission::new(
            "workflow",
            "Write to GitHub Actions API - dispatch workflow events & cancel runs. Optional for read-only.",
            false,
        ),
        Permission::new(
            "read:org",
            "List organizations - for filtering repos by org. Optional, only needed for org features.",
            false,
        ),
    ]
}

fn create_features() -> Vec<Feature> {
    vec![
        Feature::new(
            "list_monitor_workflows",
            "List & monitor workflows",
            "GET /repos/{owner}/{repo}/actions/workflows - View all workflows in your repos",
            &["repo"],
        ),
        Feature::new(
            "view_run_history",
            "View run history & logs",
            "GET /repos/{owner}/{repo}/actions/workflows/{id}/runs - See run status, logs, commit info",
            &["repo"],
        ),
        Feature::new(
            "view_run_logs",
            "Read job logs in Pipedash",
            "GET /repos/{owner}/{repo}/actions/runs/{id}/jobs and /actions/jobs/{id}/logs - Read and search job logs without leaving Pipedash",
            &["repo"],
        ),
        Feature::new(
            "monitor_status",
            "Monitor pipeline status",
            "Real-time status updates for workflow runs in your dashboard",
            &["repo"],
        ),
        Feature::new(
            "trigger_dispatch",
            "Trigger workflow dispatch",
            "POST /repos/{owner}/{repo}/actions/workflows/{id}/dispatches - Start workflows manually",
            &["workflow"],
        ),
        Feature::new(
            "cancel_workflows",
            "Cancel running workflows",
            "POST /repos/{owner}/{repo}/actions/runs/{id}/cancel - Stop queued or running workflows",
            &["workflow"],
        ),
        Feature::new(
            "rerun_failed_jobs",
            "Re-run runs",
            "POST /repos/{owner}/{repo}/actions/runs/{id}/rerun and /rerun-failed-jobs - Re-run a whole run, or only its failed jobs and the jobs that depend on them",
            &["repo"],
        ),
        Feature::new(
            "enable_workflows",
            "Enable disabled workflows",
            "PUT /repos/{owner}/{repo}/actions/workflows/{id}/enable - Turn a workflow back on after GitHub or someone on the repository disables it",
            &["workflow"],
        ),
        Feature::new(
            "filter_by_org",
            "Filter repos by organization",
            "List and filter repositories by organization. Without this permission, you can still access all your personal repositories.",
            &["read:org"],
        ),
        Feature::new(
            "access_org_repos",
            "Access organization repositories",
            "View and select repositories from organizations. Without this permission, only your personal repositories will be shown in the pipeline selection.",
            &["read:org"],
        ),
    ]
}
