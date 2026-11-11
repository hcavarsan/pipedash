pub(super) mod describe;
pub(super) mod views;

use crate::domain::application::dto::{
    ApplicationCopy,
    LabelOption,
    SegmentKind,
};
use crate::domain::application::metrics::MetricId;

fn option(value: &str, label: &str) -> LabelOption {
    LabelOption {
        value: value.to_string(),
        label: label.to_string(),
    }
}

pub(super) fn application_copy() -> ApplicationCopy {
    ApplicationCopy {
        title: "Applications".into(),
        search_placeholder: "Search applications".into(),
        empty_title: "No applications yet".into(),
        empty_description:
            "Applications appear when a provider reports a deployment. Nothing needs to change in your pipelines."
                .into(),
        new_application: "New application".into(),
        suggestions: "Suggestions".into(),
        suggestions_empty: "No open suggestions".into(),
        health_filter: "Health".into(),
        provider_filter: "Provider".into(),
        behind_only: "Behind only".into(),
        with_suggestions: "With suggestions".into(),
        include_hidden: "Show hidden".into(),
        sort: "Sort".into(),
        sort_options: vec![
            option("name", "Name"),
            option("health", "Health"),
            option("behind", "Commits behind"),
            option("last_deploy", "Last deploy"),
        ],
        health_options: vec![
            option("success", "Healthy"),
            option("degraded", "Degraded"),
            option("failed", "Failed"),
            option("running", "In progress"),
            option("pending", "Pending"),
        ],
        config_error_title: "config.toml was rejected. The last valid config stays active.".into(),
        config_notice: "Defined in config.toml".into(),
        tab_flow: "Flow".into(),
        tab_changes: "Changes".into(),
        tab_metrics: "Metrics".into(),
        flow_empty: "Nothing is linked to this application yet".into(),
        changes_empty: "No changes observed yet".into(),
        metrics_empty: "No metrics for this period".into(),
        legend_exact: "Proven link".into(),
        legend_suggested: "Suggestion".into(),
        why_linked: "Why linked?".into(),
        edit: "Edit".into(),
        rename: "Rename".into(),
        hide: "Hide".into(),
        unhide: "Show in list".into(),
        merge: "Merge with…".into(),
        split: "Split…".into(),
        members: "Members".into(),
        targets: "Targets".into(),
        delete: "Delete".into(),
        copy_toml: "Copy as TOML".into(),
        copied: "Copied".into(),
        overrides: "Overrides".into(),
        overrides_empty: "No overrides".into(),
        revert: "Revert".into(),
        shadowed: "Shadowed by config.toml".into(),
        accept: "Accept".into(),
        reject: "Reject".into(),
        accept_all: "Accept all".into(),
        unlink: "Unlink".into(),
        save: "Save".into(),
        cancel: "Cancel".into(),
        name: "Name".into(),
        stage: "Stage".into(),
        stage_build: "Build".into(),
        stage_deploy: "Deploy".into(),
        add_member: "Add pipeline".into(),
        remove: "Remove".into(),
        group: "Group".into(),
        ungroup: "Ungroup".into(),
        direction: "Direction".into(),
        direction_from: "From".into(),
        direction_to: "To".into(),
        add_direction: "Add direction".into(),
        all_targets: "All targets".into(),
        funnel: "Funnel".into(),
        export: "Export".into(),
        load_more: "Load more".into(),
        commit_column: "Commit".into(),
        warnings: "Warnings".into(),
        back: "Applications".into(),
        hidden: "Hidden".into(),
        sort_ascending: "Ascending".into(),
        sort_descending: "Descending".into(),
    }
}

pub(super) fn why_run_produced(run_number: i64, target_label: &str) -> String {
    format!("Build #{run_number} produced the image running in {target_label}")
}

pub(super) fn why_same_source(repo: &str, path: &str) -> String {
    if path.is_empty() {
        format!("Builds from {repo}, the repository this deployment syncs")
    } else {
        format!("Builds from {repo} under {path}, the path this deployment syncs")
    }
}

pub(super) fn why_unit_runs(image: &str) -> String {
    format!("The running pods use {image}")
}

pub(super) fn why_unit_deploys(target_label: &str) -> String {
    format!("The provider reports this deployment in {target_label}")
}

pub(super) fn why_linked_by_user() -> String {
    "You linked this pipeline".into()
}

pub(super) fn why_linked_by_config(key: &str) -> String {
    format!("Listed under applications.{key} in config.toml")
}

pub(super) fn deployed_after(minutes: i64, other_label: &str) -> String {
    format!("deployed {minutes} minutes after {other_label}")
}

pub(super) fn health_summary(total: usize, healthy: usize, degraded: usize) -> String {
    let failing = total.saturating_sub(healthy + degraded);
    format!("{total} targets · {healthy} healthy · {degraded} degraded · {failing} failing")
}

pub(super) fn behind(commits: u32) -> String {
    match commits {
        1 => "1 commit behind".into(),
        commits => format!("{commits} commits behind"),
    }
}

pub(super) fn short_lived(count: usize) -> String {
    format!("{count} short-lived")
}

pub(super) fn more_artifacts(count: usize) -> String {
    format!("+{count} more")
}

pub(super) fn no_recorded_start() -> String {
    "The provider records only when the deployment finished, not when it started".into()
}

pub(super) fn no_attempt_records() -> String {
    "The provider keeps a record only for deployments that succeeded".into()
}

pub(super) fn no_recorded_timing() -> String {
    "No run with a recorded start and end in this period".into()
}

pub(super) fn no_recorded_outcome() -> String {
    "No run with a recorded result in this period".into()
}

pub(super) fn no_deployments() -> String {
    "No deployment observed in this period".into()
}

pub(super) fn no_commit_times() -> String {
    "No connected provider can read commits for this repository".into()
}

pub(super) fn no_observation_window() -> String {
    "Pipedash was not watching when these changes were deployed".into()
}

pub(super) fn no_failures_observed() -> String {
    "No failure was observed in this period".into()
}

pub(super) fn coverage(counted: usize, excluded: usize) -> String {
    match excluded {
        0 => format!("{counted} changes counted"),
        excluded => format!("{counted} changes counted, {excluded} left out"),
    }
}

pub(super) fn config_conflict(local_id: &str, other_key: &str) -> String {
    format!("{local_id} also matches applications.{other_key}, so neither application claims it")
}

pub(super) fn selector_matches_nothing(provider: &str, pipeline: &str) -> String {
    format!("No pipeline matches {pipeline} on provider {provider} yet")
}

pub(super) fn config_claims_both(first: &str, second: &str, local_id: &str) -> String {
    format!("applications.{first} and applications.{second} both claim {local_id}")
}

pub(super) fn commit_locations_title() -> String {
    "Where is this commit".into()
}

pub(super) fn deploy_changes_title() -> String {
    "Changes in this deploy".into()
}

pub(super) fn location_live() -> String {
    "Live".into()
}

pub(super) fn location_not_deployed() -> String {
    "Not deployed".into()
}

pub(super) fn duration_label(seconds: i64) -> String {
    let hours = seconds / 3600;
    let minutes = seconds % 3600 / 60;
    let rest = seconds % 60;
    match (hours, minutes, rest) {
        (0, 0, rest) => format!("{rest}s"),
        (0, minutes, 0) => format!("{minutes}m"),
        (0, minutes, rest) => format!("{minutes}m {rest}s"),
        (hours, minutes, _) => format!("{hours}h {minutes}m"),
    }
}

pub(super) fn segment_label(kind: SegmentKind, seconds: i64) -> String {
    let name = match kind {
        SegmentKind::Build => "Build",
        SegmentKind::Wait => "Wait",
        SegmentKind::Deploy => "Deploy",
        SegmentKind::BuildToDeployed => "Build to deployed",
    };
    format!("{name} {}", duration_label(seconds))
}

pub(super) fn metric_label(id: MetricId) -> String {
    match id {
        MetricId::LeadTime => "Lead time",
        MetricId::DeliveryTime => "Delivery time",
        MetricId::BuildTime => "Build time",
        MetricId::WaitTime => "Wait time",
        MetricId::BuildToDeployed => "Build to deployed",
        MetricId::DeployTime => "Deploy time",
        MetricId::DeployFrequency => "Deploy frequency",
        MetricId::BuildSuccessRate => "Build success rate",
        MetricId::DeployFailures => "Deploy failures",
        MetricId::ChangeFailureRate => "Change failure rate",
        MetricId::RecoveryTime => "Recovery time",
        MetricId::Drift => "Drift",
    }
    .into()
}

pub(super) fn funnel_built() -> String {
    "Built".into()
}

pub(super) fn funnel_passed() -> String {
    "Build passed".into()
}

pub(super) fn funnel_deployed(group_label: &str) -> String {
    format!("Deployed to {group_label}")
}

pub(super) fn not_editable_config(name: &str) -> String {
    format!("{name} is defined in config.toml. Edit config.toml instead.")
}

pub(super) fn not_deletable(name: &str) -> String {
    format!("{name} can't be deleted. Hide it instead.")
}

pub(super) fn pipeline_not_found(id: &str) -> String {
    format!("Pipeline {id} was not found")
}

pub(super) fn not_a_member(name: &str) -> String {
    format!("{name} is not part of this application")
}

pub(super) fn suggestion_not_found() -> String {
    "This suggestion is no longer available".into()
}

pub(super) fn suggestion_not_actionable() -> String {
    "This suggestion can't be applied".into()
}

pub(super) fn name_empty() -> String {
    "The name can't be empty".into()
}

pub(super) fn name_too_long(max: usize) -> String {
    format!("The name can't be longer than {max} characters")
}

pub(super) fn merge_self() -> String {
    "An application can't be merged into itself".into()
}

pub(super) fn application_not_found(id: i64) -> String {
    format!("Application {id} was not found")
}

pub(super) fn override_not_found(id: i64) -> String {
    format!("Override {id} was not found for this application")
}

pub(super) fn no_members() -> String {
    "Choose at least one pipeline".into()
}

pub(super) fn no_targets_selected() -> String {
    "Choose at least one target".into()
}

pub(super) fn unknown_target(key: &str) -> String {
    format!("Target {key} does not belong to this application")
}

pub(super) fn unknown_group(key: &str) -> String {
    format!("Group {key} does not belong to this application")
}

pub(super) fn direction_cycle() -> String {
    "That direction would make the targets depend on each other in a loop".into()
}

pub(super) fn suggest_same_application(first: &str, second: &str) -> String {
    format!("{first} and {second} may be one application")
}

pub(super) fn suggest_membership(pipeline: &str, application: &str) -> String {
    format!("{pipeline} may belong to {application}")
}

pub(super) fn suggest_built_from(artifact: &str, commit: &str) -> String {
    format!("Image {artifact} may come from commit {commit}")
}

pub(super) fn why_rule_same_artifact(repository: &str) -> String {
    format!("Both run the same image digest from {repository}")
}

pub(super) fn why_rule_same_repo_path(repo: &str, path: &str) -> String {
    if path.is_empty() {
        format!("Both deploy from {repo}")
    } else {
        format!("Both deploy from {repo} under {path}")
    }
}

pub(super) fn why_membership_shared(repo: &str, path: &str) -> String {
    if path.is_empty() {
        format!("Builds from {repo}, but other pipelines build from it too")
    } else {
        format!("Builds from {repo} under {path}, but other pipelines build from it too")
    }
}

pub(super) fn why_rule_same_label(label: &str) -> String {
    format!("Both carry the application label {label}")
}

pub(super) fn why_rule_same_name(name: &str) -> String {
    format!("Both are named {name} once environment words are removed")
}

pub(super) fn why_built_from_label(source: &str, revision: &str) -> String {
    format!("The image says it was built from {source} at {revision}, but no build run confirms it")
}

pub(super) fn why_built_from_tag(repo: &str, commit: &str) -> String {
    format!("The image tag names commit {commit} in {repo}")
}
