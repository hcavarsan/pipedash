use chrono::Utc;
use pipedash_plugin_api::{
    CommitComparison,
    CommitInfo,
};

use crate::application::services::correlation_service::copy::{
    self,
    views,
};
use crate::application::services::correlation_service::tests::{
    builder,
    run_with,
    sha,
    source,
    target,
    unit,
    Fx,
};
use crate::application::test_support::StubProvider;
use crate::domain::application::dto::{
    ApplicationRef,
    LocationState,
};
use crate::domain::application::overrides::ApplicationEdit;
use crate::domain::{
    Pipeline,
    PipelineStatus,
};

const REPO: &str = "git.test/shop/api";

struct Shop {
    build: Pipeline,
    api: Pipeline,
    ci: i64,
    gitops: i64,
}

async fn shop(fx: &Fx) -> Shop {
    let ci = fx.provider("ci").await;
    let gitops = fx.provider("gitops").await;
    let build = builder(ci, "build", vec![source(REPO, &[], None)]);
    fx.apply(ci, vec![build.clone()]).await;
    let mut api = unit(gitops, "api-prod");
    api.correlation.sources = vec![source(REPO, &[], Some('2'))];
    api.correlation.targets = vec![target("1", "shop", "shop")];
    fx.apply(gitops, vec![api.clone()]).await;
    fx.settle().await;
    Shop {
        build,
        api,
        ci,
        gitops,
    }
}

fn comparison(ahead: u32, behind: u32, commits: &[char]) -> CommitComparison {
    CommitComparison {
        ahead,
        behind,
        commits: commits.iter().map(|c| sha(*c)).collect(),
    }
}

fn info(letter: char, message: &str) -> CommitInfo {
    CommitInfo {
        sha: sha(letter),
        authored_at: Utc::now(),
        committed_at: Utc::now(),
        message: message.into(),
        parents: Vec::new(),
    }
}

async fn git_host(fx: &Fx, comparison: CommitComparison) -> std::sync::Arc<StubProvider> {
    fx.h.add_stub(|id| {
        StubProvider::new(id)
            .with_source_hosts(&["git.test"])
            .with_comparison(comparison)
            .with_commit(info('2', "fix: the cart\n\nlong body"))
    })
    .await
    .1
}

#[tokio::test]
async fn commit_locations_tell_live_behind_and_not_deployed() {
    let fx = Fx::new().await;
    let shop = shop(&fx).await;
    let builds = ['1', '2', '3']
        .iter()
        .zip(1..)
        .map(|(c, n)| {
            run_with(
                &shop.build.id,
                n,
                vec![source(REPO, &[], Some(*c))],
                Vec::new(),
            )
        })
        .collect();
    fx.runs(shop.ci, &shop.build.id, builds).await;
    fx.settle().await;

    let live = fx.svc.commit_locations(&shop.build.id, 2).await.unwrap();
    assert_eq!(live.title, copy::commit_locations_title());
    let entry = &live.entries[0];
    assert_eq!(entry.application_name, "api-prod");
    assert_eq!(entry.target_label, "shop");
    assert_eq!(entry.state, LocationState::Live);
    assert_eq!(entry.label, copy::location_live());

    let unknown = fx.svc.commit_locations(&shop.build.id, 3).await.unwrap();
    assert_eq!(unknown.entries[0].state, LocationState::NotDeployed);
    assert_eq!(unknown.entries[0].label, copy::location_not_deployed());

    let host = git_host(&fx, comparison(1, 0, &['3'])).await;
    let behind = fx.svc.commit_locations(&shop.build.id, 3).await.unwrap();
    assert_eq!(behind.entries[0].state, LocationState::Behind);
    assert_eq!(behind.entries[0].label, views::location_behind(1));

    *host.comparison.lock().unwrap() = Some(comparison(0, 1, &[]));
    let contained = fx.svc.commit_locations(&shop.build.id, 1).await.unwrap();
    assert_eq!(contained.entries[0].state, LocationState::Live);

    let outside = fx.svc.commit_locations(&shop.api.id, 99).await.unwrap();
    assert!(outside.entries.is_empty());
}

#[tokio::test]
async fn pipeline_applications_skip_hidden_applications() {
    let fx = Fx::new().await;
    let shop = shop(&fx).await;
    let id = fx.app("api-prod").await.id;

    let found = fx.svc.pipeline_applications(&shop.build.id).await.unwrap();
    assert_eq!(
        found,
        [ApplicationRef {
            id,
            name: "api-prod".into()
        }]
    );
    assert!(fx
        .svc
        .pipeline_applications("stub__1__missing")
        .await
        .unwrap()
        .is_empty());

    fx.svc
        .apply_edit(id, ApplicationEdit::Hide { hidden: true }, None)
        .await
        .unwrap();
    assert!(fx
        .svc
        .pipeline_applications(&shop.build.id)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn deploy_changes_list_the_commits_since_the_last_live_deploy() {
    let fx = Fx::new().await;
    let shop = shop(&fx).await;
    let good = run_with(
        &shop.api.id,
        1,
        vec![source(REPO, &[], Some('1'))],
        Vec::new(),
    );
    let mut failed = run_with(
        &shop.api.id,
        2,
        vec![source(REPO, &[], Some('3'))],
        Vec::new(),
    );
    failed.status = PipelineStatus::Failed;
    fx.runs(shop.gitops, &shop.api.id, vec![good, failed]).await;
    fx.settle().await;

    let first = fx.svc.deploy_changes(&shop.api.id, 1).await.unwrap();
    assert_eq!(first.title, copy::deploy_changes_title());
    assert_eq!(first.unavailable_reason, Some(views::no_previous_deploy()));

    let silent = fx.svc.deploy_changes(&shop.api.id, 2).await.unwrap();
    assert!(silent.commits.is_empty());
    assert_eq!(
        silent.unavailable_reason,
        Some(views::changes_unavailable())
    );

    git_host(&fx, comparison(2, 0, &['2', '3'])).await;
    let found = fx.svc.deploy_changes(&shop.api.id, 2).await.unwrap();
    assert_eq!(found.unavailable_reason, None);
    let shas: Vec<&str> = found.commits.iter().map(|c| c.short.as_str()).collect();
    assert_eq!(shas, ["2222222", "3333333"]);
    assert_eq!(found.commits[0].message.as_deref(), Some("fix: the cart"));
    assert!(found.commits[0].authored_at.is_some());
}
