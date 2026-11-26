use crate::domain::application::board::BoardCopy;

pub(in crate::application::services::correlation_service) fn config_notice() -> String {
    "Defined in config.toml".into()
}

pub(in crate::application::services::correlation_service) fn unassigned() -> String {
    "Not in an application".into()
}

pub(in crate::application::services::correlation_service) fn unassigned_hint(
    empty: bool,
) -> String {
    if empty {
        "Every pipeline is in an application".into()
    } else {
        "Drop a pipeline here to unlink it".into()
    }
}

pub(in crate::application::services::correlation_service) fn new_application() -> String {
    "New application".into()
}

pub(in crate::application::services::correlation_service) fn new_application_hint() -> String {
    "Drop a pipeline here to create an application".into()
}

pub(in crate::application::services::correlation_service) fn lane_hint() -> String {
    "Drop a pipeline here".into()
}

pub(in crate::application::services::correlation_service) fn new_environment() -> String {
    "New environment".into()
}

pub(in crate::application::services::correlation_service) fn new_environment_hint() -> String {
    "Drop a target here to create an environment".into()
}

pub(in crate::application::services::correlation_service) fn zone_label(
    application: &str, zone: &str,
) -> String {
    format!("{application} {zone}")
}

pub(in crate::application::services::correlation_service) fn drop_not_allowed() -> String {
    "This item cannot be dropped there".into()
}

pub(in crate::application::services::correlation_service) fn board_copy() -> BoardCopy {
    BoardCopy {
        title: "Manage applications".into(),
        help: "Pipedash finds applications on its own. Drag a pipeline onto an application to link it, and drag a target onto an environment to group it. A build can feed several applications. A deployment belongs to one."
            .into(),
        search_placeholder: "Search pipelines and applications".into(),
        back: "Applications".into(),
        applications: "Applications".into(),
        applications_empty: "No applications match".into(),
        open_application: "Open application".into(),
        environments: "Environments".into(),
        environments_empty: "No targets observed yet".into(),
        new_environment: new_environment(),
        environment_name: "Name".into(),
        create_environment: "Create environment".into(),
        cancel: "Cancel".into(),
    }
}
