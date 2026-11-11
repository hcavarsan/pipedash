use chrono::{
    Duration,
    Utc,
};
use pipedash_plugin_api::defaults::default_pipeline_runs_table;

use super::{
    facets,
    sort_runs,
    Criteria,
};
use crate::application::test_support::stub_run;
use crate::domain::{
    DateRange,
    PipelineRun,
    RunHistoryQuery,
    SortDirection,
};

fn run(number: i64, branch: Option<&str>) -> PipelineRun {
    let mut run = stub_run("stub__1__web", number, 60);
    run.branch = branch.map(str::to_string);
    run
}

fn column(id: &str) -> pipedash_plugin_api::schema::ColumnDefinition {
    default_pipeline_runs_table()
        .columns
        .into_iter()
        .find(|column| column.id == id)
        .unwrap()
}

#[test]
fn criteria_should_apply_the_date_range_cutoff_to_started_at() {
    let mut old = run(1, None);
    old.started_at = Utc::now() - Duration::days(3);
    let recent = run(2, None);
    let query = RunHistoryQuery {
        date_range: Some(DateRange::Day),
        ..Default::default()
    };
    let criteria = Criteria::new(&query, Utc::now());

    assert!(!criteria.matches(&old));
    assert!(criteria.matches(&recent));
}

#[test]
fn criteria_should_ignore_blank_search_and_empty_filters() {
    let query = RunHistoryQuery {
        search: Some("   ".to_string()),
        branch: Some(String::new()),
        ..Default::default()
    };

    assert!(Criteria::new(&query, Utc::now()).matches(&run(1, None)));
}

#[test]
fn sort_runs_should_order_text_case_insensitively_with_missing_values_first() {
    let runs = vec![
        run(1, Some("beta")),
        run(2, None),
        run(3, Some("Alpha")),
        run(4, Some("alpha")),
    ];

    let ascending = sort_runs(runs.clone(), &column("branch"), SortDirection::Asc);
    let descending = sort_runs(runs, &column("branch"), SortDirection::Desc);

    let numbers = |runs: &[PipelineRun]| runs.iter().map(|run| run.run_number).collect::<Vec<_>>();
    assert_eq!(numbers(&ascending), [2, 3, 4, 1]);
    assert_eq!(numbers(&descending), [1, 4, 3, 2]);
}

#[test]
fn sort_runs_should_compare_timestamps_by_instant() {
    let mut first = run(1, None);
    first.started_at = Utc::now() - Duration::hours(2);
    let mut second = run(2, None);
    second.started_at = Utc::now() - Duration::hours(1);

    let sorted = sort_runs(
        vec![second.clone(), first.clone()],
        &column("started_at"),
        SortDirection::Asc,
    );

    assert_eq!(sorted[0].run_number, 1);
}

#[test]
fn facets_should_list_distinct_sorted_values() {
    let mut runs = vec![
        run(1, Some("main")),
        run(2, Some("dev")),
        run(3, Some("main")),
    ];
    runs[0].actor = Some("bob".to_string());
    runs[1].actor = Some("ada".to_string());

    let (branches, actors) = facets(&runs);

    assert_eq!(branches, ["dev", "main"]);
    assert_eq!(actors, ["ada", "bob"]);
}
