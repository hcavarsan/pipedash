CREATE TABLE IF NOT EXISTS run_inputs (
    pipeline_id TEXT NOT NULL,
    run_number INTEGER NOT NULL,
    inputs_json TEXT NOT NULL,
    PRIMARY KEY (pipeline_id, run_number)
);
