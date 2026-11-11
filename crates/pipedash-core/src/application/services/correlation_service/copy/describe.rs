use crate::domain::application::Stage;

pub(in super::super) fn link(name: &str) -> String {
    format!("Linked {name}")
}

pub(in super::super) fn unlink(name: &str) -> String {
    format!("Unlinked {name}")
}

pub(in super::super) fn reject() -> String {
    "Rejected a suggestion".into()
}

pub(in super::super) fn create(name: &str) -> String {
    format!("Created {name}")
}

pub(in super::super) fn merge(name: &str) -> String {
    format!("Merged {name} into this application")
}

pub(in super::super) fn split(name: &str) -> String {
    format!("Split into {name}")
}

pub(in super::super) fn rename(name: &str) -> String {
    format!("Renamed to {name}")
}

pub(in super::super) fn hide(hidden: bool) -> String {
    if hidden {
        "Hidden from the list"
    } else {
        "Shown in the list"
    }
    .into()
}

pub(in super::super) fn stage(name: &str, stage: Stage) -> String {
    format!("Set the stage of {name} to {}", stage.as_str())
}

pub(in super::super) fn target_name(name: &str) -> String {
    format!("Renamed a target group to {name}")
}

pub(in super::super) fn target_group(name: Option<&str>) -> String {
    match name {
        Some(name) => format!("Grouped targets as {name}"),
        None => "Ungrouped targets".into(),
    }
}

pub(in super::super) fn direction(from: &str, to: &str, present: bool) -> String {
    if present {
        format!("Set a direction from {from} to {to}")
    } else {
        format!("Removed the direction from {from} to {to}")
    }
}
