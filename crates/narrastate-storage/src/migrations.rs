use sqlx::{Executor, SqlitePool};

pub const MIGRATIONS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS _migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS cases (
        case_id TEXT PRIMARY KEY,
        schema_version TEXT NOT NULL,
        content_hash TEXT NOT NULL,
        source_path TEXT NOT NULL,
        loaded_at TEXT NOT NULL,
        definition_json TEXT NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS sessions (
        session_id TEXT PRIMARY KEY,
        case_id TEXT NOT NULL REFERENCES cases(case_id),
        status TEXT NOT NULL,
        revision INTEGER NOT NULL CHECK(revision >= 0),
        state_json TEXT NOT NULL,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS narrative_events (
        event_id TEXT PRIMARY KEY,
        session_id TEXT NOT NULL REFERENCES sessions(session_id) ON DELETE CASCADE,
        turn_id TEXT,
        sequence INTEGER NOT NULL CHECK(sequence >= 0),
        event_type TEXT NOT NULL,
        schema_version INTEGER NOT NULL,
        payload_json TEXT NOT NULL,
        created_at TEXT NOT NULL,
        UNIQUE(session_id, sequence)
    )",
    "CREATE TABLE IF NOT EXISTS session_snapshots (
        session_id TEXT NOT NULL REFERENCES sessions(session_id) ON DELETE CASCADE,
        revision INTEGER NOT NULL CHECK(revision >= 0),
        state_json TEXT NOT NULL,
        created_at TEXT NOT NULL,
        PRIMARY KEY (session_id, revision)
    )",
    "CREATE TABLE IF NOT EXISTS action_results (
        session_id TEXT NOT NULL REFERENCES sessions(session_id) ON DELETE CASCADE,
        client_action_id TEXT NOT NULL,
        revision INTEGER NOT NULL,
        response_json TEXT NOT NULL,
        created_at TEXT NOT NULL,
        PRIMARY KEY(session_id, client_action_id)
    )",
    "CREATE TABLE IF NOT EXISTS settings (
        setting_key TEXT PRIMARY KEY,
        value_json TEXT NOT NULL,
        updated_at TEXT NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS llm_calls (
        call_id TEXT PRIMARY KEY,
        session_id TEXT NOT NULL REFERENCES sessions(session_id) ON DELETE CASCADE,
        turn_id TEXT,
        purpose TEXT NOT NULL,
        provider TEXT NOT NULL,
        model TEXT NOT NULL,
        prompt_hash TEXT NOT NULL,
        latency_ms INTEGER NOT NULL,
        input_tokens INTEGER,
        output_tokens INTEGER,
        status TEXT NOT NULL,
        error_code TEXT,
        created_at TEXT NOT NULL
    )",
    "CREATE INDEX IF NOT EXISTS idx_narrative_events_session ON narrative_events(session_id, sequence)",
    "CREATE INDEX IF NOT EXISTS idx_sessions_case ON sessions(case_id)",
    "CREATE INDEX IF NOT EXISTS idx_snapshots_session ON session_snapshots(session_id, revision DESC)",
    "CREATE TABLE installed_cases (
        case_id TEXT NOT NULL,
        case_version TEXT NOT NULL,
        source_path TEXT NOT NULL,
        schema_version TEXT NOT NULL,
        template_content_hash TEXT NOT NULL,
        installed_at TEXT NOT NULL,
        PRIMARY KEY (case_id, case_version)
    )",
    "CREATE TABLE case_instances (
        instance_id TEXT PRIMARY KEY,
        case_id TEXT NOT NULL,
        case_version TEXT NOT NULL,
        variant_id TEXT NOT NULL,
        selector_version TEXT NOT NULL,
        seed_text TEXT NOT NULL,
        compiled_content_hash TEXT NOT NULL,
        instance_hash TEXT NOT NULL,
        compiled_json TEXT NOT NULL,
        created_at TEXT NOT NULL
    )",
    "ALTER TABLE sessions ADD COLUMN instance_id TEXT REFERENCES case_instances(instance_id)",
    "CREATE INDEX idx_sessions_instance ON sessions(instance_id)",
    "CREATE TABLE case_generation_jobs (
        job_id TEXT PRIMARY KEY,
        status TEXT NOT NULL,
        request_json TEXT NOT NULL,
        drafts_json TEXT NOT NULL,
        status_events_json TEXT NOT NULL,
        validation_report_json TEXT,
        result_path TEXT,
        attempt_count INTEGER NOT NULL DEFAULT 0 CHECK(attempt_count >= 0),
        repair_count INTEGER NOT NULL DEFAULT 0 CHECK(repair_count >= 0),
        error_code TEXT,
        error_message TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    )",
    "CREATE INDEX idx_case_generation_jobs_status ON case_generation_jobs(status, updated_at)",
];

pub async fn run_migrations(pool: &SqlitePool) -> Result<(), super::StorageError> {
    let mut connection = pool
        .acquire()
        .await
        .map_err(|error| super::StorageError::Database(format!("acquire: {error}")))?;
    connection
        .execute(MIGRATIONS[0])
        .await
        .map_err(|error| super::StorageError::Migration(format!("bootstrap: {error}")))?;
    for (index, sql) in MIGRATIONS.iter().enumerate() {
        let version = (index + 1) as i64;
        let applied: Option<i64> =
            sqlx::query_scalar("SELECT version FROM _migrations WHERE version = ?")
                .bind(version)
                .fetch_optional(&mut *connection)
                .await
                .map_err(|error| {
                    super::StorageError::Migration(format!("check V{version}: {error}"))
                })?;
        if applied.is_some() {
            continue;
        }
        connection
            .execute(*sql)
            .await
            .map_err(|error| super::StorageError::Migration(format!("V{version}: {error}")))?;
        sqlx::query(
            "INSERT OR IGNORE INTO _migrations (version, applied_at) VALUES (?, datetime('now'))",
        )
        .bind(version)
        .execute(&mut *connection)
        .await
        .map_err(|error| super::StorageError::Migration(format!("record V{version}: {error}")))?;
    }
    Ok(())
}
