use super::CoreEvent;

impl CoreEvent {
    pub fn to_json_payload(&self) -> serde_json::Value {
        match self {
            CoreEvent::ProvidersChanged => serde_json::json!({}),
            CoreEvent::PipelineViewsChanged => serde_json::json!({}),
            CoreEvent::ApplicationsChanged => serde_json::json!({}),
            CoreEvent::PipelinesUpdated {
                pipelines,
                provider_id,
                timestamp,
            } => serde_json::json!({
                "pipelines": pipelines,
                "providerId": provider_id,
                "timestamp": timestamp,
            }),
            CoreEvent::PipelineStatusChanged { pipeline_ids } => serde_json::json!({
                "pipeline_ids": pipeline_ids,
            }),
            CoreEvent::PipelinesFailed { pipelines }
            | CoreEvent::PipelinesFinished { pipelines } => {
                serde_json::json!({
                    "pipelines": pipelines,
                })
            }
            CoreEvent::RunTriggered { workflow_id } => serde_json::json!(workflow_id),
            CoreEvent::RunCancelled { pipeline_id } => serde_json::json!(pipeline_id),
            CoreEvent::MetricsGenerated { pipeline_id } => serde_json::json!(pipeline_id),
            CoreEvent::MetricsGlobalConfigChanged => serde_json::json!({}),
            CoreEvent::MetricsConfigChanged { pipeline_id } => serde_json::json!(pipeline_id),
            CoreEvent::MetricsFlushed { pipeline_id } => {
                let mut json = serde_json::json!({});
                if let Some(pid) = pipeline_id {
                    json["pipeline_id"] = serde_json::json!(pid);
                }
                json
            }
            CoreEvent::ProviderStatusUpdated { provider } => serde_json::json!({
                "provider": provider,
            }),
            CoreEvent::ProviderHealthAlert {
                alert_id,
                notification_key,
                provider_ids,
                level,
                title,
                body,
                link,
                recovered,
                ..
            } => serde_json::json!({
                "alert_id": alert_id,
                "notification_key": notification_key,
                "provider_ids": provider_ids,
                "level": level,
                "title": title,
                "body": body,
                "link": link,
                "recovered": recovered,
            }),
            CoreEvent::MigrationProgress {
                step,
                step_index,
                total_steps,
                message,
            } => serde_json::json!({
                "step": step,
                "step_index": step_index,
                "total_steps": total_steps,
                "message": message,
            }),
            CoreEvent::MigrationComplete { success, summary } => serde_json::json!({
                "success": success,
                "summary": summary,
            }),
            CoreEvent::ProviderAdded {
                provider,
                timestamp,
            } => serde_json::json!({
                "provider": provider,
                "action": "added",
                "timestamp": timestamp,
            }),
            CoreEvent::ProviderUpdated {
                provider,
                timestamp,
            } => serde_json::json!({
                "provider": provider,
                "action": "updated",
                "timestamp": timestamp,
            }),
            CoreEvent::ProviderRemoved {
                provider,
                timestamp,
            } => serde_json::json!({
                "provider": provider,
                "action": "removed",
                "timestamp": timestamp,
            }),
            CoreEvent::RunHistoryCacheInvalidated {
                pipeline_id,
                run_number,
            } => {
                let mut json = serde_json::json!({});
                if let Some(pid) = pipeline_id {
                    json["pipeline_id"] = serde_json::json!(pid);
                }
                if let Some(number) = run_number {
                    json["run_number"] = serde_json::json!(number);
                }
                json
            }
            CoreEvent::VaultUnlocked => serde_json::json!({}),
            CoreEvent::VaultLocked => serde_json::json!({}),
            CoreEvent::DevicePaired { id, name } => serde_json::json!({
                "id": id,
                "name": name,
            }),
            CoreEvent::DeviceRevoked { id } => serde_json::json!({
                "id": id,
            }),
            CoreEvent::AlertRecorded { id, seq } => serde_json::json!({
                "id": id,
                "seq": seq,
            }),
            CoreEvent::NotificationObserved { notification } => {
                serde_json::to_value(notification).unwrap_or_default()
            }
            CoreEvent::RunLogProgress {
                pipeline_id,
                run_number,
                node_id,
                received_bytes,
            } => serde_json::json!({
                "pipelineId": pipeline_id,
                "runNumber": run_number,
                "nodeId": node_id,
                "receivedBytes": received_bytes,
            }),
            CoreEvent::RunLogReady {
                pipeline_id,
                run_number,
                node_id,
                revision,
            } => serde_json::json!({
                "pipelineId": pipeline_id,
                "runNumber": run_number,
                "nodeId": node_id,
                "revision": revision,
            }),
            CoreEvent::RunLogFailed {
                pipeline_id,
                run_number,
                node_id,
                message,
            } => serde_json::json!({
                "pipelineId": pipeline_id,
                "runNumber": run_number,
                "nodeId": node_id,
                "message": message,
            }),
            CoreEvent::RunLogTreeChanged {
                pipeline_id,
                run_number,
            } => serde_json::json!({
                "pipelineId": pipeline_id,
                "runNumber": run_number,
            }),
            CoreEvent::RunUpdated {
                pipeline_id,
                run_number,
                run,
            } => serde_json::json!({
                "pipelineId": pipeline_id,
                "runNumber": run_number,
                "run": run,
            }),
        }
    }
}
