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
];

pub async fn run_migrations(pool: &SqlitePool) -> Result<(), super::StorageError> {
    let mut connection = pool
        .acquire()
        .await
        .map_err(|error| super::StorageError::Database(format!("acquire: {error}")))?;
    for (index, sql) in MIGRATIONS.iter().enumerate() {
        let version = (index + 1) as i64;
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
