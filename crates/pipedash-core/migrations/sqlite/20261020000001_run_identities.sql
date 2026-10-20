DELETE FROM run_history_cache WHERE pipeline_id LIKE 'tekton__%';
DELETE FROM pipeline_metrics WHERE pipeline_id LIKE 'tekton__%';
DELETE FROM metrics_processing_state WHERE pipeline_id LIKE 'tekton__%';
DELETE FROM run_history_cache WHERE pipeline_id LIKE 'buildkite__%';
