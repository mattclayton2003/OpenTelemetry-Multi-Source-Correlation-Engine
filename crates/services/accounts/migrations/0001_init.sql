CREATE TABLE IF NOT EXISTS accounts (
    id         TEXT PRIMARY KEY,
    owner      TEXT NOT NULL,
    balance    BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
