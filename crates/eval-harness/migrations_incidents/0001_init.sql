CREATE TABLE IF NOT EXISTS incidents (
    incident_id      TEXT PRIMARY KEY,
    schema_version   TEXT NOT NULL,
    engine_version   TEXT NOT NULL,
    config_hash      TEXT NOT NULL,
    trigger_kind     TEXT NOT NULL,
    trigger_input    TEXT NOT NULL,
    window_start     INTEGER NOT NULL,
    window_end       INTEGER NOT NULL,
    elapsed_ms       INTEGER NOT NULL,
    produced_at      INTEGER NOT NULL,
    document         TEXT NOT NULL,
    experiment_id    TEXT
);
