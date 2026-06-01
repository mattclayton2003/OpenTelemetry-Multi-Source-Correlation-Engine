CREATE TABLE eval_runs (
    eval_run_id        TEXT PRIMARY KEY,
    tag                TEXT NOT NULL,
    started_at         INTEGER NOT NULL,
    ended_at           INTEGER NOT NULL,
    config_hash        TEXT NOT NULL,
    engine_version     TEXT NOT NULL,
    runner_version     TEXT NOT NULL,
    scoring_toml_hash  TEXT NOT NULL
);

CREATE TABLE eval_results (
    eval_run_id        TEXT NOT NULL REFERENCES eval_runs(eval_run_id),
    experiment_id      TEXT NOT NULL,
    invocation_mode    TEXT NOT NULL,
    incident_id        TEXT NOT NULL,
    recall_at_1        REAL NOT NULL,
    recall_at_3        REAL NOT NULL,
    recall_at_5        REAL NOT NULL,
    precision_at_1     REAL NOT NULL,
    precision_at_3     REAL NOT NULL,
    precision_at_5     REAL NOT NULL,
    trace_coverage     REAL NOT NULL,
    error_log_coverage REAL NOT NULL,
    anomaly_coverage   REAL NOT NULL,
    tree_integrity     REAL NOT NULL,
    elapsed_ms         INTEGER NOT NULL,
    clean_fps          INTEGER NOT NULL,
    composite          REAL NOT NULL,
    notes              TEXT,
    PRIMARY KEY (eval_run_id, experiment_id, invocation_mode)
);

CREATE INDEX idx_eval_results_composite ON eval_results(composite);
CREATE INDEX idx_eval_results_mode      ON eval_results(eval_run_id, invocation_mode);
