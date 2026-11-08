use pipedash_plugin_api::{
    Plugin,
    PluginResult,
    TriggerParams,
    TriggerResult,
};
use serde_json::Value;

use super::JenkinsPlugin;

pub(super) fn form_data(inputs: Option<Value>) -> Vec<(String, String)> {
    let Some(Value::Object(inputs)) = inputs else {
        return Vec::new();
    };
    let text = |value: &Value| match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    let mut form = Vec::new();
    for (key, value) in inputs {
        match &value {
            Value::Null | Value::Object(_) => {}
            Value::Array(items) => form.extend(
                items
                    .iter()
                    .filter(|item| !item.is_null())
                    .map(|item| (key.clone(), text(item))),
            ),
            scalar => form.push((key, text(scalar))),
        }
    }
    form
}

impl JenkinsPlugin {
    pub(super) async fn trigger_pipeline_impl(
        &self, params: TriggerParams,
    ) -> PluginResult<TriggerResult> {
        let job_path = params.workflow_id.as_str();

        let mut form = form_data(params.inputs);

        if form.is_empty()
            && !self
                .fetch_workflow_parameters(&params.workflow_id)
                .await?
                .is_empty()
        {
            form.push(("json".to_string(), serde_json::json!({}).to_string()));
        }

        let client = self.client()?;
        client.trigger_build(job_path, form).await?;

        Ok(TriggerResult::default())
    }
}
