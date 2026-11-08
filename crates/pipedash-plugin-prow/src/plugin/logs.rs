use pipedash_plugin_api::{
    LogAvailability,
    LogCursor,
    LogNode,
    LogRead,
    LogReadMode,
    LogSink,
    PipelineStatus,
    PluginError,
    PluginResult,
    RunRef,
};
use serde::{
    Deserialize,
    Serialize,
};
use serde_json::Value;

use super::ProwPlugin;
use crate::config;
use crate::mapper;
use crate::storage::Artifacts;

impl LogSource {
    fn other(self) -> Self {
        match self {
            Self::Deck => Self::Storage,
            Self::Storage => Self::Deck,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum LogSource {
    Deck,
    Storage,
}

#[derive(Serialize, Deserialize)]
pub(super) struct LogPosition {
    pub(super) source: LogSource,
    pub(super) offset: u64,
}

struct RunTarget {
    job: String,
    build_id: String,
    state: Option<String>,
    artifacts: Option<Artifacts>,
    status: PipelineStatus,
    web_url: Option<String>,
}

impl ProwPlugin {
    fn target(&self, run: &RunRef<'_>) -> PluginResult<RunTarget> {
        let selection = config::parse_pipeline_id(run.pipeline_id)?;
        let deck = self.deck()?;
        let text = |key: &str| {
            run.metadata
                .get(key)
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
        };
        let state = text("state").map(str::to_string);
        let artifacts = text("artifacts").and_then(Artifacts::from_metadata);
        let status = match (state.as_deref(), text("result")) {
            (Some(state), _) => mapper::map_state(state),
            (None, Some(result)) => mapper::map_result(result, None).0,
            (None, None) => PipelineStatus::Pending,
        };
        let build_id = Some(run.run_id)
            .filter(|id| !id.is_empty())
            .or_else(|| text("build_id"))
            .ok_or_else(|| {
                PluginError::PipelineNotFound("This run has no Prow build ID".to_string())
            })?
            .to_string();
        Ok(RunTarget {
            job: selection.job,
            build_id,
            state,
            web_url: artifacts
                .as_ref()
                .map(|artifacts| artifacts.view_url(deck.base())),
            artifacts,
            status,
        })
    }

    async fn stream_from(
        &self, source: LogSource, target: &RunTarget, offset: Option<u64>, sink: &mut LogSink,
    ) -> PluginResult<(LogReadMode, u64)> {
        let deck = self.deck()?;
        match (source, &target.artifacts) {
            (LogSource::Storage, Some(artifacts)) => {
                self.storage()?
                    .stream_build_log(artifacts, offset, sink)
                    .await
            }
            _ => {
                deck.stream_pod_log(&target.job, &target.build_id, offset, sink)
                    .await
            }
        }
    }

    pub(super) async fn fetch_log_nodes_impl(&self, run: RunRef<'_>) -> PluginResult<Vec<LogNode>> {
        let target = self.target(&run)?;
        let availability = match target.state.as_deref() {
            Some("scheduling" | "triggered") => LogAvailability::Pending,
            Some("pending") => LogAvailability::Partial,
            _ => LogAvailability::Ready,
        };
        Ok(vec![mapper::log_node(
            target.status,
            availability,
            target.web_url,
        )])
    }

    pub(super) async fn read_log_impl(
        &self, run: RunRef<'_>, node_id: &str, from: Option<&LogCursor>, sink: &mut LogSink,
    ) -> PluginResult<LogRead> {
        if node_id != mapper::LOG_NODE {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid Prow log node: {node_id}"
            )));
        }
        let target = self.target(&run)?;
        let state = target.state.as_deref();
        if matches!(state, Some("scheduling" | "triggered")) {
            return Err(PluginError::PipelineNotFound(format!(
                "Build {} has no log yet",
                target.build_id
            )));
        }
        let storage = self.storage()?;
        let stored = state != Some("pending")
            && target
                .artifacts
                .as_ref()
                .is_some_and(|artifacts| storage.log_url(artifacts).is_some());
        let previous = from.and_then(|cursor| cursor.parse::<LogPosition>());
        let source = match (stored, &previous) {
            (false, _) => LogSource::Deck,
            (true, Some(previous)) => previous.source,
            (true, None) => LogSource::Storage,
        };
        let offset = previous
            .filter(|previous| previous.source == source)
            .map(|previous| previous.offset);
        if from.is_some() && offset.is_none() {
            sink.restart()?;
        }

        let (source, (mode, offset)) = match self.stream_from(source, &target, offset, sink).await {
            Ok(read) => (source, read),
            Err(PluginError::PipelineNotFound(_)) if stored && sink.bytes_written() == 0 => {
                if from.is_some() && !sink.restarted() {
                    sink.restart()?;
                }
                let fallback = source.other();
                match self.stream_from(fallback, &target, None, sink).await {
                    Ok(read) => (fallback, read),
                    Err(PluginError::PipelineNotFound(_)) => {
                        return Err(PluginError::PipelineNotFound(format!(
                            "The log of build {} is no longer available",
                            target.build_id
                        )))
                    }
                    Err(error) => return Err(error),
                }
            }
            Err(error) => return Err(error),
        };

        Ok(LogRead {
            mode,
            cursor: Some(LogCursor::new(&LogPosition { source, offset })?),
        })
    }
}
