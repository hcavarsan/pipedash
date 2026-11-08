use crate::application::test_support::{
    pipeline,
    stub_run,
    Harness,
    StubProvider,
};
use crate::domain::{
    DomainError,
    PipelineRun,
    PipelineStatus,
    RunHistoryQuery,
    SortDirection,
};

fn run_as(pipeline_id: &str, number: i64, status: PipelineStatus) -> PipelineRun {
    let mut run = stub_run(pipeline_id, number, 60);
    run.status = status;
    run
}

async fn seeded(
    h: &Harness, runs: impl Fn(&str) -> Vec<PipelineRun>,
) -> (String, std::sync::Arc<StubProvider>) {
    let (provider_id, stub) = h.add_stub(StubProvider::new).await;
    let pipeline = pipeline(provider_id, "web", PipelineStatus::Success);
    h.repository
        .update_pipelines_cache(provider_id, std::slice::from_ref(&pipeline))
        .await
        .unwrap();
    h.repository
        .cache_run_history(&pipeline.id, &runs(&pipeline.id))
        .await
        .unwrap();
    (pipeline.id, stub)
}

#[tokio::test]
async fn query_run_history_should_count_filtered_runs_across_all_cached_pages() {
    let h = Harness::new().await;
    let (id, _) = seeded(&h, |id| {
        (1..=25)
            .map(|number| {
                let status = if number % 4 == 1 {
                    PipelineStatus::Failed
                } else {
                    PipelineStatus::Success
                };
                run_as(id, number, status)
            })
            .collect()
    })
    .await;

    let page = h
        .pipeline_service
        .query_run_history(
            &id,
            RunHistoryQuery {
                page: 1,
                page_size: 10,
                statuses: vec![PipelineStatus::Failed],
                sort_column: "run_number".into(),
                direction: SortDirection::Desc,
                ..Default::default()
            },
        )
        .await
        .unwrap();

    let numbers: Vec<i64> = page.runs.iter().map(|run| run.run_number).collect();
    assert_eq!(page.total_count, 7);
    assert_eq!(numbers.len(), 7);
    assert!(numbers.windows(2).all(|pair| pair[0] > pair[1]));
    assert!(!page.has_more);
    h.pipeline_service.settle().await;
}

#[tokio::test]
async fn query_run_history_should_paginate_after_filtering_and_sorting() {
    let h = Harness::new().await;
    let (id, _) = seeded(&h, |id| {
        (1..=30)
            .map(|number| run_as(id, number, PipelineStatus::Success))
            .collect()
    })
    .await;

    let second = h
        .pipeline_service
        .query_run_history(
            &id,
            RunHistoryQuery {
                page: 2,
                page_size: 10,
                direction: SortDirection::Asc,
                ..Default::default()
            },
        )
        .await
        .unwrap();

    let numbers: Vec<i64> = second.runs.iter().map(|run| run.run_number).collect();
    assert_eq!(numbers, (11..=20).collect::<Vec<_>>());
    assert_eq!(second.total_count, 30);
    assert!(second.has_more);
    assert_eq!(second.total_pages, 3);
    h.pipeline_service.settle().await;
}

#[tokio::test]
async fn query_run_history_should_filter_by_branch_actor_and_search_and_list_facets() {
    let h = Harness::new().await;
    let (id, _) = seeded(&h, |id| {
        (1..=6)
            .map(|number| {
                let mut run = run_as(id, number, PipelineStatus::Success);
                run.branch = Some(if number % 2 == 0 { "main" } else { "dev" }.to_string());
                run.actor = Some(if number <= 3 { "ada" } else { "bob" }.to_string());
                run.commit_message = Some(format!("Fix thing {number}"));
                run
            })
            .collect()
    })
    .await;
    let query = |branch: Option<&str>, actor: Option<&str>, search: Option<&str>| RunHistoryQuery {
        branch: branch.map(str::to_string),
        actor: actor.map(str::to_string),
        search: search.map(str::to_string),
        ..Default::default()
    };

    let by_branch = h
        .pipeline_service
        .query_run_history(&id, query(Some("main"), None, None))
        .await
        .unwrap();
    let by_actor = h
        .pipeline_service
        .query_run_history(&id, query(None, Some("bob"), None))
        .await
        .unwrap();
    let by_search = h
        .pipeline_service
        .query_run_history(&id, query(None, None, Some("FIX THING 5")))
        .await
        .unwrap();

    assert_eq!(by_branch.total_count, 3);
    assert_eq!(by_actor.total_count, 3);
    assert_eq!(by_search.total_count, 1);
    assert_eq!(by_search.runs[0].run_number, 5);
    assert_eq!(by_branch.branches, ["dev", "main"]);
    assert_eq!(by_branch.actors, ["ada", "bob"]);
    h.pipeline_service.settle().await;
}

#[tokio::test]
async fn query_run_history_should_sort_by_schema_column_and_reject_unknown_columns() {
    let h = Harness::new().await;
    let (id, _) = seeded(&h, |id| {
        [(1, "main"), (2, "alpha"), (3, "Zulu")]
            .into_iter()
            .map(|(number, branch)| {
                let mut run = run_as(id, number, PipelineStatus::Success);
                run.branch = Some(branch.to_string());
                run
            })
            .collect()
    })
    .await;

    let sorted = h
        .pipeline_service
        .query_run_history(
            &id,
            RunHistoryQuery {
                sort_column: "branch".into(),
                direction: SortDirection::Asc,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let unknown = h
        .pipeline_service
        .query_run_history(
            &id,
            RunHistoryQuery {
                sort_column: "id; DROP TABLE".into(),
                ..Default::default()
            },
        )
        .await;

    let order: Vec<i64> = sorted.runs.iter().map(|run| run.run_number).collect();
    assert_eq!(order, [2, 1, 3]);
    assert!(matches!(unknown, Err(DomainError::InvalidConfig(_))));
    h.pipeline_service.settle().await;
}

#[tokio::test]
async fn query_run_history_should_fetch_deeper_history_until_filters_are_satisfied() {
    let h = Harness::new().await;
    let (provider_id, _) = h
        .add_stub(|id| {
            let pipeline_id = format!("stub__{id}__web");
            StubProvider::new(id).with_runs(
                (1..=150)
                    .rev()
                    .map(|number| run_as(&pipeline_id, number, PipelineStatus::Success))
                    .collect(),
            )
        })
        .await;
    let pipeline = pipeline(provider_id, "web", PipelineStatus::Success);
    h.repository
        .update_pipelines_cache(provider_id, std::slice::from_ref(&pipeline))
        .await
        .unwrap();

    let page = h
        .pipeline_service
        .query_run_history(&pipeline.id, RunHistoryQuery::page(1, 10))
        .await
        .unwrap();

    assert_eq!(page.runs.len(), 10);
    assert_eq!(page.total_count, 100);
    assert!(page.has_more);
    assert!(!page.is_complete);
    h.pipeline_service.settle().await;
}

#[tokio::test]
async fn query_run_history_should_refresh_bounded_history_even_when_the_cache_fills_the_page() {
    let h = Harness::new().await;
    let (provider_id, _) = h
        .add_stub(|id| {
            let pipeline_id = format!("stub__{id}__web");
            StubProvider::new(id).bounded_history().with_runs(
                (1..=31)
                    .rev()
                    .map(|number| run_as(&pipeline_id, number, PipelineStatus::Success))
                    .collect(),
            )
        })
        .await;
    let mut pipeline = pipeline(provider_id, "web", PipelineStatus::Success);
    pipeline.provider_type = crate::application::test_support::plugins::STUB_BOUNDED.to_string();
    h.repository
        .update_pipelines_cache(provider_id, std::slice::from_ref(&pipeline))
        .await
        .unwrap();
    let stale: Vec<_> = (1..=30)
        .map(|number| run_as(&pipeline.id, number, PipelineStatus::Success))
        .collect();
    h.repository
        .cache_run_history(&pipeline.id, &stale)
        .await
        .unwrap();

    let page = h
        .pipeline_service
        .query_run_history(&pipeline.id, RunHistoryQuery::page(1, 10))
        .await
        .unwrap();

    assert!(page.runs.iter().any(|run| run.run_number == 31));
    assert_eq!(page.total_count, 31);
    h.pipeline_service.settle().await;
}

#[tokio::test]
async fn query_run_history_should_offer_more_pages_when_a_filtered_scan_hits_its_budget() {
    let h = Harness::new().await;
    let (provider_id, _) = h
        .add_stub(|id| {
            let pipeline_id = format!("stub__{id}__web");
            StubProvider::new(id).with_runs(
                (1..=1_200)
                    .rev()
                    .map(|number| {
                        let status = if number == 1 {
                            PipelineStatus::Failed
                        } else {
                            PipelineStatus::Success
                        };
                        run_as(&pipeline_id, number, status)
                    })
                    .collect(),
            )
        })
        .await;
    let pipeline = pipeline(provider_id, "web", PipelineStatus::Success);
    h.repository
        .update_pipelines_cache(provider_id, std::slice::from_ref(&pipeline))
        .await
        .unwrap();
    let failed = |page| RunHistoryQuery {
        page,
        page_size: 10,
        statuses: vec![PipelineStatus::Failed],
        ..RunHistoryQuery::page(page, 10)
    };

    let first = h
        .pipeline_service
        .query_run_history(&pipeline.id, failed(1))
        .await
        .unwrap();
    assert_eq!(first.total_count, 0);
    assert!(first.has_more);
    assert!(!first.is_complete);

    let mut page = 2;
    let last = loop {
        let next = h
            .pipeline_service
            .query_run_history(&pipeline.id, failed(page))
            .await
            .unwrap();
        if !next.has_more {
            break next;
        }
        page += 1;
    };
    assert!(last.is_complete);
    assert_eq!(last.total_count, 1);

    h.pipeline_service.settle().await;
}

#[tokio::test]
async fn query_run_history_should_scan_deeper_on_the_same_page_when_asked() {
    let h = Harness::new().await;
    let (provider_id, _) = h
        .add_stub(|id| {
            let pipeline_id = format!("stub__{id}__web");
            StubProvider::new(id).with_runs(
                (1..=1_200)
                    .rev()
                    .map(|number| {
                        let status = if number == 1 {
                            PipelineStatus::Failed
                        } else {
                            PipelineStatus::Success
                        };
                        run_as(&pipeline_id, number, status)
                    })
                    .collect(),
            )
        })
        .await;
    let pipeline = pipeline(provider_id, "web", PipelineStatus::Success);
    h.repository
        .update_pipelines_cache(provider_id, std::slice::from_ref(&pipeline))
        .await
        .unwrap();

    let page = h
        .pipeline_service
        .query_run_history(
            &pipeline.id,
            RunHistoryQuery {
                statuses: vec![PipelineStatus::Failed],
                scan_depth: 3,
                ..RunHistoryQuery::page(1, 10)
            },
        )
        .await
        .unwrap();

    assert_eq!(page.total_count, 1);
    assert!(!page.has_more);
    assert!(page.is_complete);
    h.pipeline_service.settle().await;
}
