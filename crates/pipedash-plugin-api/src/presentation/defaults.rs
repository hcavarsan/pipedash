use super::{
    ActionCopy,
    Notice,
    PipelineTerminology,
    PluginPresentation,
    StatusOption,
    ViewCopy,
    WizardCopy,
};
use crate::types::PipelineStatus;

fn notice(title: &str, message: &str) -> Notice {
    Notice {
        title: title.to_string(),
        message: message.to_string(),
    }
}

impl Default for PipelineTerminology {
    fn default() -> Self {
        Self {
            item: "pipeline".to_string(),
            items: "pipelines".to_string(),
            group: "organization".to_string(),
            groups: "organizations".to_string(),
            group_title: "Organization".to_string(),
            source_title: "Repository".to_string(),
            sources_title: "Repositories".to_string(),
            history: "Run history".to_string(),
            trigger: "Trigger".to_string(),
            run: "run".to_string(),
            runs: "runs".to_string(),
            organization_label: "Organization".to_string(),
            section_count_label: "Workflows".to_string(),
        }
    }
}

impl Default for ViewCopy {
    fn default() -> Self {
        Self {
            title: "Pipelines".to_string(),
            list_title: "All pipelines".to_string(),
            search_placeholder: "Search repositories, workflows…".to_string(),
            matching_empty: notice(
                "No matching workflows",
                "No repositories or workflows match the current filters.",
            ),
            hidden_empty: notice(
                "All workflows are hidden",
                "Turn on Show hidden pipelines in View options to list them and unhide them.",
            ),
            empty: notice(
                "No workflows yet",
                "Workflows appear here after they run on your configured providers.",
            ),
            pinned_empty: notice(
                "No pinned workflows",
                "Use the pin icon on a pipeline to keep it here.",
            ),
        }
    }
}

pub(super) fn status_options() -> Vec<StatusOption> {
    [
        (PipelineStatus::Success, "Success"),
        (PipelineStatus::Failed, "Failed"),
        (PipelineStatus::Degraded, "Degraded"),
        (PipelineStatus::Running, "Running"),
        (PipelineStatus::Pending, "Pending"),
        (PipelineStatus::Cancelled, "Cancelled"),
        (PipelineStatus::Skipped, "Skipped"),
    ]
    .into_iter()
    .map(|(value, label)| StatusOption {
        value,
        label: label.to_string(),
    })
    .collect()
}

fn trigger_copy() -> ActionCopy {
    ActionCopy {
        label: "Trigger workflow".to_string(),
        compact_label: "Trigger".to_string(),
        title: "Trigger".to_string(),
        success: notice(
            "Workflow triggered",
            "Workflow has been triggered successfully",
        ),
        error_title: "Couldn't trigger workflow".to_string(),
        note: None,
    }
}

fn rerun_copy() -> ActionCopy {
    ActionCopy {
        label: "Rerun workflow".to_string(),
        compact_label: "Rerun".to_string(),
        title: "Rerun".to_string(),
        success: notice("Workflow triggered", "Workflow rerun started"),
        error_title: "Couldn't rerun workflow".to_string(),
        note: None,
    }
}

pub(super) fn presentation(provider_name: &str) -> PluginPresentation {
    PluginPresentation {
        terminology: PipelineTerminology::default(),
        status_options: status_options(),
        wizard: WizardCopy {
            credentials_description: "Enter your API credentials".to_string(),
            name_placeholder: format!("e.g. {provider_name} (work)"),
            permissions_title: "Token permissions".to_string(),
        },
        view: ViewCopy::default(),
        trigger: trigger_copy(),
        rerun: rerun_copy(),
        history_note: None,
        empty_history_description: "This workflow has no recorded runs.".to_string(),
        run_details_title: "Run details".to_string(),
        progress_title: "Run in progress".to_string(),
        archived_notice: None,
        force_cancel_confirmation:
            "Force cancel only after normal cancellation fails. You can't undo this.".to_string(),
    }
}
