CREATE TABLE experiments (
    id                       TEXT PRIMARY KEY,
    yaml_path                TEXT NOT NULL,
    yaml_sha256              TEXT NOT NULL,
    started_at               INTEGER NOT NULL,
    ended_at                 INTEGER NOT NULL,
    primary_faulted_service  TEXT NOT NULL,
    failure_class            TEXT NOT NULL,
    blast_radius             TEXT NOT NULL,
    clean_services           TEXT NOT NULL,
    runner_version           TEXT NOT NULL,
    status                   TEXT NOT NULL,
    notes                    TEXT
);

CREATE TABLE fault_events (
    experiment_id   TEXT NOT NULL REFERENCES experiments(id),
    sequence_no     INTEGER NOT NULL,
    kind            TEXT NOT NULL,
    target          TEXT NOT NULL,
    started_at      INTEGER NOT NULL,
    ended_at        INTEGER NOT NULL,
    config_json     TEXT NOT NULL,
    PRIMARY KEY (experiment_id, sequence_no)
);

CREATE TABLE recovery_signals (
    experiment_id   TEXT NOT NULL REFERENCES experiments(id),
    signal          TEXT NOT NULL,
    cleared_at      INTEGER NOT NULL,
    PRIMARY KEY (experiment_id, signal)
);
