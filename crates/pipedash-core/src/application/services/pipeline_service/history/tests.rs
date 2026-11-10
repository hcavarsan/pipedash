use crate::application::test_support::plugins::STUB_RENAMED_RUN;
use crate::application::test_support::{
    pipeline,
    stub_run,
    Harness,
    StubProvider,
};
use crate::domain::run_history::{
    AppliedRunSort,
    RunSort,
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

fn by(column: &str, direction: SortDirection) -> Option<RunSort> {
    Some(RunSort {
        column: column.to_string(),
        direction,
    })
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

async fn provider_history(h: &Harness, runs: impl FnOnce(&str) -> Vec<PipelineRun>) -> String {
    let (provider_id, _) = h
        .add_stub(|id| {
            let pipeline_id = format!("stub__{id}__web");
            StubProvider::new(id).with_runs(runs(&pipeline_id))
        })
        .await;
    let pipeline = pipeline(provider_id, "web", PipelineStatus::Success);
    h.repository
        .update_pipelines_cache(provider_id, std::slice::from_ref(&pipeline))
        .await
        .unwrap();
    pipeline.id
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
                sort: by("run_number", SortDirection::Desc),
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
                sort: by("run_number", SortDirection::Asc),
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
                sort: by("branch", SortDirection::Asc),
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
                sort: by("id; DROP TABLE", SortDirection::Desc),
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
async fn provider_order_needs_no_run_number_column() {
    let h = Harness::new().await;
    let (provider_id, _) = h.add_stub(StubProvider::new).await;
    let mut pipeline = pipeline(provider_id, "web", PipelineStatus::Success);
    pipeline.provider_type = STUB_RENAMED_RUN.to_string();
    h.repository
        .update_pipelines_cache(provider_id, std::slice::from_ref(&pipeline))
        .await
        .unwrap();
    let runs: Vec<_> = (1..=3)
        .map(|number| run_as(&pipeline.id, number, PipelineStatus::Success))
        .collect();
    h.repository
        .cache_run_history(&pipeline.id, &runs)
        .await
        .unwrap();
    let numbers = |query| async {
        let page = h
            .pipeline_service
            .query_run_history(&pipeline.id, query)
            .await
            .unwrap();
        let numbers: Vec<i64> = page.runs.iter().map(|run| run.run_number).collect();
        (numbers, page.sort)
    };

    let (newest, applied) = numbers(RunHistoryQuery::default()).await;
    let (oldest, _) = numbers(RunHistoryQuery {
        sort: by("build", SortDirection::Asc),
        ..Default::default()
    })
    .await;

    assert_eq!(newest, [3, 2, 1]);
    assert_eq!(
        applied,
        AppliedRunSort {
            column: Some("build".to_string()),
            direction: SortDirection::Desc,
        }
    );
    assert_eq!(oldest, [1, 2, 3]);
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
async fn oldest_first_history_stays_stable_after_loading_later_pages() {
    let h = Harness::new().await;
    let id = provider_history(&h, |id| {
        (1..=150)
            .rev()
            .map(|number| run_as(id, number, PipelineStatus::Success))
            .collect()
    })
    .await;

    for (page, first) in [(1, 1_i64), (2, 21), (5, 81), (1, 1)] {
        let result = h
            .pipeline_service
            .query_run_history(
                &id,
                RunHistoryQuery {
                    sort: by("run_number", SortDirection::Asc),
                    ..RunHistoryQuery::page(page, 20)
                },
            )
            .await
            .unwrap();
        let numbers: Vec<_> = result.runs.iter().map(|run| run.run_number).collect();
        assert_eq!(numbers, (first..first + 20).collect::<Vec<_>>());
        assert_eq!(result.total_count, 150);
        assert!(result.is_complete);
    }
    h.pipeline_service.settle().await;
}

#[tokio::test]
async fn global_sort_waits_for_a_complete_scan_instead_of_sorting_a_prefix() {
    let h = Harness::new().await;
    let id = provider_history(&h, |id| {
        (1..=750)
            .rev()
            .map(|number| run_as(id, number, PipelineStatus::Success))
            .collect()
    })
    .await;
    let query = |page, scan_depth| RunHistoryQuery {
        sort: by("run_number", SortDirection::Asc),
        scan_depth,
        ..RunHistoryQuery::page(page, 20)
    };

    let pending = h
        .pipeline_service
        .query_run_history(&id, query(1, 1))
        .await
        .unwrap();
    assert!(pending.runs.is_empty());
    assert!(!pending.has_more);
    assert!(pending.can_scan_deeper);
    assert!(!pending.is_complete);
    assert!(pending.scan_notice.is_some());

    for (page, first) in [(1, 1_i64), (2, 21), (1, 1)] {
        let result = h
            .pipeline_service
            .query_run_history(&id, query(page, 2))
            .await
            .unwrap();
        let numbers: Vec<_> = result.runs.iter().map(|run| run.run_number).collect();
        assert_eq!(numbers, (first..first + 20).collect::<Vec<_>>());
        assert_eq!(result.total_count, 750);
        assert!(result.is_complete);
        assert!(result.scan_notice.is_none());
    }
    h.pipeline_service.settle().await;
}

#[tokio::test]
async fn descending_duration_sort_includes_runs_outside_the_newest_prefix() {
    let h = Harness::new().await;
    let id = provider_history(&h, |id| {
        (1..=150)
            .rev()
            .map(|number| {
                let mut run = run_as(id, number, PipelineStatus::Success);
                if number == 1 {
                    run.duration_seconds = Some(3_600);
                }
                run
            })
            .collect()
    })
    .await;

    let result = h
        .pipeline_service
        .query_run_history(
            &id,
            RunHistoryQuery {
                sort: by("duration_seconds", SortDirection::Desc),
                ..RunHistoryQuery::page(1, 20)
            },
        )
        .await
        .unwrap();
    assert_eq!(result.runs[0].run_number, 1);
    assert_eq!(result.total_count, 150);
    assert!(result.is_complete);
    h.pipeline_service.settle().await;
}

fn run_started_at_minute(pipeline_id: &str, number: i64) -> PipelineRun {
    let mut run = run_as(pipeline_id, number, PipelineStatus::Success);
    run.started_at = chrono::DateTime::UNIX_EPOCH + chrono::Duration::minutes(number);
    run
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
                    .map(|number| run_started_at_minute(&pipeline_id, number))
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
        .map(|number| run_started_at_minute(&pipeline.id, number))
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
async fn query_run_history_should_scan_deeper_on_the_same_page_instead_of_skipping_matches() {
    let h = Harness::new().await;
    let (provider_id, _) = h
        .add_stub(|id| {
            let pipeline_id = format!("stub__{id}__web");
            StubProvider::new(id).with_runs(
                (1..=1_200)
                    .rev()
                    .map(|number| {
                        let status = if number == 700 {
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
    let failed = |scan_depth| RunHistoryQuery {
        statuses: vec![PipelineStatus::Failed],
        scan_depth,
        ..RunHistoryQuery::page(1, 10)
    };

    let shallow = h
        .pipeline_service
        .query_run_history(&pipeline.id, failed(1))
        .await
        .unwrap();
    assert!(shallow.runs.is_empty());
    assert!(!shallow.has_more);
    assert!(shallow.can_scan_deeper);
    assert!(!shallow.is_complete);

    let deeper = h
        .pipeline_service
        .query_run_history(&pipeline.id, failed(2))
        .await
        .unwrap();
    let numbers: Vec<i64> = deeper.runs.iter().map(|run| run.run_number).collect();
    assert_eq!(numbers, [700]);
    assert!(!deeper.has_more);
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
