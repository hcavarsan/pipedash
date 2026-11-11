use pipedash_plugin_api::{
    PluginMetadata,
    PluginPresentation,
};

use crate::domain::{
    SectionPresentation,
    ViewPresentation,
};

#[cfg(test)]
mod tests;

fn shared<'a>(providers: &[&'a PluginMetadata]) -> Option<&'a PluginPresentation> {
    let first = &providers.first()?.presentation;
    providers
        .iter()
        .all(|provider| provider.presentation.terminology == first.terminology)
        .then_some(first)
}

pub(super) fn view_presentation(providers: &[&PluginMetadata]) -> ViewPresentation {
    shared(providers).map_or_else(ViewPresentation::default, |presentation| ViewPresentation {
        terminology: presentation.terminology.clone(),
        status_options: presentation.status_options.clone(),
        copy: presentation.view.clone(),
    })
}

pub(super) fn section_presentation(providers: &[&PluginMetadata]) -> SectionPresentation {
    shared(providers).map_or_else(SectionPresentation::default, |presentation| {
        SectionPresentation {
            organization_label: presentation.terminology.organization_label.clone(),
            count_label: presentation.terminology.section_count_label.clone(),
        }
    })
}
