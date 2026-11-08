use std::cmp::Ordering;
use std::collections::HashSet;

use chrono::Utc;

use super::sort::natural_cmp;
use super::{
    build_page,
    list_views,
    ResolvedView,
    Snapshot,
};
use crate::application::test_support::pipeline;
use crate::application::test_support::plugins::registry;
use crate::domain::{
    DomainError,
    MembershipSource,
    Pipeline,
    PipelineStatus,
    PipelineView,
    PipelineViewQuery,
    ProviderSummary,
    SectionKind,
    SectionMode,
    SortDirection,
    SortKey,
    ViewCapabilities,
    ViewDisplay,
    ViewKind,
    ViewPipeline,
    ViewRef,
    ViewRules,
};

fn provider(id: i64, name: &str) -> ProviderSummary {
    serde_json::from_value(serde_json::json!({
        "id": id, "name": name, "provider_type": "stub", "icon": null,
        "pipeline_count": 0, "last_updated": null, "refresh_interval": 30,
        "configured_repositories": [], "last_fetch_status": "success",
        "last_fetch_error": null, "last_fetch_at": null, "version": 1
    }))
    .unwrap()
}

fn view(id: ViewRef, kind: ViewKind, display: ViewDisplay) -> ResolvedView {
    ResolvedView {
        view: PipelineView {
            id,
            kind,
            name: kind.as_str().to_string(),
            rules: ViewRules::default(),
            display,
            parent: None,
            capabilities: ViewCapabilities::for_kind(kind),
            count: 0,
        },
        manual: Vec::new(),
        excluded: HashSet::new(),
    }
}

fn custom(
    id: i64, name: &str, parent: Option<i64>, manual: &[&str], display: ViewDisplay,
) -> ResolvedView {
    let mut resolved = view(ViewRef::Custom(id), ViewKind::Custom, display);
    resolved.view.name = name.to_string();
    resolved.view.parent = parent.map(ViewRef::Custom);
    resolved.manual = manual.iter().map(|n| format!("stub__1__{n}")).collect();
    resolved
}

fn snapshot(pipelines: Vec<Pipeline>) -> Snapshot {
    Snapshot {
        pipelines,
        providers: vec![provider(1, "Stub One")],
        views: vec![
            view(ViewRef::All, ViewKind::All, ViewDisplay::default()),
            view(
                ViewRef::Pinned,
                ViewKind::Pinned,
                ViewDisplay::pinned_default(),
            ),
        ],
        hidden: HashSet::new(),
        pins: Vec::new(),
        registry: registry(),
    }
}

fn query(view: ViewRef) -> PipelineViewQuery {
    PipelineViewQuery {
        view,
        ..Default::default()
    }
}

fn named(name: &str, repository: &str, status: PipelineStatus) -> Pipeline {
    Pipeline {
        repository: repository.to_string(),
        ..pipeline(1, name, status)
    }
}

fn names(pipelines: &[ViewPipeline]) -> Vec<&str> {
    pipelines.iter().map(|p| p.pipeline.name.as_str()).collect()
}

#[test]
fn hidden_failed_workflow_stays_out_of_repository_status_with_show_hidden() {
    let mut s = snapshot(vec![
        pipeline(1, "build", PipelineStatus::Success),
        pipeline(1, "legacy", PipelineStatus::Failed),
    ]);
    s.hidden.insert("stub__1__legacy".into());
    let q = PipelineViewQuery {
        show_hidden: true,
        ..query(ViewRef::All)
    };

    let page = build_page(&s, &q, Utc::now()).unwrap();

    assert_eq!(page.sections.len(), 1);
    let section = &page.sections[0];
    assert_eq!(section.status, Some(PipelineStatus::Success));
    assert_eq!(names(&section.pipelines), vec!["build", "legacy"]);
    assert!(section.pipelines[1].hidden);
    assert_eq!(page.hidden_count, 1);
}

#[test]
fn repository_with_only_hidden_workflows_keeps_their_status() {
    let mut s = snapshot(vec![pipeline(1, "legacy", PipelineStatus::Failed)]);
    s.hidden.insert("stub__1__legacy".into());
    let q = PipelineViewQuery {
        show_hidden: true,
        ..query(ViewRef::All)
    };

    let page = build_page(&s, &q, Utc::now()).unwrap();

    assert_eq!(page.sections[0].status, Some(PipelineStatus::Failed));
}

#[test]
fn view_membership_is_manual_plus_rules_minus_excluded() {
    let mut s = snapshot(vec![
        named("a", "other/x", PipelineStatus::Success),
        named("b", "acme/y", PipelineStatus::Success),
        named("c", "acme/z", PipelineStatus::Success),
    ]);
    let mut custom = view(ViewRef::Custom(7), ViewKind::Custom, ViewDisplay::default());
    custom.manual = vec!["stub__1__a".into()];
    custom.excluded = HashSet::from(["stub__1__c".to_string()]);
    custom.view.rules.repositories = vec!["ACME/*".into()];
    s.views.push(custom);

    let page = build_page(&s, &query(ViewRef::Custom(7)), Utc::now()).unwrap();
    let listed: Vec<&ViewPipeline> = page.sections.iter().flat_map(|s| &s.pipelines).collect();
    let sources: Vec<(&str, MembershipSource)> = listed
        .iter()
        .map(|p| (p.pipeline.name.as_str(), p.memberships[0].source))
        .collect();
    assert_eq!(
        sources,
        vec![
            ("a", MembershipSource::Manual),
            ("b", MembershipSource::Rule)
        ]
    );
    assert_eq!(list_views(&s)[2].count, 2);

    s.hidden.insert("stub__1__b".into());
    s.hidden.insert("stub__1__a".into());
    let page = build_page(&s, &query(ViewRef::Custom(7)), Utc::now()).unwrap();
    let listed: Vec<&str> = page
        .sections
        .iter()
        .flat_map(|s| names(&s.pipelines))
        .collect();
    assert_eq!(listed, vec!["a"]);
    assert_eq!(page.hidden_count, 1);

    let q = PipelineViewQuery {
        show_hidden: true,
        ..query(ViewRef::Custom(7))
    };
    assert_eq!(build_page(&s, &q, Utc::now()).unwrap().total, 2);
}

#[test]
fn pinned_block_only_on_all_without_provider_filter() {
    let mut s = snapshot(vec![
        pipeline(1, "build", PipelineStatus::Success),
        pipeline(1, "deploy", PipelineStatus::Success),
    ]);
    s.views[1].manual = vec!["stub__1__deploy".into(), "stub__1__build".into()];

    let page = build_page(&s, &query(ViewRef::All), Utc::now()).unwrap();
    assert_eq!(names(&page.pinned), vec!["deploy", "build"]);
    assert!(page.pinned.iter().all(|p| p.pinned));

    let filtered = PipelineViewQuery {
        provider_id: Some(1),
        ..query(ViewRef::All)
    };
    assert!(build_page(&s, &filtered, Utc::now())
        .unwrap()
        .pinned
        .is_empty());
    let pinned_page = build_page(&s, &query(ViewRef::Pinned), Utc::now()).unwrap();
    assert!(pinned_page.pinned.is_empty());
    assert_eq!(
        names(&pinned_page.sections[0].pipelines),
        vec!["deploy", "build"]
    );
}

#[test]
fn status_sections_follow_severity_order() {
    let mut s = snapshot(vec![
        pipeline(1, "ok", PipelineStatus::Success),
        pipeline(1, "broken", PipelineStatus::Failed),
        pipeline(1, "busy", PipelineStatus::Running),
    ]);
    s.views[0].view.display = ViewDisplay {
        sections: SectionMode::Status,
        direction: SortDirection::Desc,
        ..Default::default()
    };

    let page = build_page(&s, &query(ViewRef::All), Utc::now()).unwrap();
    let ids: Vec<&str> = page.sections.iter().map(|s| s.id.as_str()).collect();

    assert_eq!(
        ids,
        vec!["status:failed", "status:running", "status:success"]
    );
}

#[test]
fn manual_sort_keeps_member_order() {
    let mut s = snapshot(vec![
        named("a", "acme/one", PipelineStatus::Success),
        named("b", "acme/two", PipelineStatus::Success),
        named("c", "acme/three", PipelineStatus::Success),
        named("d", "acme/four", PipelineStatus::Success),
    ]);
    let mut custom = view(
        ViewRef::Custom(3),
        ViewKind::Custom,
        ViewDisplay {
            sort: SortKey::Manual,
            sections: SectionMode::None,
            ..Default::default()
        },
    );
    custom.manual = vec!["stub__1__c".into(), "stub__1__a".into()];
    custom.view.rules.name_contains = Some("d".into());
    s.views.push(custom);

    let page = build_page(&s, &query(ViewRef::Custom(3)), Utc::now()).unwrap();

    assert_eq!(page.sections.len(), 1);
    assert_eq!(names(&page.sections[0].pipelines), vec!["c", "a", "d"]);
}

#[test]
fn parent_view_lists_sub_view_sections_first_then_its_own_leftovers() {
    let mut s = snapshot(vec![
        named("lib-ci", "acme/lib", PipelineStatus::Success),
        named("app-ci", "acme/app", PipelineStatus::Success),
        named("a1", "acme/x", PipelineStatus::Success),
        named("shared", "acme/x", PipelineStatus::Failed),
        named("b1", "acme/y", PipelineStatus::Success),
    ]);
    s.views.push(custom(
        1,
        "Parent",
        None,
        &["lib-ci", "app-ci", "a1"],
        ViewDisplay::default(),
    ));
    s.views.push(custom(
        2,
        "Alpha",
        Some(1),
        &["a1", "shared"],
        ViewDisplay::default(),
    ));
    s.views.push(custom(
        3,
        "Beta",
        Some(1),
        &["b1", "shared"],
        ViewDisplay::default(),
    ));
    s.views
        .push(custom(4, "Empty", Some(1), &[], ViewDisplay::default()));

    let page = build_page(&s, &query(ViewRef::Custom(1)), Utc::now()).unwrap();
    let sections: Vec<(&str, SectionKind, &str, Vec<&str>)> = page
        .sections
        .iter()
        .map(|s| (s.id.as_str(), s.kind, s.title.as_str(), names(&s.pipelines)))
        .collect();
    assert_eq!(
        sections,
        vec![
            ("view:2", SectionKind::View, "Alpha", vec!["a1", "shared"]),
            ("view:3", SectionKind::View, "Beta", vec!["b1", "shared"]),
            (
                "repo:acme/app",
                SectionKind::Repository,
                "app",
                vec!["app-ci"]
            ),
            (
                "repo:acme/lib",
                SectionKind::Repository,
                "lib",
                vec!["lib-ci"]
            ),
        ]
    );
    assert_eq!(page.total, 5);
    assert_eq!(page.view.count, 5);
    let counts: Vec<usize> = list_views(&s).iter().skip(2).map(|v| v.count).collect();
    assert_eq!(counts, vec![5, 2, 2, 0]);

    let own = &page.sections[2].pipelines[0];
    let own_sources: Vec<(ViewRef, MembershipSource)> = own
        .memberships
        .iter()
        .map(|m| (m.view.clone(), m.source))
        .collect();
    assert_eq!(
        own_sources,
        vec![(ViewRef::Custom(1), MembershipSource::Manual)]
    );
    for section in &page.sections[..2] {
        let shared = &section.pipelines[1];
        let sources: Vec<(ViewRef, MembershipSource)> = shared
            .memberships
            .iter()
            .map(|m| (m.view.clone(), m.source))
            .collect();
        assert_eq!(
            sources,
            vec![
                (ViewRef::Custom(1), MembershipSource::SubView),
                (ViewRef::Custom(2), MembershipSource::Manual),
                (ViewRef::Custom(3), MembershipSource::Manual),
            ]
        );
    }

    let filtered = PipelineViewQuery {
        search: Some("b1".into()),
        ..query(ViewRef::Custom(1))
    };
    let page = build_page(&s, &filtered, Utc::now()).unwrap();
    assert_eq!(page.sections.len(), 1);
    assert_eq!(page.sections[0].id, "view:3");
    assert_eq!(page.total, 1);
}

#[test]
fn sub_view_sections_lead_whatever_the_sort_and_own_section_mode() {
    let mut s = snapshot(vec![
        named("own", "acme/x", PipelineStatus::Success),
        named("a1", "acme/x", PipelineStatus::Success),
        named("z1", "acme/x", PipelineStatus::Failed),
    ]);
    let display = ViewDisplay {
        sections: SectionMode::None,
        direction: SortDirection::Desc,
        ..Default::default()
    };
    s.views.push(custom(1, "Parent", None, &["own"], display));
    s.views.push(custom(
        2,
        "Kids",
        Some(1),
        &["a1", "z1"],
        ViewDisplay::default(),
    ));

    let page = build_page(&s, &query(ViewRef::Custom(1)), Utc::now()).unwrap();
    let sections: Vec<(&str, SectionKind, Vec<&str>)> = page
        .sections
        .iter()
        .map(|s| (s.id.as_str(), s.kind, names(&s.pipelines)))
        .collect();
    assert_eq!(
        sections,
        vec![
            ("view:2", SectionKind::View, vec!["z1", "a1"]),
            ("all", SectionKind::None, vec!["own"]),
        ]
    );
    assert_eq!(page.sections[0].status, Some(PipelineStatus::Failed));
    assert_eq!(page.total, 3);
}

#[test]
fn search_matches_full_repository_path() {
    let s = snapshot(vec![
        named("build", "acme/app", PipelineStatus::Success),
        named("deploy", "acme/other", PipelineStatus::Success),
    ]);
    let q = PipelineViewQuery {
        search: Some("acme/app".into()),
        ..query(ViewRef::All)
    };

    let page = build_page(&s, &q, Utc::now()).unwrap();

    assert_eq!(page.total, 1);
    assert_eq!(names(&page.sections[0].pipelines), vec!["build"]);
}

#[test]
fn natural_cmp_orders_numbers_numerically_and_ignores_case() {
    assert_eq!(natural_cmp("build-2", "Build-10"), Ordering::Less);
    assert_eq!(natural_cmp("Alpha", "alpha"), Ordering::Equal);
    assert_eq!(natural_cmp("app", "Beta"), Ordering::Less);
}

#[test]
fn unknown_view_is_not_found() {
    let s = snapshot(Vec::new());

    let error = build_page(&s, &query(ViewRef::Custom(99)), Utc::now()).unwrap_err();

    assert!(matches!(error, DomainError::NotFound(_)));
}

#[test]
fn provider_facets_are_ordered_by_display_name() {
    let mut s = snapshot(vec![
        pipeline(1, "x", PipelineStatus::Success),
        pipeline(2, "y", PipelineStatus::Success),
        pipeline(3, "z", PipelineStatus::Success),
    ]);
    s.providers = vec![
        provider(1, "Zeta"),
        provider(2, "alpha"),
        provider(3, "Build 10"),
    ];

    let page = build_page(&s, &query(ViewRef::All), Utc::now()).unwrap();

    assert_eq!(page.facets.provider_ids, vec![2, 3, 1]);
}

#[test]
fn section_count_counts_own_sections_or_pipelines_without_sections() {
    let pipelines = vec![
        named("a", "acme/one", PipelineStatus::Success),
        named("b", "acme/two", PipelineStatus::Failed),
        named("c", "acme/two", PipelineStatus::Success),
    ];
    let count = |sections: SectionMode| {
        let mut s = snapshot(pipelines.clone());
        s.views[0].view.display = ViewDisplay {
            sections,
            ..Default::default()
        };
        build_page(&s, &query(ViewRef::All), Utc::now())
            .unwrap()
            .section_count
    };

    assert_eq!(count(SectionMode::Repository), 2);
    assert_eq!(count(SectionMode::Status), 2);
    assert_eq!(count(SectionMode::None), 3);

    let display = ViewDisplay {
        sections: SectionMode::Repository,
        ..Default::default()
    };
    let mut s = snapshot(pipelines);
    s.views.push(custom(1, "parent", None, &["b"], display));
    s.views.push(custom(2, "child", Some(1), &["a"], display));
    let page = build_page(&s, &query(ViewRef::Custom(1)), Utc::now()).unwrap();
    assert_eq!(page.sections.len(), 2);
    assert_eq!(page.section_count, 1);
}

#[test]
fn section_freshness_is_the_worst_of_its_providers() {
    use crate::domain::{
        FreshnessState,
        ProviderFreshness,
    };

    let mut s = snapshot(vec![
        pipeline(1, "one", PipelineStatus::Success),
        pipeline(2, "two", PipelineStatus::Success),
    ]);
    let stale_at = Utc::now();
    let mut first = provider(1, "Stub One");
    first.freshness = ProviderFreshness {
        state: FreshnessState::Fresh,
        stale_at: Some(stale_at),
    };
    let mut second = provider(2, "Stub Two");
    second.freshness = ProviderFreshness {
        state: FreshnessState::Fresh,
        stale_at: Some(stale_at + chrono::Duration::minutes(5)),
    };
    s.providers = vec![first, second.clone()];

    let page = build_page(&s, &query(ViewRef::All), Utc::now()).unwrap();
    assert_eq!(
        page.sections[0].freshness,
        ProviderFreshness {
            state: FreshnessState::Fresh,
            stale_at: Some(stale_at),
        }
    );

    second.freshness.state = FreshnessState::Down;
    s.providers[1] = second;
    let page = build_page(&s, &query(ViewRef::All), Utc::now()).unwrap();
    assert_eq!(page.sections[0].freshness.state, FreshnessState::Down);
    assert_eq!(page.sections[0].freshness.stale_at, None);
}
