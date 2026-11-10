use std::collections::HashMap;
use std::sync::Arc;

use pipedash_plugin_api::config::{
    credential,
    insecure,
    selected_items,
};
use pipedash_plugin_api::http::require_client;
use pipedash_plugin_api::pagination::paginate;
use pipedash_plugin_api::support::{
    client_options,
    numeric_id,
    organizations,
    run_metadata_str,
    write_in_chunks,
};
use pipedash_plugin_api::utils::deployed_status;
use pipedash_plugin_api::*;
use tracing::{
    debug,
    info,
    warn,
};

use crate::{
    client,
    config,
    log_dialect,
    mapper,
    metadata,
    stage_cursor::StageCursor,
    types,
};

plugin_struct!(
    KomodoPlugin,
    client::KomodoClient,
    metadata::create_metadata();
    server_url: String = String::new()
);

impl KomodoPlugin {
    async fn live_status(&self, stack_id: &str) -> PluginResult<PipelineStatus> {
        let stacks = self.client()?.list_stacks().await?;
        let stack = stacks
            .iter()
            .find(|stack| stack.id == stack_id)
            .ok_or_else(|| PluginError::PipelineNotFound(format!("Stack {stack_id} not found")))?;

        Ok(mapper::map_stack_state(&stack.info).0)
    }

    async fn update_started_at(
        &self, stack_id: &str, run_number: i64,
    ) -> PluginResult<types::UpdateListItem> {
        let mut matching = self
            .client()?
            .updates_started_at(stack_id, run_number)
            .await?;
        if matching.len() > 1 {
            return Err(PluginError::ApiError(format!(
                "{} updates of stack {stack_id} share run number {run_number}; refusing to pick one",
                matching.len()
            )));
        }
        matching.pop().ok_or_else(|| {
            PluginError::PipelineNotFound(format!(
                "Update {run_number} not found for stack {stack_id}"
            ))
        })
    }

    async fn run_update_logs(
        &self, run: &RunRef<'_>, stack_id: &str,
    ) -> PluginResult<(String, types::UpdateLogs)> {
        let update_id = match run_metadata_str(run, "update_id") {
            Some(id) => id.to_string(),
            None => self.update_started_at(stack_id, run.run_number).await?.id,
        };
        let logs = self.client()?.update_logs(&update_id).await?;
        Ok((update_id, logs))
    }

    async fn write_lines<'a>(
        lines: impl Iterator<Item = (LineMarker, &'a str)>, sink: &mut LogSink,
    ) -> PluginResult<()> {
        write_in_chunks(sink, lines, |chunk, (marker, line)| {
            log_dialect::push_line(chunk, marker, line);
            Ok(())
        })
        .await
    }
}

plugin_impl!(KomodoPlugin [
    metadata,
    available_from_filtered,
] {
    fn configured_repositories(&self, _config: &HashMap<String, String>) -> Vec<String> {
        Vec::new()
    }

    fn client_options(&self, config: &HashMap<String, String>) -> ClientOptions {
        client_options(
            config::get_server_url(config).ok().as_deref(),
            insecure(config),
        )
    }

    fn initialize(
        &mut self, provider_id: i64, config: HashMap<String, String>,
        http_client: Option<Arc<reqwest::Client>>,
    ) -> PluginResult<()> {
        info!(provider_id, "Initializing Komodo plugin");

        let server_url = config::get_server_url(&config)?;
        let api_key = credential(&config, "api_key")?;
        let api_secret = credential(&config, "token")?;
        if insecure(&config) {
            warn!("Insecure TLS mode enabled - certificate verification disabled");
        }
        let http_client = require_client(http_client)?;

        self.client = Some(client::KomodoClient::new(
            http_client,
            server_url.clone(),
            api_key,
            api_secret,
        ));
        self.server_url = server_url;
        self.config = config;
        Ok(())
    }

    async fn validate_credentials(&self) -> PluginResult<bool> {
        let version = self.client()?.version().await?;
        debug!(version, "Komodo credentials validated");
        Ok(true)
    }

    async fn fetch_organizations(&self) -> PluginResult<Vec<Organization>> {
        let stacks = self.client()?.list_stacks().await?;
        Ok(organizations(
            stacks
                .iter()
                .map(|stack| stack.info.host())
                .filter(|host| !host.is_empty()),
            None,
        ))
    }

    async fn fetch_available_pipelines_filtered(
        &self, org: Option<String>, search: Option<String>, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        let search = search.map(|term| term.to_lowercase());
        let matching: Vec<AvailablePipeline> = self
            .client()?
            .list_stacks()
            .await?
            .iter()
            .map(mapper::map_available_pipeline)
            .filter(|stack| org.is_none() || stack.organization == org)
            .filter(|stack| {
                search.as_ref().is_none_or(|term| {
                    stack.name.to_lowercase().contains(term)
                        || stack
                            .description
                            .as_ref()
                            .is_some_and(|text| text.to_lowercase().contains(term))
                })
            })
            .collect();

        paginate(matching, params.unwrap_or_default())
    }

    async fn fetch_pipelines(&self) -> PluginResult<Vec<Pipeline>> {
        let selected = selected_items(&self.config);
        let client = self.client()?;
        let stacks = client.list_stacks().await?;

        let listed: Vec<&types::StackListItem> = stacks
            .iter()
            .filter(|stack| {
                selected.as_ref().is_none_or(|items| {
                    items
                        .iter()
                        .any(|item| *item == stack.id || *item == stack.name)
                })
            })
            .collect();
        let stack_ids: Vec<&str> = listed.iter().map(|stack| stack.id.as_str()).collect();
        let last_starts = client.latest_update_starts(&stack_ids).await?;

        let pipelines: Vec<Pipeline> = listed
            .iter()
            .map(|stack| mapper::map_pipeline(stack, last_starts.get(&stack.id).copied()))
            .collect();

        debug!(
            stacks = stacks.len(),
            pipelines = pipelines.len(),
            "Mapped Komodo stacks to pipelines"
        );
        Ok(pipelines)
    }

    async fn fetch_run_history(
        &self, pipeline_id: &str, limit: usize,
    ) -> PluginResult<Vec<PipelineRun>> {
        let stack_id = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client()?;
        let updates = client.stack_updates(stack_id, limit).await?;

        let mut runs: Vec<PipelineRun> = updates
            .iter()
            .map(|update| mapper::map_run(stack_id, &self.server_url, update, None))
            .collect();
        let latest = runs.iter_mut().max_by_key(|run| run.run_number);
        if let Some(latest) = latest.filter(|run| run.status == PipelineStatus::Success) {
            latest.status = deployed_status(latest.status, self.live_status(stack_id).await?);
        }

        Ok(runs)
    }

    async fn fetch_run_details(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<PipelineRun> {
        let stack_id = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client()?;

        let item = self.update_started_at(stack_id, run_number).await?;
        let detail = if item.id.is_empty() {
            None
        } else {
            Some(client.update(&item.id).await?)
        };
        let mut run = mapper::map_run(stack_id, &self.server_url, &item, detail.as_ref());
        if run.status == PipelineStatus::Success {
            let latest = client.latest_update_starts(&[stack_id]).await?;
            if latest.get(stack_id) == Some(&run_number) {
                run.status = deployed_status(run.status, self.live_status(stack_id).await?);
            }
        }

        Ok(run)
    }

    async fn fetch_log_nodes(&self, run: RunRef<'_>) -> PluginResult<Vec<LogNode>> {
        let stack_id = config::parse_pipeline_id(run.pipeline_id)?;
        let (update_id, update) = self.run_update_logs(&run, stack_id).await?;

        Ok(mapper::map_log_nodes(
            &update,
            &mapper::run_url(&self.server_url, stack_id, &update_id),
        ))
    }

    async fn read_log(
        &self, run: RunRef<'_>, node_id: &str, from: Option<&LogCursor>, sink: &mut LogSink,
    ) -> PluginResult<LogRead> {
        let index = numeric_id::<usize>(node_id, "log node ID")?;
        let stack_id = config::parse_pipeline_id(run.pipeline_id)?;
        let (_, update) = self.run_update_logs(&run, stack_id).await?;
        let log = update.logs.get(index).ok_or_else(|| {
            PluginError::InvalidConfig(format!("Update has no log stage {index}"))
        })?;

        let resumed = match from {
            None => None,
            Some(cursor) => {
                let resumed = cursor
                    .parse::<StageCursor>()
                    .and_then(|cursor| cursor.resume(log));
                if resumed.is_none() {
                    sink.restart()?;
                }
                resumed
            }
        };
        let next = LogCursor::new(&StageCursor::of(log))?;
        match resumed {
            Some((stdout, stderr)) => {
                Self::write_lines(
                    log_dialect::text_lines("", stdout, stderr, log.success),
                    sink,
                )
                .await?;
                Ok(LogRead::append(next))
            }
            None => {
                Self::write_lines(log_dialect::stage_lines(log), sink).await?;
                Ok(LogRead::replace(Some(next)))
            }
        }
    }

    fn log_dialect(&self) -> &'static dyn LogDialect {
        &log_dialect::DIALECT
    }

    async fn trigger_pipeline(&self, params: TriggerParams) -> PluginResult<TriggerResult> {
        let stack_id = config::parse_pipeline_id(&params.workflow_id)?;
        let update = self.client()?.deploy_stack(stack_id).await?;
        info!(
            stack_id,
            update_id = update.id.map(types::MongoId::into_string).unwrap_or_default(),
            "Triggered Komodo stack deploy"
        );

        Ok(TriggerResult {
            run_id: None,
            run_number: Some(update.start_ts).filter(|start_ts| *start_ts > 0),
        })
    }
});

#[cfg(test)]
mod tests;
