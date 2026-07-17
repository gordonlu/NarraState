use async_trait::async_trait;
use narrastate_case::{
    adapt_v01, compile, freeze_case, verify_compiled_hash, verify_instance_hash,
};
use narrastate_core::case::CaseDefinition;
use narrastate_core::id::{
    CaseId, CaseInstanceId, ClientActionId, ContentHash, GenerationJobId, SessionId, TurnId,
    VariantId,
};
use narrastate_core::session::{
    NarrativeEvent, NarrativeEventKind, NarrativeEventPayload, SessionState, SessionStatus,
};
use narrastate_core::{CaseInstance, CompiledCase, GenerationStatus, Seed, VariantSelectorVersion};
use narrastate_runtime::ports::{
    CommitOutcome, GenerationJobRecord, ImageProviderSettings, InstalledCaseRecord,
    LegacyBackfillReport, LlmCallMetadata, ProviderSettings, Repository, StorageError,
};
use sha2::{Digest, Sha256};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};
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
        let indexed_identity: Option<(String, Option<String>)> =
            sqlx::query_as("SELECT case_id, instance_id FROM sessions WHERE session_id = ?")
                .bind(session.session_id.to_string())
                .fetch_optional(&mut **transaction)
                .await
                .map_err(map_database)?;
        let Some((case_id, instance_id)) = indexed_identity else {
            return Err(StorageError::NotFound(format!(
                "session {}",
                session.session_id
            )));
        };
        if case_id != session.case_id.as_ref()
            || instance_id != session.instance_id.map(|id| id.to_string())
        {
            return Err(StorageError::Constraint(
                "session case_id and instance_id are immutable after creation".into(),
            ));
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
             (session_id, case_id, instance_id, status, revision, state_json, created_at, updated_at)
             VALUES (?, ?, ?, ?, 0, ?, ?, ?)",
        )
        .bind(session.session_id.to_string())
        .bind(session.case_id.as_ref())
        .bind(session.instance_id.map(|id| id.to_string()))
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
        let row: Option<(String, Option<String>)> =
            sqlx::query_as("SELECT state_json, instance_id FROM sessions WHERE session_id = ?")
                .bind(session_id.to_string())
                .fetch_optional(&self.pool)
                .await
                .map_err(map_database)?;
        let Some((json, stored_instance_id)) = row else {
            return Err(StorageError::NotFound(format!("session {session_id}")));
        };
        let state: SessionState = serde_json::from_str(&json)
            .map_err(|error| StorageError::Serialization(error.to_string()))?;
        let state_instance_id = state.instance_id.map(|id| id.to_string());
        if state_instance_id != stored_instance_id {
            return Err(StorageError::Constraint(format!(
                "session {session_id} instance ID differs between indexed column and state snapshot"
            )));
        }
        Ok(state)
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
        let mut recovered = recovered;
        match (recovered.instance_id, persisted.instance_id) {
            (None, Some(instance_id)) => recovered.instance_id = Some(instance_id),
            (Some(replayed), Some(indexed)) if replayed != indexed => {
                return Err(StorageError::Constraint(format!(
                    "session {session_id} replay instance {replayed} differs from indexed instance {indexed}"
                )))
            }
            _ => {}
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
            sqlx::query_scalar("SELECT definition_json FROM cases ORDER BY case_id")
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

    async fn install_case(&self, case: &InstalledCaseRecord) -> Result<(), StorageError> {
        let existing: Option<String> = sqlx::query_scalar(
            "SELECT template_content_hash FROM installed_cases
             WHERE case_id = ? AND case_version = ?",
        )
        .bind(case.case_id.as_ref())
        .bind(&case.case_version)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_database)?;
        if existing
            .as_ref()
            .is_some_and(|hash| hash != &case.template_content_hash)
        {
            return Err(StorageError::Constraint(format!(
                "installed case {} version {} already has different content",
                case.case_id, case.case_version
            )));
        }
        sqlx::query(
            "INSERT INTO installed_cases
             (case_id, case_version, source_path, schema_version, template_content_hash, installed_at)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(case_id, case_version) DO UPDATE SET
               source_path = excluded.source_path,
               installed_at = excluded.installed_at",
        )
        .bind(case.case_id.as_ref())
        .bind(&case.case_version)
        .bind(&case.source_path)
        .bind(&case.schema_version)
        .bind(&case.template_content_hash)
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(map_database)?;
        Ok(())
    }

    async fn list_installed_cases(&self) -> Result<Vec<InstalledCaseRecord>, StorageError> {
        type InstalledRow = (String, String, String, String, String);
        let rows: Vec<InstalledRow> = sqlx::query_as(
            "SELECT case_id, case_version, source_path, schema_version, template_content_hash
             FROM installed_cases ORDER BY case_id, case_version",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_database)?;
        Ok(rows
            .into_iter()
            .map(
                |(case_id, case_version, source_path, schema_version, template_content_hash)| {
                    InstalledCaseRecord {
                        case_id: CaseId::from(case_id),
                        case_version,
                        source_path,
                        schema_version,
                        template_content_hash,
                    }
                },
            )
            .collect())
    }

    async fn update_installed_case_visuals(
        &self,
        case: &InstalledCaseRecord,
    ) -> Result<(), StorageError> {
        let existing_schema: Option<String> = sqlx::query_scalar(
            "SELECT schema_version FROM installed_cases WHERE case_id = ? AND case_version = ?",
        )
        .bind(case.case_id.as_ref())
        .bind(&case.case_version)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_database)?;
        let Some(existing_schema) = existing_schema else {
            return Err(StorageError::NotFound(format!(
                "installed case {} {}",
                case.case_id, case.case_version
            )));
        };
        if existing_schema != case.schema_version {
            return Err(StorageError::Constraint(format!(
                "installed case {} {} schema version cannot change during visual update",
                case.case_id, case.case_version
            )));
        }
        sqlx::query(
            "UPDATE installed_cases
             SET source_path = ?, template_content_hash = ?, installed_at = ?
             WHERE case_id = ? AND case_version = ?",
        )
        .bind(&case.source_path)
        .bind(&case.template_content_hash)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(case.case_id.as_ref())
        .bind(&case.case_version)
        .execute(&self.pool)
        .await
        .map_err(map_database)?;
        Ok(())
    }

    async fn save_case_instance(&self, instance: &CaseInstance) -> Result<(), StorageError> {
        if !verify_compiled_hash(&instance.compiled_case)
            .map_err(|error| StorageError::Serialization(error.to_string()))?
        {
            return Err(StorageError::Constraint(
                "compiled case content hash does not match snapshot".into(),
            ));
        }
        if !verify_instance_hash(instance)
            .map_err(|error| StorageError::Serialization(error.to_string()))?
        {
            return Err(StorageError::Constraint(
                "case instance identity or hash is inconsistent".into(),
            ));
        }
        let compiled_json = serde_json::to_string(&instance.compiled_case)
            .map_err(|error| StorageError::Serialization(error.to_string()))?;
        sqlx::query(
            "INSERT INTO case_instances
             (instance_id, case_id, case_version, variant_id, selector_version, seed_text,
              compiled_content_hash, instance_hash, compiled_json, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(instance.instance_id.to_string())
        .bind(instance.case_id.as_ref())
        .bind(&instance.case_version)
        .bind(instance.variant_id.as_ref())
        .bind(selector_version_text(instance.selector_version))
        .bind(instance.seed.0.to_string())
        .bind(instance.compiled_content_hash.as_ref())
        .bind(instance.instance_hash.as_ref())
        .bind(compiled_json)
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(map_database)?;
        Ok(())
    }

    async fn load_case_instance(
        &self,
        instance_id: &CaseInstanceId,
    ) -> Result<CaseInstance, StorageError> {
        type InstanceRow = (
            String,
            String,
            String,
            String,
            String,
            String,
            String,
            String,
        );
        let row: Option<InstanceRow> = sqlx::query_as(
            "SELECT case_id, case_version, variant_id, selector_version, seed_text,
                    compiled_content_hash, instance_hash, compiled_json
             FROM case_instances WHERE instance_id = ?",
        )
        .bind(instance_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(map_database)?;
        let Some((case_id, case_version, variant_id, selector, seed, content_hash, hash, json)) =
            row
        else {
            return Err(StorageError::NotFound(format!(
                "case instance {instance_id}"
            )));
        };
        let compiled_case: CompiledCase = serde_json::from_str(&json)
            .map_err(|error| StorageError::Serialization(error.to_string()))?;
        let seed = seed.parse::<u64>().map(Seed).map_err(|error| {
            StorageError::Serialization(format!("invalid case instance seed: {error}"))
        })?;
        let selector_version = parse_selector_version(&selector)?;
        let instance = CaseInstance {
            instance_id: *instance_id,
            case_id: CaseId::from(case_id),
            case_version,
            variant_id: VariantId::from(variant_id),
            selector_version,
            seed,
            compiled_content_hash: ContentHash::from(content_hash),
            instance_hash: ContentHash::from(hash),
            compiled_case,
        };
        if !verify_compiled_hash(&instance.compiled_case)
            .map_err(|error| StorageError::Serialization(error.to_string()))?
            || !verify_instance_hash(&instance)
                .map_err(|error| StorageError::Serialization(error.to_string()))?
        {
            return Err(StorageError::Constraint(format!(
                "case instance {instance_id} failed content hash verification"
            )));
        }
        Ok(instance)
    }

    async fn backfill_legacy_session_instances(
        &self,
    ) -> Result<LegacyBackfillReport, StorageError> {
        let rows: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT session_id, case_id, state_json FROM sessions
             WHERE instance_id IS NULL ORDER BY session_id",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_database)?;
        let mut migrated = 0_usize;
        for (session_id_text, case_id_text, state_json) in rows {
            let mut state: SessionState = serde_json::from_str(&state_json)
                .map_err(|error| StorageError::Serialization(error.to_string()))?;
            if state.session_id.to_string() != session_id_text
                || state.case_id.as_ref() != case_id_text
            {
                return Err(StorageError::Constraint(format!(
                    "legacy session {session_id_text} indexed identity differs from state snapshot"
                )));
            }
            let case = self.load_case(&state.case_id).await?;
            let template = adapt_v01(case, "0.1.0", VariantId::from("classic"))
                .map_err(|error| StorageError::Constraint(error.to_string()))?;
            let compiled = compile(&template, &template.default_variant_id).map_err(|report| {
                StorageError::Constraint(
                    report
                        .errors
                        .into_iter()
                        .map(|issue| format!("{} at {}: {}", issue.code, issue.path, issue.message))
                        .collect::<Vec<_>>()
                        .join("; "),
                )
            })?;
            let digest = Sha256::digest(session_id_text.as_bytes());
            let seed = Seed(u64::from_be_bytes(
                digest[..8]
                    .try_into()
                    .expect("SHA-256 contains at least eight bytes"),
            ));
            let instance = freeze_case(compiled, seed);
            self.save_case_instance(&instance).await?;
            state.instance_id = Some(instance.instance_id);
            let updated_json = serde_json::to_string(&state)
                .map_err(|error| StorageError::Serialization(error.to_string()))?;
            let result = sqlx::query(
                "UPDATE sessions SET instance_id = ?, state_json = ?, updated_at = ?
                 WHERE session_id = ? AND instance_id IS NULL",
            )
            .bind(instance.instance_id.to_string())
            .bind(updated_json)
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(&session_id_text)
            .execute(&self.pool)
            .await
            .map_err(map_database)?;
            if result.rows_affected() != 1 {
                return Err(StorageError::Constraint(format!(
                    "legacy session {session_id_text} changed during instance backfill"
                )));
            }
            migrated += 1;
        }
        Ok(LegacyBackfillReport {
            migrated_sessions: migrated,
            limitations: (migrated > 0)
                .then(|| {
                    "Backfill uses the legacy case definition currently stored in SQLite; versions overwritten before this migration cannot be recovered."
                        .to_string()
                })
                .into_iter()
                .collect(),
        })
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

    async fn save_image_provider_settings(
        &self,
        settings: &ImageProviderSettings,
    ) -> Result<(), StorageError> {
        let json = serde_json::to_string(settings)
            .map_err(|error| StorageError::Serialization(error.to_string()))?;
        sqlx::query("INSERT OR REPLACE INTO settings (setting_key, value_json, updated_at) VALUES ('image-provider', ?, ?)")
            .bind(json).bind(chrono::Utc::now().to_rfc3339()).execute(&self.pool).await.map_err(map_database)?;
        Ok(())
    }

    async fn load_image_provider_settings(
        &self,
    ) -> Result<Option<ImageProviderSettings>, StorageError> {
        let value: Option<String> = sqlx::query_scalar(
            "SELECT value_json FROM settings WHERE setting_key = 'image-provider'",
        )
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

    async fn save_generation_job(&self, job: &GenerationJobRecord) -> Result<(), StorageError> {
        sqlx::query(
            "INSERT INTO case_generation_jobs
             (job_id, status, request_json, drafts_json, status_events_json,
              validation_report_json, result_path, attempt_count, repair_count,
              error_code, error_message, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(job_id) DO UPDATE SET
              status=excluded.status, drafts_json=excluded.drafts_json,
              status_events_json=excluded.status_events_json,
              validation_report_json=excluded.validation_report_json,
              result_path=excluded.result_path, attempt_count=excluded.attempt_count,
              repair_count=excluded.repair_count, error_code=excluded.error_code,
              error_message=excluded.error_message, updated_at=excluded.updated_at",
        )
        .bind(job.job_id.to_string())
        .bind(generation_status_text(job.status))
        .bind(&job.request_json)
        .bind(&job.drafts_json)
        .bind(&job.status_events_json)
        .bind(&job.validation_report_json)
        .bind(&job.result_path)
        .bind(i64::from(job.attempt_count))
        .bind(i64::from(job.repair_count))
        .bind(&job.error_code)
        .bind(&job.error_message)
        .bind(&job.created_at)
        .bind(&job.updated_at)
        .execute(&self.pool)
        .await
        .map_err(map_database)?;
        Ok(())
    }

    async fn load_generation_job(
        &self,
        job_id: &GenerationJobId,
    ) -> Result<GenerationJobRecord, StorageError> {
        let row = sqlx::query("SELECT * FROM case_generation_jobs WHERE job_id = ?")
            .bind(job_id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(map_database)?
            .ok_or_else(|| StorageError::NotFound(format!("generation job {job_id}")))?;
        Ok(GenerationJobRecord {
            job_id: *job_id,
            status: parse_generation_status(row.try_get("status").map_err(map_database)?)?,
            request_json: row.try_get("request_json").map_err(map_database)?,
            drafts_json: row.try_get("drafts_json").map_err(map_database)?,
            status_events_json: row.try_get("status_events_json").map_err(map_database)?,
            validation_report_json: row
                .try_get("validation_report_json")
                .map_err(map_database)?,
            result_path: row.try_get("result_path").map_err(map_database)?,
            attempt_count: u32::try_from(
                row.try_get::<i64, _>("attempt_count")
                    .map_err(map_database)?,
            )
            .map_err(|_| StorageError::Serialization("invalid attempt_count".into()))?,
            repair_count: u32::try_from(
                row.try_get::<i64, _>("repair_count")
                    .map_err(map_database)?,
            )
            .map_err(|_| StorageError::Serialization("invalid repair_count".into()))?,
            error_code: row.try_get("error_code").map_err(map_database)?,
            error_message: row.try_get("error_message").map_err(map_database)?,
            created_at: row.try_get("created_at").map_err(map_database)?,
            updated_at: row.try_get("updated_at").map_err(map_database)?,
        })
    }

    async fn fail_interrupted_generation_jobs(&self) -> Result<u64, StorageError> {
        let now = chrono::Utc::now().to_rfc3339();
        let result = sqlx::query(
            "UPDATE case_generation_jobs
             SET status='failed', error_code='GENERATION_INTERRUPTED',
                 error_message='server restarted while generation was in progress', updated_at=?
             WHERE status NOT IN ('completed', 'failed')",
        )
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(map_database)?;
        Ok(result.rows_affected())
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

fn selector_version_text(version: VariantSelectorVersion) -> &'static str {
    match version {
        VariantSelectorVersion::V1 => "selector-v1",
    }
}

fn parse_selector_version(value: &str) -> Result<VariantSelectorVersion, StorageError> {
    match value {
        "selector-v1" => Ok(VariantSelectorVersion::V1),
        other => Err(StorageError::Serialization(format!(
            "unsupported variant selector version {other}"
        ))),
    }
}

fn generation_status_text(status: GenerationStatus) -> &'static str {
    match status {
        GenerationStatus::Pending => "pending",
        GenerationStatus::Drafting => "drafting",
        GenerationStatus::Parsing => "parsing",
        GenerationStatus::Normalizing => "normalizing",
        GenerationStatus::Compiling => "compiling",
        GenerationStatus::Validating => "validating",
        GenerationStatus::Simulating => "simulating",
        GenerationStatus::Repairing => "repairing",
        GenerationStatus::Completed => "completed",
        GenerationStatus::Failed => "failed",
    }
}

fn parse_generation_status(value: &str) -> Result<GenerationStatus, StorageError> {
    match value {
        "pending" => Ok(GenerationStatus::Pending),
        "drafting" => Ok(GenerationStatus::Drafting),
        "parsing" => Ok(GenerationStatus::Parsing),
        "normalizing" => Ok(GenerationStatus::Normalizing),
        "compiling" => Ok(GenerationStatus::Compiling),
        "validating" => Ok(GenerationStatus::Validating),
        "simulating" => Ok(GenerationStatus::Simulating),
        "repairing" => Ok(GenerationStatus::Repairing),
        "completed" => Ok(GenerationStatus::Completed),
        "failed" => Ok(GenerationStatus::Failed),
        other => Err(StorageError::Serialization(format!(
            "unsupported generation status {other}"
        ))),
    }
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
