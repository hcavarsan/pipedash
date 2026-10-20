ALTER TABLE run_history_cache ALTER COLUMN run_number TYPE BIGINT;
ALTER TABLE pipeline_metrics ALTER COLUMN run_number TYPE BIGINT;
ALTER TABLE metrics_processing_state ALTER COLUMN last_processed_run_number TYPE BIGINT;

DELETE FROM run_history_cache WHERE pipeline_id LIKE 'tekton__%';
DELETE FROM pipeline_metrics WHERE pipeline_id LIKE 'tekton__%';
DELETE FROM metrics_processing_state WHERE pipeline_id LIKE 'tekton__%';
DELETE FROM run_history_cache WHERE pipeline_id LIKE 'buildkite__%';
