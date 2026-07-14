use narrastate_core::case::CaseDefinition;
use narrastate_core::id::{CaseId, SessionId, TurnId};
use narrastate_core::session::{NarrativeEvent, NarrativeEventKind, SessionState};
use narrastate_runtime::ports::{Repository, StorageError};

use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;

pub struct SqliteRepository {
    pool: SqlitePool,
    rt: tokio::runtime::Runtime,
}

impl SqliteRepository {
    pub fn new(database_url: &str) -> Result<Self, StorageError> {
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| StorageError::Internal(format!("runtime: {e}")))?;

        let pool = rt
            .block_on(SqlitePoolOptions::new().connect(database_url))
            .map_err(|e| StorageError::Database(format!("connect: {e}")))?;

        if let Err(e) = rt.block_on(sqlx::query("PRAGMA journal_mode=WAL").execute(&pool)) {
            return Err(StorageError::Database(format!("WAL: {e}")));
        }
        if let Err(e) = rt.block_on(sqlx::query("PRAGMA foreign_keys=ON").execute(&pool)) {
            return Err(StorageError::Database(format!("fk: {e}")));
        }

        let repo = Self { pool, rt };
        super::migrations::run_migrations(&repo.pool)?;
        Ok(repo)
    }

    pub fn new_in_memory() -> Result<Self, StorageError> {
        Self::new("file::memory:?cache=shared")
    }

    fn block<T>(
        &self,
        fut: impl std::future::Future<Output = Result<T, StorageError>>,
    ) -> Result<T, StorageError> {
        self.rt.block_on(fut)
    }
}

impl Repository for SqliteRepository {
    fn create_session(&self, session: &SessionState) -> Result<(), StorageError> {
        self.block(async {
            let state_json = serde_json::to_string(session)
                .map_err(|e| StorageError::Serialization(e.to_string()))?;
            let now = chrono::Utc::now().to_rfc3339();

            sqlx::query(
                "INSERT INTO sessions (session_id, case_id, status, revision, state_json, created_at, updated_at) \
                 VALUES (?, ?, ?, 0, ?, ?, ?)",
            )
            .bind(session.session_id.0.to_string())
            .bind(session.case_id.as_ref())
            .bind(format!("{:?}", session.status))
            .bind(&state_json)
            .bind(&now)
            .bind(&now)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                if e.to_string().contains("UNIQUE") {
                    StorageError::Constraint(format!("session {} already exists", session.session_id))
                } else {
                    StorageError::Database(e.to_string())
                }
            })?;

            Ok(())
        })
    }

    fn load_session(&self, session_id: &SessionId) -> Result<SessionState, StorageError> {
        self.block(async {
            let state_json: Option<String> =
                sqlx::query_scalar("SELECT state_json FROM sessions WHERE session_id = ?")
                    .bind(session_id.0.to_string())
                    .fetch_optional(&self.pool)
                    .await
                    .map_err(|e| StorageError::Database(e.to_string()))?;

            match state_json {
                Some(json) => serde_json::from_str(&json)
                    .map_err(|e| StorageError::Serialization(e.to_string())),
                None => Err(StorageError::NotFound(format!("session {session_id}"))),
            }
        })
    }

    fn update_session(&self, session: &SessionState) -> Result<(), StorageError> {
        self.block(async {
            let state_json = serde_json::to_string(session)
                .map_err(|e| StorageError::Serialization(e.to_string()))?;
            let now = chrono::Utc::now().to_rfc3339();

            let result = sqlx::query(
                "UPDATE sessions SET status = ?, revision = ?, state_json = ?, updated_at = ? \
                 WHERE session_id = ? AND revision = ?",
            )
            .bind(format!("{:?}", session.status))
            .bind(session.revision as i64)
            .bind(&state_json)
            .bind(&now)
            .bind(session.session_id.0.to_string())
            .bind(session.revision as i64 - 1)
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;

            if result.rows_affected() == 0 {
                let current_rev: Option<i64> =
                    sqlx::query_scalar("SELECT revision FROM sessions WHERE session_id = ?")
                        .bind(session.session_id.0.to_string())
                        .fetch_optional(&self.pool)
                        .await
                        .map_err(|e| StorageError::Database(e.to_string()))?
                        .flatten();

                match current_rev {
                    Some(rev) => Err(StorageError::RevisionConflict {
                        expected: session.revision,
                        actual: rev as u64,
                    }),
                    None => Err(StorageError::NotFound(format!(
                        "session {}",
                        session.session_id
                    ))),
                }
            } else {
                Ok(())
            }
        })
    }

    fn save_case(&self, case: &CaseDefinition) -> Result<(), StorageError> {
        self.block(async {
            let definition_json = serde_json::to_string(case)
                .map_err(|e| StorageError::Serialization(e.to_string()))?;
            let now = chrono::Utc::now().to_rfc3339();
            let content_hash = {
                use std::hash::{Hash, Hasher};
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                definition_json.hash(&mut hasher);
                format!("{:x}", hasher.finish())
            };

            sqlx::query(
                "INSERT OR REPLACE INTO cases \
                 (case_id, schema_version, content_hash, source_path, loaded_at, definition_json) \
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(case.id.as_ref())
            .bind("0.1.0")
            .bind(&content_hash)
            .bind("")
            .bind(&now)
            .bind(&definition_json)
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;

            Ok(())
        })
    }

    fn load_case(&self, case_id: &CaseId) -> Result<CaseDefinition, StorageError> {
        self.block(async {
            let definition_json: Option<String> =
                sqlx::query_scalar("SELECT definition_json FROM cases WHERE case_id = ?")
                    .bind(case_id.as_ref())
                    .fetch_optional(&self.pool)
                    .await
                    .map_err(|e| StorageError::Database(e.to_string()))?;

            match definition_json {
                Some(json) => serde_json::from_str(&json)
                    .map_err(|e| StorageError::Serialization(e.to_string())),
                None => Err(StorageError::NotFound(format!("case {case_id}"))),
            }
        })
    }

    fn list_cases(&self) -> Result<Vec<CaseDefinition>, StorageError> {
        self.block(async {
            let rows: Vec<String> =
                sqlx::query_scalar("SELECT definition_json FROM cases ORDER BY loaded_at DESC")
                    .fetch_all(&self.pool)
                    .await
                    .map_err(|e| StorageError::Database(e.to_string()))?;

            rows.iter()
                .map(|json| {
                    serde_json::from_str(json)
                        .map_err(|e| StorageError::Serialization(e.to_string()))
                })
                .collect()
        })
    }

    fn append_events(
        &self,
        _session_id: &SessionId,
        events: &[NarrativeEvent],
    ) -> Result<(), StorageError> {
        self.block(async {
            let mut tx = self
                .pool
                .begin()
                .await
                .map_err(|e| StorageError::Database(e.to_string()))?;

            for event in events {
                let now = chrono::Utc::now().to_rfc3339();
                let payload_json = serde_json::to_string(&event.payload)
                    .map_err(|e| StorageError::Serialization(e.to_string()))?;
                let event_type_json = serde_json::to_string(&event.event_type)
                    .map_err(|e| StorageError::Serialization(e.to_string()))?;

                sqlx::query(
                    "INSERT INTO narrative_events \
                     (event_id, session_id, turn_id, sequence, event_type, schema_version, payload_json, created_at) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(event.event_id.to_string())
                .bind(event.session_id.0.to_string())
                .bind(event.turn_id.as_ref().map(|t| t.0.to_string()))
                .bind(event.sequence as i64)
                .bind(&event_type_json)
                .bind(event.schema_version as i64)
                .bind(&payload_json)
                .bind(&now)
                .execute(&mut *tx)
                .await
                .map_err(|e| StorageError::Database(e.to_string()))?;
            }

            tx.commit()
                .await
                .map_err(|e| StorageError::Database(e.to_string()))?;

            Ok(())
        })
    }

    fn load_events(&self, session_id: &SessionId) -> Result<Vec<NarrativeEvent>, StorageError> {
        self.block(async {
            #[derive(sqlx::FromRow)]
            struct EventRow {
                event_id: String,
                session_id: String,
                turn_id: Option<String>,
                sequence: i64,
                event_type: String,
                schema_version: i64,
                payload_json: String,
            }

            let rows: Vec<EventRow> = sqlx::query_as(
                "SELECT event_id, session_id, turn_id, sequence, event_type, schema_version, payload_json \
                 FROM narrative_events WHERE session_id = ? ORDER BY sequence",
            )
            .bind(session_id.0.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;

            let mut events = Vec::with_capacity(rows.len());
            for r in rows {
                let event_id = uuid::Uuid::parse_str(&r.event_id)
                    .map_err(|e| StorageError::Serialization(e.to_string()))?;
                let event_type: NarrativeEventKind = serde_json::from_str(&r.event_type)
                    .map_err(|e| StorageError::Serialization(e.to_string()))?;
                let session_id = SessionId(uuid::Uuid::parse_str(&r.session_id)
                    .map_err(|e| StorageError::Serialization(e.to_string()))?);
                let turn_id = match r.turn_id {
                    Some(t) => Some(TurnId(uuid::Uuid::parse_str(&t)
                        .map_err(|e| StorageError::Serialization(e.to_string()))?)),
                    None => None,
                };
                events.push(NarrativeEvent {
                    event_id,
                    session_id,
                    turn_id,
                    sequence: r.sequence as u64,
                    event_type,
                    schema_version: r.schema_version as u32,
                    payload: serde_json::from_str(&r.payload_json)
                        .map_err(|e| StorageError::Serialization(e.to_string()))?,
                });
            }
            Ok(events)
        })
    }

    fn save_snapshot(
        &self,
        session_id: &SessionId,
        revision: u64,
        state: &SessionState,
    ) -> Result<(), StorageError> {
        self.block(async {
            let state_json = serde_json::to_string(state)
                .map_err(|e| StorageError::Serialization(e.to_string()))?;
            let now = chrono::Utc::now().to_rfc3339();

            sqlx::query(
                "INSERT OR REPLACE INTO session_snapshots (session_id, revision, state_json, created_at) \
                 VALUES (?, ?, ?, ?)",
            )
            .bind(session_id.0.to_string())
            .bind(revision as i64)
            .bind(&state_json)
            .bind(&now)
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;

            Ok(())
        })
    }

    fn load_latest_snapshot(
        &self,
        session_id: &SessionId,
    ) -> Result<Option<(u64, SessionState)>, StorageError> {
        self.block(async {
            #[derive(sqlx::FromRow)]
            struct SnapshotRow {
                revision: i64,
                state_json: String,
            }

            let row: Option<SnapshotRow> = sqlx::query_as(
                "SELECT revision, state_json FROM session_snapshots \
                 WHERE session_id = ? ORDER BY revision DESC LIMIT 1",
            )
            .bind(session_id.0.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| StorageError::Database(e.to_string()))?;

            match row {
                Some(r) => {
                    let state: SessionState = serde_json::from_str(&r.state_json)
                        .map_err(|e| StorageError::Serialization(e.to_string()))?;
                    Ok(Some((r.revision as u64, state)))
                }
                None => Ok(None),
            }
        })
    }
}
