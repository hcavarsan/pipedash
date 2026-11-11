pub(in crate::application::services::correlation_service) fn why_builds_from(repo: &str) -> String {
    format!("This pipeline builds from {repo}")
}

pub(in crate::application::services::correlation_service) fn invalid_cursor() -> String {
    "The page cursor is not valid".into()
}

pub(in crate::application::services::correlation_service) fn why_syncs_from(
    repo: &str, path: &str,
) -> String {
    if path.is_empty() {
        format!("This deployment syncs {repo}")
    } else {
        format!("This deployment syncs {repo} under {path}")
    }
}

pub(in crate::application::services::correlation_service) fn why_direction(
    source: &str, from_label: &str, to_label: &str,
) -> String {
    match source {
        "user" => format!("You set {to_label} to follow {from_label}"),
        "config" => format!("config.toml sets {to_label} to follow {from_label}"),
        _ => format!("A declared dependency says {to_label} follows {from_label}"),
    }
}

pub(in crate::application::services::correlation_service) fn image_ref(
    repository: &str, digest: &str,
) -> String {
    let short: String = digest.chars().take(19).collect();
    format!("{repository}@{short}")
}

pub(in crate::application::services::correlation_service) fn host_and_path(
    repo: &str,
) -> (String, Option<String>) {
    match repo.split_once('/') {
        Some((host, path)) if !path.is_empty() => (path.into(), Some(host.into())),
        _ => (repo.into(), None),
    }
}

pub(in crate::application::services::correlation_service) fn run_label(run_number: i64) -> String {
    format!("#{run_number}")
}

pub(in crate::application::services::correlation_service) fn column_build() -> String {
    "Build".into()
}

pub(in crate::application::services::correlation_service) fn location_behind(
    commits: u32,
) -> String {
    match commits {
        1 => "Runs an older commit, 1 commit behind this one".into(),
        commits => format!("Runs an older commit, {commits} commits behind this one"),
    }
}

pub(in crate::application::services::correlation_service) fn changes_unavailable() -> String {
    "No connected provider can compare commits for this repository".into()
}

pub(in crate::application::services::correlation_service) fn no_previous_deploy() -> String {
    "No earlier successful deployment is known to compare with".into()
}
