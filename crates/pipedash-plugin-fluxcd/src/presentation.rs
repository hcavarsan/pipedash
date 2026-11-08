use pipedash_plugin_api::{
    ActionCopy,
    Notice,
    PipelineStatus,
    PipelineTerminology,
    PluginPresentation,
    StatusOption,
    ViewCopy,
    WizardCopy,
};

fn notice(title: &str, message: &str) -> Notice {
    Notice {
        title: title.to_string(),
        message: message.to_string(),
    }
}

fn terminology() -> PipelineTerminology {
    PipelineTerminology {
        item: "resource".to_string(),
        items: "resources".to_string(),
        group: "namespace".to_string(),
        groups: "namespaces".to_string(),
        group_title: "Namespace".to_string(),
        source_title: "Source".to_string(),
        sources_title: "Sources".to_string(),
        history: "Reconciliation history".to_string(),
        trigger: "Reconcile".to_string(),
        run: "reconciliation".to_string(),
        runs: "reconciliations".to_string(),
        organization_label: "Namespace".to_string(),
        section_count_label: "Resources".to_string(),
    }
}

fn status_options() -> Vec<StatusOption> {
    [
        (PipelineStatus::Success, "Ready"),
        (PipelineStatus::Failed, "Not ready"),
        (PipelineStatus::Degraded, "Degraded"),
        (PipelineStatus::Running, "Reconciling"),
        (PipelineStatus::Pending, "Pending"),
        (PipelineStatus::Skipped, "Suspended"),
    ]
    .into_iter()
    .map(|(value, label)| StatusOption {
        value,
        label: label.to_string(),
    })
    .collect()
}

fn view() -> ViewCopy {
    ViewCopy {
        title: "FluxCD resources".to_string(),
        list_title: "FluxCD resources".to_string(),
        search_placeholder: "Search resources, namespaces, sources…".to_string(),
        matching_empty: notice(
            "No matching resources",
            "No resources match the current filters.",
        ),
        hidden_empty: notice(
            "All resources are hidden",
            "Turn on Show hidden pipelines in View options to list them and unhide them.",
        ),
        empty: notice(
            "No FluxCD resources found",
            "Check the configured namespaces and resource selection in Settings → Providers.",
        ),
        pinned_empty: notice(
            "No pinned resources",
            "Use the pin icon on a resource to keep it here.",
        ),
    }
}

fn trigger() -> ActionCopy {
    ActionCopy {
        label: "Reconcile".to_string(),
        compact_label: "Reconcile".to_string(),
        title: "Reconcile".to_string(),
        success: notice(
            "Reconciliation requested",
            "Flux will reconcile the resource using its current configuration.",
        ),
        error_title: "Couldn't request reconciliation".to_string(),
        note: Some(
            "Reconcile uses the current resource configuration. It does not restore this historical revision."
                .to_string(),
        ),
    }
}

fn rerun() -> ActionCopy {
    ActionCopy {
        label: "Reconcile".to_string(),
        compact_label: "Reconcile".to_string(),
        title: "Reconcile".to_string(),
        success: notice(
            "Reconciliation requested",
            "Flux will reconcile the current resource configuration.",
        ),
        error_title: "Couldn't request reconciliation".to_string(),
        note: None,
    }
}

pub(crate) fn presentation() -> PluginPresentation {
    PluginPresentation {
        terminology: terminology(),
        status_options: status_options(),
        wizard: WizardCopy {
            credentials_description: "Connect to your Kubernetes cluster".to_string(),
            name_placeholder: "e.g. FluxCD (production)".to_string(),
            permissions_title: "Kubernetes permissions".to_string(),
        },
        view: view(),
        trigger: trigger(),
        rerun: rerun(),
        history_note: Some(
            "FluxCD retains a limited reconciliation history. Repeated reconciliations may be grouped, and older entries may be unavailable."
                .to_string(),
        ),
        empty_history_description:
            "Flux has not recorded a reconciliation for this resource yet.".to_string(),
        run_details_title: "Reconciliation details".to_string(),
        progress_title: "Reconciliation in progress".to_string(),
        archived_notice: Some(notice(
            "Cached reconciliation",
            "Flux no longer retains this entry. These details come from Pipedash's cache and will disappear if the cache is cleared.",
        )),
        ..PluginPresentation::new("FluxCD")
    }
}
