use crate::domain::application::board::BoardCopy;

pub(in crate::application::services::correlation_service) fn config_notice() -> String {
    "Defined in config.toml".into()
}

pub(in crate::application::services::correlation_service) fn board_copy() -> BoardCopy {
    BoardCopy {
        title: "Manage applications".into(),
        help: "Pipedash finds applications on its own. Drag a pipeline onto an application to link it, and drag a target onto an environment to group it. A build can feed several applications. A deployment belongs to one."
            .into(),
        search_placeholder: "Search pipelines and applications".into(),
        back: "Applications".into(),
        unassigned: "Not in an application".into(),
        unassigned_empty: "Every pipeline is in an application".into(),
        unassigned_drop: "Drop a pipeline here to unlink it".into(),
        applications: "Applications".into(),
        applications_empty: "No applications match".into(),
        new_application_drop: "Drop a pipeline here to create an application".into(),
        open_application: "Open application".into(),
        build: "Build".into(),
        deploy: "Deploy".into(),
        lane_empty: "Drop a pipeline here".into(),
        environments: "Environments".into(),
        environments_empty: "No targets observed yet".into(),
        new_environment_drop: "Drop a target here to create an environment".into(),
        new_environment: "New environment".into(),
        environment_name: "Name".into(),
        create_environment: "Create environment".into(),
        cancel: "Cancel".into(),
    }
}
