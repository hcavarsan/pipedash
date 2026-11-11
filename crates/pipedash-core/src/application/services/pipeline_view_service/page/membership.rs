use std::collections::{
    HashMap,
    HashSet,
};

use super::{
    ResolvedView,
    Snapshot,
};
use crate::domain::{
    MembershipSource,
    Pipeline,
    ProviderSummary,
    ViewKind,
    ViewRef,
};

pub(super) struct Item<'a> {
    pub(super) pipeline: &'a Pipeline,
    pub(super) provider: &'a ProviderSummary,
    pub(super) organization: String,
    pub(super) repo: String,
    pub(super) hidden: bool,
}

pub(super) fn split_repository(repository: &str, provider_name: &str) -> (String, String) {
    match repository.split_once('/') {
        Some((org, rest)) if !org.is_empty() && org != "(root)" && org != "Unknown" => {
            (org.to_string(), rest.to_string())
        }
        Some((_, rest)) => (provider_name.to_string(), rest.to_string()),
        None => (provider_name.to_string(), repository.to_string()),
    }
}

pub(super) fn eligible(s: &Snapshot) -> Vec<Item<'_>> {
    let providers: HashMap<i64, &ProviderSummary> = s.providers.iter().map(|p| (p.id, p)).collect();
    s.pipelines
        .iter()
        .filter_map(|pipeline| {
            if pipeline.repository.is_empty() {
                tracing::debug!("Skipping pipeline {} without a repository", pipeline.id);
                return None;
            }
            let Some(provider) = providers.get(&pipeline.provider_id).copied() else {
                tracing::debug!(
                    "Skipping pipeline {} of unknown provider {}",
                    pipeline.id,
                    pipeline.provider_id
                );
                return None;
            };
            let (organization, repo) = split_repository(&pipeline.repository, &provider.name);
            Some(Item {
                pipeline,
                provider,
                organization,
                repo,
                hidden: s.hidden.contains(&pipeline.id),
            })
        })
        .collect()
}

pub(super) struct Member {
    pub(super) index: usize,
    pub(super) source: MembershipSource,
    pub(super) forced: bool,
}

fn descendants<'a>(views: &'a [ResolvedView], id: &ViewRef) -> Vec<&'a ResolvedView> {
    let mut found = Vec::new();
    for child in views.iter().filter(|v| v.view.parent.as_ref() == Some(id)) {
        found.push(child);
        found.extend(descendants(views, &child.view.id));
    }
    found
}

pub(super) fn members(
    views: &[ResolvedView], view: &ResolvedView, items: &[Item<'_>], show_hidden: bool,
) -> Vec<Member> {
    let mut found: Vec<Option<Member>> = items.iter().map(|_| None).collect();
    let tree = std::iter::once((view, false)).chain(
        descendants(views, &view.view.id)
            .into_iter()
            .map(|child| (child, true)),
    );
    for (resolved, inherited) in tree {
        let manual: HashSet<&str> = resolved.manual.iter().map(String::as_str).collect();
        let matcher = resolved.view.rules.matcher();
        let all = resolved.view.kind == ViewKind::All;
        for (index, item) in items.iter().enumerate() {
            let id = item.pipeline.id.as_str();
            let (source, forced) = if !all && manual.contains(id) {
                (MembershipSource::Manual, true)
            } else if all || (!resolved.excluded.contains(id) && matcher.matches(item.pipeline)) {
                (MembershipSource::Rule, false)
            } else {
                continue;
            };
            match &mut found[index] {
                Some(member) => member.forced |= forced,
                slot => {
                    *slot = Some(Member {
                        index,
                        source: if inherited {
                            MembershipSource::SubView
                        } else {
                            source
                        },
                        forced,
                    })
                }
            }
        }
    }
    found
        .into_iter()
        .flatten()
        .filter(|m| show_hidden || m.forced || !items[m.index].hidden)
        .collect()
}
