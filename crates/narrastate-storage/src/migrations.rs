pub const MIGRATIONS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS _migrations (
        version INTEGER PRIMARY KEY,
        applied_at TEXT NOT NULL
    );",
    "CREATE TABLE IF NOT EXISTS cases (
        case_id TEXT PRIMARY KEY,
        schema_version TEXT NOT NULL,
        content_hash TEXT NOT NULL,
        source_path TEXT NOT NULL,
        loaded_at TEXT NOT NULL,
        definition_json TEXT NOT NULL
    );",
    "CREATE TABLE IF NOT EXISTS sessions (
        session_id TEXT PRIMARY KEY,
        case_id TEXT NOT NULL,
        status TEXT NOT NULL,
        revision INTEGER NOT NULL,
        state_json TEXT NOT NULL,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );",
    "CREATE TABLE IF NOT EXISTS narrative_events (
        event_id TEXT PRIMARY KEY,
        session_id TEXT NOT NULL,
        turn_id TEXT,
        sequence INTEGER NOT NULL,
        event_type TEXT NOT NULL,
        schema_version INTEGER NOT NULL,
        payload_json TEXT NOT NULL,
        created_at TEXT NOT NULL,
        UNIQUE(session_id, sequence)
    );",
    "CREATE TABLE IF NOT EXISTS session_snapshots (
        session_id TEXT NOT NULL,
        revision INTEGER NOT NULL,
        state_json TEXT NOT NULL,
        created_at TEXT NOT NULL,
        PRIMARY KEY (session_id, revision)
    );",
    "CREATE INDEX IF NOT EXISTS idx_narrative_events_session ON narrative_events(session_id);",
    "CREATE INDEX IF NOT EXISTS idx_sessions_case ON sessions(case_id);",
    "CREATE INDEX IF NOT EXISTS idx_snapshots_session ON session_snapshots(session_id);",
];

pub fn run_migrations(pool: &sqlx::SqlitePool) -> Result<(), super::StorageError> {
    use super::StorageError;
    use sqlx::Executor;

    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| StorageError::Internal(format!("tokio runtime: {e}")))?;

    rt.block_on(async move {
        let mut conn = pool.acquire().await
            .map_err(|e| StorageError::Database(format!("acquire: {e}")))?;

        for (i, sql) in MIGRATIONS.iter().enumerate() {
            let version = (i + 1) as i64;
            conn.execute(*sql).await
                .map_err(|e| StorageError::Migration(format!("V{version}: {e}")))?;
            sqlx::query("INSERT OR IGNORE INTO _migrations (version, applied_at) VALUES (?, datetime('now'))")
                .bind(version)
                .execute(&mut *conn)
                .await
                .map_err(|e| StorageError::Migration(format!("record V{version}: {e}")))?;
        }

        Ok(())
    })
}
