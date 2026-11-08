use super::StoredView;
use crate::domain::{
    DomainError,
    DomainResult,
    PinKind,
    ViewDisplay,
    ViewKind,
    ViewRules,
};

pub(super) type ViewRow = (i64, String, String, i64, String, String, Option<i64>);
pub(super) type MemberRow = (i64, i64, String, i64, bool);
pub(super) type PinRow = (String, String);

pub(super) fn parse_kind(kind: &str) -> Option<ViewKind> {
    match kind {
        "all" => Some(ViewKind::All),
        "pinned" => Some(ViewKind::Pinned),
        "hidden" => Some(ViewKind::Hidden),
        "custom" => Some(ViewKind::Custom),
        _ => None,
    }
}

pub(super) fn parse_pin_kind(kind: &str) -> Option<PinKind> {
    match kind {
        "view" => Some(PinKind::View),
        "repository" => Some(PinKind::Repository),
        _ => None,
    }
}

pub(super) fn stored_view(row: ViewRow) -> Option<StoredView> {
    let (id, kind, name, position, rules_json, display_json, parent_id) = row;
    let Some(kind) = parse_kind(&kind) else {
        tracing::warn!("Skipping pipeline view {id} with unknown kind {kind}");
        return None;
    };
    let rules = serde_json::from_str(&rules_json).unwrap_or_else(|e| {
        tracing::warn!("Invalid rules for pipeline view {id}: {e}");
        ViewRules::default()
    });
    let display = serde_json::from_str(&display_json).unwrap_or_else(|e| {
        tracing::warn!("Invalid display settings for pipeline view {id}: {e}");
        if kind == ViewKind::Pinned {
            ViewDisplay::pinned_default()
        } else {
            ViewDisplay::default()
        }
    });
    Some(StoredView {
        id,
        kind,
        name,
        parent_id,
        position,
        rules,
        display,
    })
}

pub(super) fn to_json<T: serde::Serialize>(value: &T) -> DomainResult<String> {
    serde_json::to_string(value).map_err(|e| DomainError::InternalError(e.to_string()))
}
