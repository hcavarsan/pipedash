use pipedash_plugin_api::{
    Pipeline,
    PipelineRun,
};
use serde_json::{
    from_value,
    json,
    Value,
};

use crate::mapper::{
    map_pipeline,
    map_run,
};
use crate::types::{
    StackListItem,
    Update,
    UpdateListItem,
};

const SERVER_URL: &str = "https://komodo.test";
const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

fn stack() -> Value {
    json!({
        "id": "66a1", "type": "Stack", "name": "web", "template": false,
        "info": {
            "server_id": "66s1", "server_name": "prod-1", "repo": "acme/web", "branch": "main",
            "repo_link": "https://github.com/acme/web/tree/main", "state": "running",
            "deployed_hash": SHA
        }
    })
}

fn updates() -> Value {
    json!([
        {
            "id": "66u2", "operation": "DeployStack", "start_ts": 1761660100000_i64,
            "success": true, "status": "Complete"
        },
        {
            "id": "66u1", "operation": "DeployStack", "start_ts": 1761660000000_i64,
            "success": false, "status": "Complete"
        }
    ])
}

fn detail() -> Value {
    json!({
        "_id": "66u2", "start_ts": 1761660100000_i64, "end_ts": 1761660142500_i64,
        "success": true, "status": "Complete", "commit_hash": SHA
    })
}

fn fixture() -> Option<(Pipeline, Vec<PipelineRun>)> {
    let stack: StackListItem = from_value(stack()).ok()?;
    let items: Vec<UpdateListItem> = from_value(updates()).ok()?;
    let detail: Update = from_value(detail()).ok()?;
    let pipeline = map_pipeline(&stack, items.first().map(|item| item.start_ts));
    let runs = items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let detail = (index == 0).then_some(&detail);
            map_run(&stack.id, SERVER_URL, item, detail, Some(&stack.info))
        })
        .collect();
    Some((pipeline, runs))
}

pub fn correlation_fixtures() -> Vec<(Pipeline, Vec<PipelineRun>)> {
    fixture().into_iter().collect()
}

#[cfg(test)]
mod tests;
