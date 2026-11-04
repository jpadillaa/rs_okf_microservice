-- Registro durable PostgreSQL. Los archivos permanecen en OKF_DATA_DIR.
CREATE TABLE jobs (
    id TEXT PRIMARY KEY,
    filename TEXT NOT NULL,
    original TEXT NOT NULL,
    sha256 TEXT NOT NULL,
    status TEXT NOT NULL,
    attempt BIGINT NOT NULL DEFAULT 0 CHECK (attempt >= 0),
    lease_until BIGINT,
    bundle TEXT,
    error TEXT,
    created_at BIGINT NOT NULL,
    retry_at BIGINT,
    converter_version TEXT,
    converter_commit TEXT,
    profile_version TEXT,
    finished_at BIGINT,
    expired_at BIGINT,
    purged_at BIGINT
);
CREATE INDEX jobs_status ON jobs(status, created_at, id);
CREATE INDEX jobs_retention ON jobs(status, finished_at);
CREATE TABLE worker_heartbeats (id TEXT PRIMARY KEY, seen_at BIGINT NOT NULL);
CREATE TABLE conversion_metrics (
    format TEXT NOT NULL,
    result TEXT NOT NULL,
    count BIGINT NOT NULL,
    read_ms BIGINT NOT NULL,
    assemble_ms BIGINT NOT NULL,
    validate_ms BIGINT NOT NULL,
    total_ms BIGINT NOT NULL,
    peak_mb BIGINT NOT NULL,
    retries BIGINT NOT NULL,
    PRIMARY KEY(format, result)
);
