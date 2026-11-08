use pipedash_plugin_api::{
    PipelineTerminology,
    PluginMetadata,
};

use super::{
    section_presentation,
    view_presentation,
};
use crate::application::test_support::plugins::{
    registry,
    STUB,
};
use crate::domain::{
    SectionPresentation,
    ViewPresentation,
};

fn metadata(organization_label: &str) -> PluginMetadata {
    let mut metadata = registry()
        .metadata()
        .find(|metadata| metadata.provider_type == STUB)
        .cloned()
        .unwrap();
    metadata.presentation.terminology.organization_label = organization_label.to_string();
    metadata.presentation.terminology.section_count_label = format!("{organization_label} count");
    metadata.presentation.view.title = format!("{organization_label} view");
    metadata
}

#[test]
fn equal_terminology_uses_the_shared_presentation() {
    let (a, b) = (metadata("Cluster"), metadata("Cluster"));
    let providers = [&a, &b];

    let view = view_presentation(&providers);
    assert_eq!(view.terminology, a.presentation.terminology);
    assert_eq!(view.status_options, a.presentation.status_options);
    assert_eq!(view.copy, a.presentation.view);
    assert_eq!(
        section_presentation(&providers),
        SectionPresentation {
            organization_label: "Cluster".to_string(),
            count_label: "Cluster count".to_string(),
        }
    );
}

#[test]
fn mixed_terminology_falls_back_to_defaults() {
    let (a, b) = (metadata("Cluster"), metadata("Org"));
    let providers = [&a, &b];

    assert_eq!(view_presentation(&providers), ViewPresentation::default());
    assert_eq!(
        section_presentation(&providers),
        SectionPresentation::default()
    );
}

#[test]
fn empty_falls_back_to_defaults() {
    assert_eq!(view_presentation(&[]), ViewPresentation::default());
    assert_eq!(section_presentation(&[]), SectionPresentation::default());
    assert_eq!(
        ViewPresentation::default().terminology,
        PipelineTerminology::default()
    );
}
