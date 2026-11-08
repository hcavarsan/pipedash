use pipedash_plugin_api::*;

pub fn create_table_schema() -> schema::TableSchema {
    let mut runs = defaults::default_pipeline_runs_table();
    runs.insert_after(
        "run_number",
        [columns::trigger_event_column(
            "Event that triggered the workflow (push, pull_request, workflow_dispatch, etc.)",
        )],
    );
    defaults::table_schema(runs, defaults::default_pipelines_table())
}
