use pipedash_plugin_api::{
    TargetLifetime,
    UnitId,
};

use crate::application::services::correlation_service::copy;
use crate::application::services::correlation_service::tests::{
    app_ref,
    artifact,
    builder,
    consumed,
    digest,
    run_with,
    source,
    target,
    unit,
    Fx,
};
use crate::domain::application::dto::{
    ApplicationDetail,
    ApplicationListQuery,
    ApplicationSort,
    ApplicationSummary,
    CellState,
    ChipArrow,
    ChipKind,
    FlowNodeKind,
};
use crate::domain::application::overrides::ApplicationEdit;
use crate::domain::application::Confidence;
use crate::domain::application::IdentityKind;
use crate::domain::pipeline_view::SortDirection;
use crate::domain::{
    Pipeline,
    PipelineStatus,
};
use crate::infrastructure::PipedashConfig;

const IMAGE: &str = "registry.test/shop/api";

async fn names(fx: &Fx, query: ApplicationListQuery) -> Vec<String> {
    fx.settle().await;
    fx.svc
        .list(query)
        .await
        .unwrap()
        .applications
        .into_iter()
        .map(|a| a.name)
        .collect()
}

async fn first_app(fx: &Fx) -> ApplicationSummary {
    fx.settle().await;
    let mut list = fx.svc.list(ApplicationListQuery::default()).await.unwrap();
    list.applications.remove(0)
}

async fn deployed_api(fx: &Fx) -> (Pipeline, Pipeline, ApplicationDetail) {
    let ci = fx.provider("ci").await;
    let gitops = fx.provider("gitops").await;
    let build = builder(ci, "build", vec![source("git.test/shop/api", &[], None)]);
    fx.apply(ci, vec![build.clone()]).await;
    let run = run_with(
        &build.id,
        1,
        vec![source("git.test/shop/api", &[], Some('1'))],
        vec![artifact(IMAGE, Some(digest('a')), None)],
    );
    fx.runs(ci, &build.id, vec![run]).await;
    let mut api = unit(gitops, "api-prod");
    api.correlation.sources = vec![source("git.test/shop/config", &["apps/api"], Some('2'))];
    api.correlation.targets = vec![target("1", "shop", "shop")];
    api.correlation.consumed = vec![consumed(IMAGE, digest('a'), Some(0))];
    fx.apply(gitops, vec![api.clone()]).await;
    fx.settle().await;
    let detail = fx.svc.detail(fx.app("api-prod").await.id).await.unwrap();
    (build, api, detail)
}

#[tokio::test]
async fn build_links_to_deploy_by_digest_flow() {
    let fx = Fx::new().await;
    let (build, api, detail) = deployed_api(&fx).await;

    let why = copy::why_run_produced(1, "shop");
    let link = detail.flow.links.iter().find(|l| l.why == why).unwrap();
    assert_eq!(link.confidence, Confidence::Exact);
    assert_eq!(link.unlink_pipeline_id.as_deref(), Some(build.id.as_str()));
    assert!(link.edge_id.is_some());
    assert_eq!(link.from, format!("build:{}", build.id));

    let node = |id: &str| detail.flow.nodes.iter().find(|n| n.id == id).unwrap();
    assert_eq!(node("source:git.test/shop/api").column, 0);
    assert_eq!(node(&format!("build:{}", build.id)).column, 1);
    assert_eq!(node("artifact:registry.test/shop/api").column, 2);
    let source = node("source:git.test/shop/api");
    assert_eq!(source.title, "shop/api");
    assert_eq!(source.subtitle.as_deref(), Some("git.test"));
    let artifact = node("artifact:registry.test/shop/api");
    assert_eq!(artifact.title, "shop/api");
    assert_eq!(artifact.subtitle.as_deref(), Some("registry.test"));
    let shop = node("target:uid:1|shop");
    assert_eq!((shop.column, shop.kind), (3, FlowNodeKind::Target));
    assert_eq!(shop.pipeline_id.as_deref(), Some(api.id.as_str()));
    assert_eq!((detail.flow.columns, detail.flow.rows), (4, 2));
    assert_eq!(detail.members.len(), 2);
    assert!(detail.suggestions.is_empty());
    assert!(!detail.config_defined);
    assert!(detail.summary.capabilities.edit);
}

#[tokio::test]
async fn copy_as_toml_describes_the_application_as_config() {
    let fx = Fx::new().await;
    let (build, api, detail) = deployed_api(&fx).await;

    let config: PipedashConfig = ::toml::from_str(detail.toml.as_deref().unwrap()).unwrap();
    let app = &config.applications["api-prod"];
    assert_eq!(app.name, "api-prod");
    let member = |provider: &str, id: &str| app.members.iter().any(|m| m.matches(provider, id));
    assert!(member("ci", build.id.rsplit("__").next().unwrap()));
    assert!(member("gitops", api.id.rsplit("__").next().unwrap()));
    assert_eq!(app.members.len(), 2);
    let table = &app.targets["shop"];
    assert_eq!(table.clusters, ["uid:1"]);
    assert_eq!(table.namespaces, ["shop"]);
}

#[tokio::test]
async fn list_filters_and_sorts_in_the_backend() {
    let fx = Fx::new().await;
    let ci = fx.provider("ci").await;
    let gitops = fx.provider("gitops").await;
    let compile = builder(
        ci,
        "compile",
        vec![source("git.test/shop/alpha", &[], None)],
    );
    let mut alpha = unit(ci, "alpha");
    alpha.correlation.sources = vec![source("git.test/shop/alpha", &[], Some('1'))];
    alpha.correlation.targets = vec![target("1", "alpha", "alpha")];
    let mut beta = unit(ci, "beta");
    beta.status = PipelineStatus::Failed;
    beta.correlation.targets = vec![target("1", "beta", "beta")];
    fx.apply(ci, vec![compile, alpha, beta]).await;
    let mut gamma = unit(gitops, "gamma");
    gamma.correlation.targets = vec![target("1", "gamma", "gamma")];
    fx.apply(gitops, vec![gamma]).await;
    let query = ApplicationListQuery::default;

    assert_eq!(names(&fx, query()).await, ["alpha", "beta", "gamma"]);
    let all = fx.svc.list(query()).await.unwrap();
    assert_eq!((all.total, all.suggestion_count), (3, 0));
    let search = |text: &str| ApplicationListQuery {
        search: Some(text.into()),
        ..query()
    };
    assert_eq!(names(&fx, search("BET")).await, ["beta"]);
    assert_eq!(names(&fx, search("compil")).await, ["alpha"]);
    let health = |status| ApplicationListQuery {
        health: vec![status],
        ..query()
    };
    assert_eq!(names(&fx, health(PipelineStatus::Failed)).await, ["beta"]);
    assert_eq!(
        names(&fx, health(PipelineStatus::Success)).await,
        ["alpha", "gamma"]
    );
    let providers = ApplicationListQuery {
        provider_ids: vec![gitops],
        ..query()
    };
    assert_eq!(names(&fx, providers).await, ["gamma"]);
    let sorted = |sort, direction| ApplicationListQuery {
        sort,
        direction,
        ..query()
    };
    let by_health = sorted(ApplicationSort::Health, SortDirection::Desc);
    assert_eq!(names(&fx, by_health).await, ["beta", "alpha", "gamma"]);
    let by_name = sorted(ApplicationSort::Name, SortDirection::Desc);
    assert_eq!(names(&fx, by_name).await, ["gamma", "beta", "alpha"]);
    let behind = ApplicationListQuery {
        behind_only: true,
        ..query()
    };
    assert!(names(&fx, behind).await.is_empty());

    let id = fx.app("alpha").await.id;
    fx.svc
        .apply_edit(id, ApplicationEdit::Hide { hidden: true }, None)
        .await
        .unwrap();
    assert_eq!(names(&fx, query()).await, ["beta", "gamma"]);
    let shown = ApplicationListQuery {
        include_hidden: true,
        ..query()
    };
    assert_eq!(names(&fx, shown).await, ["alpha", "beta", "gamma"]);
    assert_eq!(fx.svc.list(query()).await.unwrap().total, 3);
}

#[tokio::test]
async fn open_suggestions_show_in_the_list_and_as_dashed_siblings() {
    let fx = Fx::new().await;
    let ci = fx.provider("ci").await;
    let mut first = unit(ci, "checkout-a");
    first.correlation.app_label = Some("shop".into());
    first.correlation.targets = vec![target("1", "a", "a")];
    let mut second = unit(ci, "payments-b");
    second.correlation.app_label = Some("shop".into());
    second.correlation.targets = vec![target("1", "b", "b")];
    let mut third = unit(ci, "solo");
    third.correlation.targets = vec![target("1", "c", "c")];
    fx.apply(ci, vec![first, second.clone(), third]).await;
    fx.settle().await;

    let list = fx.svc.list(ApplicationListQuery::default()).await.unwrap();
    assert_eq!(list.suggestion_count, 1);
    let with = ApplicationListQuery {
        with_suggestions: true,
        ..ApplicationListQuery::default()
    };
    assert_eq!(names(&fx, with).await, ["checkout-a", "payments-b"]);
    assert_eq!(fx.svc.suggestions().await.unwrap().len(), 1);

    let detail = fx.svc.detail(fx.app("checkout-a").await.id).await.unwrap();
    assert_eq!(detail.summary.suggestion_count, 1);
    let dashed = detail
        .flow
        .links
        .iter()
        .find(|l| l.confidence == Confidence::Suggested)
        .unwrap();
    assert_eq!(dashed.to, format!("sibling:{}", second.id));
    assert_eq!(
        dashed.suggestion_id.as_deref(),
        Some(detail.suggestions[0].id.as_str())
    );
}

#[tokio::test]
async fn more_than_six_targets_collapse_into_one_summary_chip() {
    let fx = Fx::new().await;
    let ci = fx.provider("ci").await;
    let units = |count: usize| {
        let mut api = unit(ci, "api");
        api.correlation.targets = (0..count)
            .map(|i| target("1", &format!("ns{i}"), &format!("ns{i}")))
            .collect();
        api
    };

    fx.apply(ci, vec![units(6)]).await;
    let six = first_app(&fx).await;
    assert_eq!(six.chips.len(), 6);
    assert!(six.chips.iter().all(|c| c.kind == ChipKind::Target));

    fx.apply(ci, vec![units(7)]).await;
    let seven = first_app(&fx).await;
    assert_eq!(seven.chips.len(), 1);
    assert_eq!(seven.chips[0].kind, ChipKind::Summary);
    assert_eq!(seven.chips[0].label, copy::health_summary(7, 7, 0));
    assert!(seven.arrows.is_empty());
}

#[tokio::test]
async fn arrows_and_columns_come_only_from_directions() {
    let fx = Fx::new().await;
    let ci = fx.provider("ci").await;
    let mut qa = unit(ci, "qa");
    qa.correlation.app_ref = Some(app_ref("checkout"));
    qa.correlation.targets = vec![target("1", "qa", "QA")];
    let mut prod = unit(ci, "prod");
    prod.correlation.app_ref = Some(app_ref("checkout"));
    prod.correlation.targets = vec![target("1", "prod", "Prod")];
    prod.correlation.unit_depends_on = vec![UnitId::new("qa").unwrap()];
    let mut preview = unit(ci, "preview");
    preview.correlation.app_ref = Some(app_ref("checkout"));
    preview.correlation.targets = vec![target("1", "preview", "Preview")];
    fx.apply(ci, vec![qa, prod, preview]).await;
    fx.settle().await;

    let detail = fx.svc.detail(fx.app("checkout").await.id).await.unwrap();
    assert_eq!(detail.summary.chips.len(), 3);
    assert_eq!(
        detail.summary.arrows,
        [ChipArrow {
            from: "uid:1|qa".into(),
            to: "uid:1|prod".into()
        }]
    );
    let column = |id: &str| {
        detail
            .flow
            .nodes
            .iter()
            .find(|n| n.id == id)
            .map(|n| n.column)
    };
    assert_eq!(column("target:uid:1|qa"), Some(3));
    assert_eq!(column("target:uid:1|preview"), Some(3));
    assert_eq!(column("target:uid:1|prod"), Some(4));
    assert!(detail
        .flow
        .links
        .iter()
        .any(|l| l.from == "target:uid:1|qa" && l.to == "target:uid:1|prod"));
}

#[tokio::test]
async fn chips_follow_target_directions() {
    let fx = Fx::new().await;
    let ci = fx.provider("ci").await;
    let stage = |name: &str, label: &str, after: Option<&str>| {
        let mut p = unit(ci, name);
        p.correlation.app_ref = Some(app_ref("checkout"));
        p.correlation.targets = vec![target("1", name, label)];
        p.correlation.unit_depends_on = after.and_then(UnitId::new).into_iter().collect();
        p
    };
    let mut preview = stage("preview", "Preview", None);
    preview.correlation.targets[0].lifetime = TargetLifetime::ShortLived;
    fx.apply(
        ci,
        vec![
            stage("prod", "Prod", Some("staging")),
            stage("staging", "Staging", Some("qa")),
            stage("qa", "QA", None),
            stage("alpha", "Alpha", None),
            preview,
        ],
    )
    .await;
    fx.settle().await;

    let id = fx.app("checkout").await.id;
    let detail = fx.svc.detail(id).await.unwrap();
    let labels: Vec<&str> = detail
        .summary
        .chips
        .iter()
        .map(|c| c.label.as_str())
        .collect();
    assert_eq!(
        labels,
        [
            "Alpha",
            "QA",
            "Staging",
            "Prod",
            copy::short_lived(1).as_str()
        ]
    );
    let targets: Vec<&str> = detail
        .targets
        .iter()
        .map(|t| t.group_label.as_str())
        .collect();
    assert_eq!(
        targets,
        [
            "Alpha",
            "QA",
            "Staging",
            "Prod",
            copy::short_lived(1).as_str()
        ]
    );
    let page = fx.svc.changes(id, None, 10).await.unwrap();
    let columns: Vec<&str> = page
        .columns
        .iter()
        .skip(1)
        .map(|c| c.label.as_str())
        .collect();
    assert_eq!(columns, ["Alpha", "QA", "Staging", "Prod"]);
}

#[tokio::test]
async fn changes_page_has_one_cell_per_stage_and_marks_the_live_commit() {
    let fx = Fx::new().await;
    let ci = fx.provider("ci").await;
    let gitops = fx.provider("gitops").await;
    let repo = "git.test/shop/api";
    let build = builder(ci, "build", vec![source(repo, &[], None)]);
    fx.apply(ci, vec![build.clone()]).await;
    let mut api = unit(gitops, "api-prod");
    api.correlation.sources = vec![source(repo, &[], Some('2'))];
    api.correlation.targets = vec![target("1", "shop", "shop")];
    fx.apply(gitops, vec![api.clone()]).await;
    let builds = ['1', '2', '3']
        .iter()
        .zip(1..)
        .map(|(c, n)| run_with(&build.id, n, vec![source(repo, &[], Some(*c))], Vec::new()))
        .collect();
    fx.runs(ci, &build.id, builds).await;
    let deploy = run_with(&api.id, 1, vec![source(repo, &[], Some('2'))], Vec::new());
    fx.runs(gitops, &api.id, vec![deploy]).await;
    fx.settle().await;
    let id = fx.app("api-prod").await.id;

    let page = fx.svc.changes(id, None, 10).await.unwrap();
    let keys: Vec<&str> = page.columns.iter().map(|c| c.key.as_str()).collect();
    assert_eq!(keys, ["build", "uid:1|shop"]);
    assert_eq!(page.rows.len(), 3);
    assert!(!page.has_more);
    let row = |c: char| {
        let sha = c.to_string().repeat(40);
        page.rows.iter().find(|r| r.commit_sha == sha).unwrap()
    };
    let live = &row('2').cells[1];
    assert_eq!(live.state, CellState::Live);
    assert_eq!(live.run_number, Some(1));
    assert_eq!(live.pipeline_id.as_deref(), Some(api.id.as_str()));
    assert_eq!(row('2').cells[0].state, CellState::Success);
    assert_eq!(row('2').cells[0].run_number, Some(2));
    assert_eq!(row('3').cells[1].state, CellState::NoEvidence);
    assert_eq!(row('1').commit_short, "1111111");

    let first = fx.svc.changes(id, None, 2).await.unwrap();
    assert!(first.has_more);
    let second = fx
        .svc
        .changes(id, first.next_cursor.clone(), 2)
        .await
        .unwrap();
    assert_eq!((second.rows.len(), second.has_more), (1, false));
    assert!(fx.svc.changes(id, Some("nope".into()), 2).await.is_err());
}

const CONFIG: &str = "[providers.ci]\nname = \"CI\"\ntype = \"stub\"\ntoken = \"t\"\nrefresh_interval = 30\n\n[applications.shop]\nname = \"Shop\"\n\n[[applications.shop.members]]\nprovider = \"ci\"\npipeline = \"shop-*\"\n";

#[tokio::test]
async fn config_defined_application_is_read_only_and_has_no_toml() {
    let fx = Fx::new().await;
    std::fs::write(fx.h.config_state.config_path(), CONFIG).unwrap();
    fx.h.provider_service
        .apply_config_file(&fx.h.config_state)
        .await
        .unwrap();
    let provider =
        fx.h.repository
            .config_backend()
            .list_providers()
            .await
            .unwrap()
            .into_iter()
            .find(|p| p.name == "ci")
            .and_then(|p| p.id)
            .unwrap();
    let mut api = unit(provider, "shop-api");
    api.correlation.targets = vec![target("1", "shop", "shop")];
    fx.apply(provider, vec![api]).await;
    fx.settle().await;

    let list = fx.svc.list(ApplicationListQuery::default()).await.unwrap();
    let summary = &list.applications[0];
    assert_eq!(summary.name, "Shop");
    assert_eq!(summary.identity_kind, IdentityKind::Config);
    assert!(!summary.capabilities.edit && !summary.capabilities.delete);
    let detail = fx.svc.detail(summary.id).await.unwrap();
    assert!(detail.config_defined);
    assert!(detail.toml.is_none());
    assert!(!detail.summary.capabilities.edit);
}
