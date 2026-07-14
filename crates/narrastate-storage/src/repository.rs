use async_trait::async_trait;
use narrastate_core::case::CaseDefinition;
use narrastate_core::id::{CaseId, ClientActionId, SessionId, TurnId};
use narrastate_core::session::{
    NarrativeEvent, NarrativeEventKind, NarrativeEventPayload, SessionState, SessionStatus,
};
use narrastate_runtime::ports::{
    CommitOutcome, LlmCallMetadata, ProviderSettings, Repository, StorageError,
};
use sha2::{Digest, Sha256};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Sqlite, SqlitePool, Transaction};
use std::str::FromStr;

pub struct SqliteRepository {
    pool: SqlitePool,
}

impl SqliteRepository {
    pub async fn new(database: &str) -> Result<Self, StorageError> {
        let is_memory = database.contains(":memory:") || database.contains("mode=memory");
        let options = if database.starts_with("sqlite:") || database.starts_with("file:") {
            SqliteConnectOptions::from_str(database)
                .map_err(|error| StorageError::Database(format!("invalid SQLite URL: {error}")))?
                .create_if_missing(!is_memory)
        } else {
            SqliteConnectOptions::new()
                .filename(database)
                .create_if_missing(true)
        }
        .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(if is_memory { 1 } else { 5 })
            .connect_with(options)
            .await
            .map_err(|error| StorageError::Database(format!("connect: {error}")))?;
        if !is_memory {
            sqlx::query("PRAGMA journal_mode=WAL")
                .execute(&pool)
                .await
                .map_err(|error| StorageError::Database(format!("WAL: {error}")))?;
        }
        super::migrations::run_migrations(&pool).await?;
        Ok(Self { pool })
    }

    pub async fn new_in_memory() -> Result<Self, StorageError> {
        Self::new("sqlite::memory:").await
    }

    async fn insert_events(
        transaction: &mut Transaction<'_, Sqlite>,
        session_id: &SessionId,
        events: &[NarrativeEvent],
    ) -> Result<(), StorageError> {
        for event in events {
            if &event.session_id != session_id {
                return Err(StorageError::Constraint(format!(
                    "event {} belongs to session {}, expected {}",
                    event.event_id, event.session_id, session_id
                )));
            }
            let payload = serde_json::to_string(&event.payload)
                .map_err(|error| StorageError::Serialization(error.to_string()))?;
            let event_type = serde_json::to_string(&event.event_type)
                .map_err(|error| StorageError::Serialization(error.to_string()))?;
            sqlx::query(
                "INSERT INTO narrative_events
                 (event_id, session_id, turn_id, sequence, event_type, schema_version, payload_json, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(event.event_id.to_string())
            .bind(session_id.to_string())
            .bind(event.turn_id.map(|id| id.to_string()))
            .bind(event.sequence as i64)
            .bind(event_type)
            .bind(event.schema_version as i64)
            .bind(payload)
            .bind(chrono::Utc::now().to_rfc3339())
            .execute(&mut **transaction)
            .await
            .map_err(map_database)?;
        }
        Ok(())
    }

    async fn current_revision(
        transaction: &mut Transaction<'_, Sqlite>,
        session_id: &SessionId,
    ) -> Result<u64, StorageError> {
        let revision: Option<i64> =
            sqlx::query_scalar("SELECT revision FROM sessions WHERE session_id = ?")
                .bind(session_id.to_string())
                .fetch_optional(&mut **transaction)
                .await
                .map_err(map_database)?;
        revision
            .map(|value| value as u64)
            .ok_or_else(|| StorageError::NotFound(format!("session {session_id}")))
    }

    async fn update_session_in_transaction(
        transaction: &mut Transaction<'_, Sqlite>,
        expected_revision: u64,
        session: &SessionState,
    ) -> Result<(), StorageError> {
        if session.revision != expected_revision.saturating_add(1) {
            return Err(StorageError::Constraint(format!(
                "new revision {} must equal expected revision {} + 1",
                session.revision, expected_revision
            )));
        }
        let actual = Self::current_revision(transaction, &session.session_id).await?;
        if actual != expected_revision {
            return Err(StorageError::RevisionConflict {
                expected: expected_revision,
                actual,
            });
        }
        let state = serde_json::to_string(session)
            .map_err(|error| StorageError::Serialization(error.to_string()))?;
        let result = sqlx::query(
            "UPDATE sessions SET status = ?, revision = ?, state_json = ?, updated_at = ?
             WHERE session_id = ? AND revision = ?",
        )
        .bind(format!("{:?}", session.status))
        .bind(session.revision as i64)
        .bind(state)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(session.session_id.to_string())
        .bind(expected_revision as i64)
        .execute(&mut **transaction)
        .await
        .map_err(map_database)?;
        if result.rows_affected() != 1 {
            return Err(StorageError::RevisionConflict {
                expected: expected_revision,
                actual: Self::current_revision(transaction, &session.session_id).await?,
            });
        }
        Ok(())
    }

    async fn maybe_snapshot(
        transaction: &mut Transaction<'_, Sqlite>,
        session: &SessionState,
    ) -> Result<(), StorageError> {
        if !session.revision.is_multiple_of(10) && session.status != SessionStatus::Resolved {
            return Ok(());
        }
        let state = serde_json::to_string(session)
            .map_err(|error| StorageError::Serialization(error.to_string()))?;
        sqlx::query(
            "INSERT OR REPLACE INTO session_snapshots
             (session_id, revision, state_json, created_at) VALUES (?, ?, ?, ?)",
        )
        .bind(session.session_id.to_string())
        .bind(session.revision as i64)
        .bind(state)
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&mut **transaction)
        .await
        .map_err(map_database)?;
        Ok(())
    }
}

#[async_trait]
impl Repository for SqliteRepository {
    async fn create_session(
        &self,
        session: &SessionState,
        events: &[NarrativeEvent],
    ) -> Result<(), StorageError> {
        if session.revision != 0 {
            return Err(StorageError::Constraint(
                "new session revision must be zero".into(),
            ));
        }
        let mut transaction = self.pool.begin().await.map_err(map_database)?;
        let state = serde_json::to_string(session)
            .map_err(|error| StorageError::Serialization(error.to_string()))?;
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO sessions
             (session_id, case_id, status, revision, state_json, created_at, updated_at)
             VALUES (?, ?, ?, 0, ?, ?, ?)",
        )
        .bind(session.session_id.to_string())
        .bind(session.case_id.as_ref())
        .bind(format!("{:?}", session.status))
        .bind(state)
        .bind(&now)
        .bind(&now)
        .execute(&mut *transaction)
        .await
        .map_err(map_database)?;
        Self::insert_events(&mut transaction, &session.session_id, events).await?;
        transaction.commit().await.map_err(map_database)
    }

    async fn load_session(&self, session_id: &SessionId) -> Result<SessionState, StorageError> {
        let state: Option<String> =
            sqlx::query_scalar("SELECT state_json FROM sessions WHERE session_id = ?")
                .bind(session_id.to_string())
                .fetch_optional(&self.pool)
                .await
                .map_err(map_database)?;
        deserialize_optional(state, format!("session {session_id}"))
    }

    async fn recover_session(&self, session_id: &SessionId) -> Result<SessionState, StorageError> {
        let persisted = self.load_session(session_id).await?;
        let mut recovered = self
            .load_latest_snapshot(session_id)
            .await?
            .map(|(_, state)| state);
        for event in self.load_events(session_id).await? {
            let candidate = match event.payload {
                NarrativeEventPayload::SessionCreated { state }
                | NarrativeEventPayload::TurnCommitted { state, .. }
                | NarrativeEventPayload::AccusationSubmitted { state }
                | NarrativeEventPayload::CaseResolved { state } => Some(*state),
                _ => None,
            };
            if let Some(state) = candidate {
                if recovered
                    .as_ref()
                    .is_none_or(|current| state.revision > current.revision)
                {
                    recovered = Some(state);
                }
            }
        }
        let recovered = recovered.ok_or_else(|| {
            StorageError::Internal(format!(
                "session {session_id} has no replayable creation event"
            ))
        })?;
        if recovered.revision != persisted.revision {
            return Err(StorageError::Internal(format!(
                "session {session_id} replay revision {} differs from persisted revision {}",
                recovered.revision, persisted.revision
            )));
        }
        Ok(recovered)
    }

    async fn commit_turn(
        &self,
        expected_revision: u64,
        client_action_id: &ClientActionId,
        session: &SessionState,
        events: &[NarrativeEvent],
        response: &serde_json::Value,
    ) -> Result<CommitOutcome, StorageError> {
        let mut transaction = self.pool.begin().await.map_err(map_database)?;
        let existing: Option<String> = sqlx::query_scalar(
            "SELECT response_json FROM action_results WHERE session_id = ? AND client_action_id = ?",
        )
        .bind(session.session_id.to_string())
        .bind(client_action_id.to_string())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(map_database)?;
        if let Some(json) = existing {
            let response = serde_json::from_str(&json)
                .map_err(|error| StorageError::Serialization(error.to_string()))?;
            transaction.rollback().await.map_err(map_database)?;
            return Ok(CommitOutcome::Idempotent(response));
        }
        Self::update_session_in_transaction(&mut transaction, expected_revision, session).await?;
        Self::insert_events(&mut transaction, &session.session_id, events).await?;
        let response_json = serde_json::to_string(response)
            .map_err(|error| StorageError::Serialization(error.to_string()))?;
        sqlx::query(
            "INSERT INTO action_results
             (session_id, client_action_id, revision, response_json, created_at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(session.session_id.to_string())
        .bind(client_action_id.to_string())
        .bind(session.revision as i64)
        .bind(response_json)
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&mut *transaction)
        .await
        .map_err(map_database)?;
        Self::maybe_snapshot(&mut transaction, session).await?;
        transaction.commit().await.map_err(map_database)?;
        Ok(CommitOutcome::Committed)
    }

    async fn commit_session(
        &self,
        expected_revision: u64,
        session: &SessionState,
        events: &[NarrativeEvent],
    ) -> Result<(), StorageError> {
        let mut transaction = self.pool.begin().await.map_err(map_database)?;
        Self::update_session_in_transaction(&mut transaction, expected_revision, session).await?;
        Self::insert_events(&mut transaction, &session.session_id, events).await?;
        Self::maybe_snapshot(&mut transaction, session).await?;
        transaction.commit().await.map_err(map_database)
    }

    async fn load_action_result(
        &self,
        session_id: &SessionId,
        client_action_id: &ClientActionId,
    ) -> Result<Option<serde_json::Value>, StorageError> {
        let value: Option<String> = sqlx::query_scalar(
            "SELECT response_json FROM action_results WHERE session_id = ? AND client_action_id = ?",
        )
        .bind(session_id.to_string())
        .bind(client_action_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(map_database)?;
        value
            .map(|json| {
                serde_json::from_str(&json)
                    .map_err(|error| StorageError::Serialization(error.to_string()))
            })
            .transpose()
    }

    async fn save_case(&self, case: &CaseDefinition) -> Result<(), StorageError> {
        case.validate().map_err(|errors| {
            StorageError::Constraint(
                errors
                    .into_iter()
                    .map(|error| error.to_string())
                    .collect::<Vec<_>>()
                    .join("; "),
            )
        })?;
        let definition = serde_json::to_string(case)
            .map_err(|error| StorageError::Serialization(error.to_string()))?;
        let hash = format!("{:x}", Sha256::digest(definition.as_bytes()));
        sqlx::query(
            "INSERT OR REPLACE INTO cases
             (case_id, schema_version, content_hash, source_path, loaded_at, definition_json)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(case.id.as_ref())
        .bind(&case.schema_version)
        .bind(hash)
        .bind("")
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(definition)
        .execute(&self.pool)
        .await
        .map_err(map_database)?;
        Ok(())
    }

    async fn load_case(&self, case_id: &CaseId) -> Result<CaseDefinition, StorageError> {
        let definition: Option<String> =
            sqlx::query_scalar("SELECT definition_json FROM cases WHERE case_id = ?")
                .bind(case_id.as_ref())
                .fetch_optional(&self.pool)
                .await
                .map_err(map_database)?;
        deserialize_optional(definition, format!("case {case_id}"))
    }

    async fn list_cases(&self) -> Result<Vec<CaseDefinition>, StorageError> {
        let values: Vec<String> =
            sqlx::query_scalar("SELECT definition_json FROM cases ORDER BY loaded_at DESC")
                .fetch_all(&self.pool)
                .await
                .map_err(map_database)?;
        values
            .into_iter()
            .map(|json| {
                serde_json::from_str(&json)
                    .map_err(|error| StorageError::Serialization(error.to_string()))
            })
            .collect()
    }

    async fn append_events(
        &self,
        session_id: &SessionId,
        events: &[NarrativeEvent],
    ) -> Result<(), StorageError> {
        let mut transaction = self.pool.begin().await.map_err(map_database)?;
        Self::insert_events(&mut transaction, session_id, events).await?;
        transaction.commit().await.map_err(map_database)
    }

    async fn load_events(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<NarrativeEvent>, StorageError> {
        #[derive(sqlx::FromRow)]
        struct Row {
            event_id: String,
            session_id: String,
            turn_id: Option<String>,
            sequence: i64,
            event_type: String,
            schema_version: i64,
            payload_json: String,
        }
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT event_id, session_id, turn_id, sequence, event_type, schema_version, payload_json
             FROM narrative_events WHERE session_id = ? ORDER BY sequence",
        )
        .bind(session_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(map_database)?;
        rows.into_iter()
            .map(|row| {
                Ok(NarrativeEvent {
                    event_id: uuid::Uuid::parse_str(&row.event_id)
                        .map_err(|error| StorageError::Serialization(error.to_string()))?,
                    session_id: SessionId(
                        uuid::Uuid::parse_str(&row.session_id)
                            .map_err(|error| StorageError::Serialization(error.to_string()))?,
                    ),
                    turn_id: row
                        .turn_id
                        .map(|value| {
                            uuid::Uuid::parse_str(&value)
                                .map(TurnId)
                                .map_err(|error| StorageError::Serialization(error.to_string()))
                        })
                        .transpose()?,
                    sequence: row.sequence as u64,
                    event_type: serde_json::from_str::<NarrativeEventKind>(&row.event_type)
                        .map_err(|error| StorageError::Serialization(error.to_string()))?,
                    schema_version: row.schema_version as u32,
                    payload: serde_json::from_str(&row.payload_json)
                        .map_err(|error| StorageError::Serialization(error.to_string()))?,
                })
            })
            .collect()
    }

    async fn save_snapshot(
        &self,
        session_id: &SessionId,
        revision: u64,
        state: &SessionState,
    ) -> Result<(), StorageError> {
        if state.session_id != *session_id || state.revision != revision {
            return Err(StorageError::Constraint(
                "snapshot identity/revision mismatch".into(),
            ));
        }
        let json = serde_json::to_string(state)
            .map_err(|error| StorageError::Serialization(error.to_string()))?;
        sqlx::query("INSERT OR REPLACE INTO session_snapshots (session_id, revision, state_json, created_at) VALUES (?, ?, ?, ?)")
            .bind(session_id.to_string()).bind(revision as i64).bind(json).bind(chrono::Utc::now().to_rfc3339())
            .execute(&self.pool).await.map_err(map_database)?;
        Ok(())
    }

    async fn load_latest_snapshot(
        &self,
        session_id: &SessionId,
    ) -> Result<Option<(u64, SessionState)>, StorageError> {
        let row: Option<(i64, String)> = sqlx::query_as("SELECT revision, state_json FROM session_snapshots WHERE session_id = ? ORDER BY revision DESC LIMIT 1")
            .bind(session_id.to_string()).fetch_optional(&self.pool).await.map_err(map_database)?;
        row.map(|(revision, json)| {
            serde_json::from_str(&json)
                .map(|state| (revision as u64, state))
                .map_err(|error| StorageError::Serialization(error.to_string()))
        })
        .transpose()
    }

    async fn save_provider_settings(
        &self,
        settings: &ProviderSettings,
    ) -> Result<(), StorageError> {
        let json = serde_json::to_string(settings)
            .map_err(|error| StorageError::Serialization(error.to_string()))?;
        sqlx::query("INSERT OR REPLACE INTO settings (setting_key, value_json, updated_at) VALUES ('provider', ?, ?)")
            .bind(json).bind(chrono::Utc::now().to_rfc3339()).execute(&self.pool).await.map_err(map_database)?;
        Ok(())
    }

    async fn load_provider_settings(&self) -> Result<Option<ProviderSettings>, StorageError> {
        let value: Option<String> =
            sqlx::query_scalar("SELECT value_json FROM settings WHERE setting_key = 'provider'")
                .fetch_optional(&self.pool)
                .await
                .map_err(map_database)?;
        value
            .map(|json| {
                serde_json::from_str(&json)
                    .map_err(|error| StorageError::Serialization(error.to_string()))
            })
            .transpose()
    }

    async fn record_llm_call(&self, call: &LlmCallMetadata) -> Result<(), StorageError> {
        sqlx::query("INSERT INTO llm_calls (call_id, session_id, turn_id, purpose, provider, model, prompt_hash, latency_ms, input_tokens, output_tokens, status, error_code, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(&call.call_id).bind(call.session_id.to_string()).bind(&call.turn_id).bind(&call.purpose).bind(&call.provider).bind(&call.model).bind(&call.prompt_hash).bind(call.latency_ms as i64).bind(call.input_tokens.map(|v| v as i64)).bind(call.output_tokens.map(|v| v as i64)).bind(&call.status).bind(&call.error_code).bind(chrono::Utc::now().to_rfc3339())
            .execute(&self.pool).await.map_err(map_database)?;
        Ok(())
    }

    async fn load_llm_calls(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<LlmCallMetadata>, StorageError> {
        type LlmRow = (
            String,
            Option<String>,
            String,
            String,
            String,
            String,
            i64,
            Option<i64>,
            Option<i64>,
            String,
            Option<String>,
        );
        let rows: Vec<LlmRow> = sqlx::query_as(
            "SELECT call_id, turn_id, purpose, provider, model, prompt_hash, latency_ms,
                    input_tokens, output_tokens, status, error_code
             FROM llm_calls WHERE session_id = ? ORDER BY created_at, call_id",
        )
        .bind(session_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(map_database)?;
        Ok(rows
            .into_iter()
            .map(
                |(
                    call_id,
                    turn_id,
                    purpose,
                    provider,
                    model,
                    prompt_hash,
                    latency_ms,
                    input_tokens,
                    output_tokens,
                    status,
                    error_code,
                )| LlmCallMetadata {
                    call_id,
                    session_id: *session_id,
                    turn_id,
                    purpose,
                    provider,
                    model,
                    prompt_hash,
                    latency_ms: latency_ms as u64,
                    input_tokens: input_tokens.map(|value| value as u64),
                    output_tokens: output_tokens.map(|value| value as u64),
                    status,
                    error_code,
                },
            )
            .collect())
    }
}

fn map_database(error: sqlx::Error) -> StorageError {
    if let sqlx::Error::Database(database) = &error {
        if database.is_unique_violation()
            || database.is_foreign_key_violation()
            || database.is_check_violation()
        {
            return StorageError::Constraint(database.message().to_string());
        }
    }
    StorageError::Database(error.to_string())
}

fn deserialize_optional<T: serde::de::DeserializeOwned>(
    value: Option<String>,
    label: String,
) -> Result<T, StorageError> {
    match value {
        Some(json) => serde_json::from_str(&json)
            .map_err(|error| StorageError::Serialization(error.to_string())),
        None => Err(StorageError::NotFound(label)),
    }
}
