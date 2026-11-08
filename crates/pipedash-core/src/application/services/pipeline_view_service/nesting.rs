use super::hierarchy::{
    check_nesting,
    custom_nodes,
    denied,
    lineage,
    MAX_DEPTH,
};
use super::PipelineViewService;
use crate::domain::{
    DomainError,
    DomainResult,
    ViewEditorOptions,
    ViewKind,
    ViewRef,
};

#[cfg(test)]
mod tests;

impl PipelineViewService {
    pub async fn editor_options(&self, view: Option<ViewRef>) -> DomainResult<ViewEditorOptions> {
        let views = self.list_views().await?;
        let nodes = custom_nodes(&views);
        let moving = match view {
            None => None,
            Some(id) => {
                let found = views.iter().find(|v| v.id == id).ok_or_else(|| {
                    DomainError::NotFound(format!("Pipeline view {id} not found"))
                })?;
                match found.id {
                    ViewRef::Custom(n) if found.kind == ViewKind::Custom => Some(n),
                    _ => return Err(denied(&found.name, "moved")),
                }
            }
        };
        let eligible_parents = views
            .iter()
            .filter(|v| match v.id {
                ViewRef::Custom(parent) => check_nesting(&nodes, moving, parent, "View").is_ok(),
                _ => false,
            })
            .cloned()
            .collect();
        Ok(ViewEditorOptions {
            eligible_parents,
            can_add_sub_view: moving.is_some_and(|id| lineage(&nodes, id).len() < MAX_DEPTH),
        })
    }
}
