use super::hierarchy::{
    custom_nodes,
    subtree,
};
use super::page;
use super::PipelineViewService;
use crate::domain::{
    DomainResult,
    PipelineNavigation,
    ViewKind,
    ViewRef,
};

#[cfg(test)]
mod tests;

impl PipelineViewService {
    pub async fn navigation(&self, search: Option<String>) -> DomainResult<PipelineNavigation> {
        let snapshot = self.snapshot().await?;
        let total_pipelines = snapshot
            .providers
            .iter()
            .map(|p| p.pipeline_count)
            .sum::<usize>();
        let query = search
            .as_deref()
            .map(|q| q.trim().to_lowercase())
            .filter(|q| !q.is_empty());
        let mut views = page::list_views(&snapshot);
        views.retain(|v| v.kind != ViewKind::All);
        let Some(query) = query else {
            return Ok(PipelineNavigation {
                views,
                pins: snapshot.pins,
                providers: snapshot.providers,
                total_pipelines,
            });
        };

        let matches = |name: &str| name.to_lowercase().contains(&query);
        let nodes = custom_nodes(&views);
        let hits: Vec<i64> = views
            .iter()
            .filter_map(|v| match v.id {
                ViewRef::Custom(id) if matches(&v.name) => Some(id),
                _ => None,
            })
            .collect();
        views.retain(|v| {
            matches(&v.name)
                || matches!(v.id, ViewRef::Custom(id)
                    if subtree(&nodes, id).iter().any(|d| hits.contains(d)))
        });
        let mut pins = snapshot.pins;
        pins.retain(|p| matches(&p.name));
        let mut providers = snapshot.providers;
        providers.retain(|p| matches(&p.name));
        Ok(PipelineNavigation {
            views,
            pins,
            providers,
            total_pipelines,
        })
    }
}
