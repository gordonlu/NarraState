use narrastate_core::{
    ClientActionId, GenerationJobId, GenerationStatus, NarrativeEvent, NarrativeEventKind,
    NarrativeEventPayload, Seed, SessionId, SessionMode, SessionState, SessionStatus, VariantId,
};
use narrastate_runtime::ports::{
    CommitOutcome, GenerationJobRecord, ImageProviderSettings, InstalledCaseRecord,
    LlmCallMetadata, ProviderSettings, Repository, StorageError,
};
use narrastate_storage::SqliteRepository;
use uuid::Uuid;

fn load_case() -> narrastate_core::CaseDefinition {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cases/rain-gallery/case.json");
    let json = std::fs::read_to_string(path).expect("rain-gallery fixture");
    serde_json::from_str(&json).expect("valid case JSON")
}

fn session(case: &narrastate_core::CaseDefinition) -> SessionState {
    SessionState {
        session_id: SessionId::new(),
        case_id: case.id.clone(),
        instance_id: None,
        mode: SessionMode::Mock,
        status: SessionStatus::Active,
        current_turn: 0,
        active_character: case
            .characters
            .first()
            .map(|character| character.id.clone()),
        discovered_facts: case
            .initial_player_knowledge
            .fact_ids
            .iter()
            .cloned()
            .collect(),
        discovered_evidence: case
            .evidence
            .iter()
            .map(|evidence| evidence.id.clone())
            .collect(),
        character_states: case
            .characters
            .iter()
            .map(|character| {
                (
                    character.id.clone(),
                    narrastate_core::CharacterRuntimeState::new(character.resilience),
                )
            })
            .collect(),
        conversation: Vec::new(),
        accusations: Vec::new(),
        revision: 0,
    }
}

fn frozen_instance(seed: u64) -> narrastate_core::CaseInstance {
    let template = narrastate_case::adapt_v01(load_case(), "1.0.0", VariantId::from("classic"))
        .expect("legacy adapter");
    let compiled =
        narrastate_case::compile(&template, &VariantId::from("classic")).expect("compile case");
    narrastate_case::freeze_case(compiled, Seed(seed))
}

fn created_event(state: &SessionState) -> NarrativeEvent {
    NarrativeEvent {
        event_id: Uuid::new_v4(),
        session_id: state.session_id,
        turn_id: None,
        sequence: 0,
        event_type: NarrativeEventKind::SessionCreated,
        schema_version: 1,
        payload: NarrativeEventPayload::SessionCreated {
            state: Box::new(state.clone()),
        },
    }
}

fn committed_event(state: &SessionState, action: ClientActionId, sequence: u64) -> NarrativeEvent {
    NarrativeEvent {
        event_id: Uuid::new_v4(),
        session_id: state.session_id,
        turn_id: None,
        sequence,
        event_type: NarrativeEventKind::TurnCommitted,
        schema_version: 1,
        payload: NarrativeEventPayload::TurnCommitted {
            client_action_id: action,
            state: Box::new(state.clone()),
        },
    }
}

async fn repository_with_session() -> (SqliteRepository, SessionState) {
    let repo = SqliteRepository::new_in_memory().await.expect("repository");
    let case = load_case();
    repo.save_case(&case).await.expect("save case");
    let state = session(&case);
    repo.create_session(&state, &[created_event(&state)])
        .await
        .expect("create session atomically");
    (repo, state)
}

#[tokio::test]
async fn case_list_order_is_stable_across_load_order() {
    let repo = SqliteRepository::new_in_memory().await.expect("repository");
    let mut later = load_case();
    later.id = "z-case".into();
    let mut earlier = load_case();
    earlier.id = "a-case".into();

    repo.save_case(&later).await.expect("save later case");
    repo.save_case(&earlier).await.expect("save earlier case");

    let ids: Vec<_> = repo
        .list_cases()
        .await
        .expect("list cases")
        .into_iter()
        .map(|case| case.id)
        .collect();
    assert_eq!(ids, vec!["a-case".into(), "z-case".into()]);
}

#[tokio::test]
async fn installed_visual_update_changes_only_mutable_package_index_fields() {
    let repo = SqliteRepository::new_in_memory().await.unwrap();
    let original = InstalledCaseRecord {
        case_id: "generated-case".into(),
        case_version: "1.0.0".into(),
        source_path: "/tmp/generated-case/1.0.0".into(),
        schema_version: "0.2".into(),
        template_content_hash: "sha256:old".into(),
    };
    repo.install_case(&original).await.unwrap();
    let mut updated = original.clone();
    updated.template_content_hash = "sha256:new-visuals".into();
    repo.update_installed_case_visuals(&updated).await.unwrap();

    let records = repo.list_installed_cases().await.unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].template_content_hash, "sha256:new-visuals");
    assert_eq!(records[0].case_version, "1.0.0");
    assert_eq!(records[0].schema_version, "0.2");

    let mut missing = updated;
    missing.case_id = "missing".into();
    assert!(matches!(
        repo.update_installed_case_visuals(&missing).await,
        Err(StorageError::NotFound(_))
    ));
}

#[tokio::test]
async fn migration_case_session_settings_and_llm_metadata_roundtrip() {
    let (repo, state) = repository_with_session().await;
    let loaded = repo.load_session(&state.session_id).await.expect("session");
    assert_eq!(loaded.revision, 0);
    assert_eq!(repo.list_cases().await.expect("cases").len(), 1);

    let settings = ProviderSettings {
        base_url: "https://example.invalid/v1".into(),
        model: "test-model".into(),
    };
    repo.save_provider_settings(&settings)
        .await
        .expect("settings");
    let loaded_settings = repo
        .load_provider_settings()
        .await
        .expect("load settings")
        .expect("settings exist");
    assert_eq!(loaded_settings.model, "test-model");
    repo.save_image_provider_settings(&ImageProviderSettings {
        enabled: true,
        base_url: "https://images.example.invalid/v1".into(),
        model: "image-model".into(),
    })
    .await
    .unwrap();
    let image_settings = repo.load_image_provider_settings().await.unwrap().unwrap();
    assert_eq!(image_settings.model, "image-model");
    assert_eq!(loaded_settings.model, "test-model");

    repo.record_llm_call(&LlmCallMetadata {
        call_id: Uuid::new_v4().to_string(),
        session_id: state.session_id,
        turn_id: None,
        purpose: "interpreter".into(),
        provider: "openai-compatible".into(),
        model: "test-model".into(),
        prompt_hash: "redacted-hash".into(),
        latency_ms: 12,
        input_tokens: Some(10),
        output_tokens: Some(4),
        status: "ok".into(),
        error_code: None,
    })
    .await
    .expect("record metadata");
    let calls = repo
        .load_llm_calls(&state.session_id)
        .await
        .expect("load metadata");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].model, "test-model");
    assert_eq!(calls[0].input_tokens, Some(10));
}

#[tokio::test]
async fn frozen_instance_roundtrips_u64_seed_as_text() {
    let repo = SqliteRepository::new_in_memory().await.unwrap();
    let instance = frozen_instance(u64::MAX);
    repo.save_case_instance(&instance).await.unwrap();

    let loaded = repo
        .load_case_instance(&instance.instance_id)
        .await
        .unwrap();
    assert_eq!(loaded.seed, Seed(u64::MAX));
    assert_eq!(loaded.instance_hash, instance.instance_hash);
    assert_eq!(
        loaded.compiled_case.definition.title,
        instance.compiled_case.definition.title
    );
}

#[tokio::test]
async fn installed_case_version_cannot_silently_change_content() {
    let repo = SqliteRepository::new_in_memory().await.unwrap();
    let mut record = InstalledCaseRecord {
        case_id: narrastate_core::CaseId::from("rain-gallery"),
        case_version: "1.0.0".into(),
        source_path: "/first/path".into(),
        schema_version: "0.2".into(),
        template_content_hash: "sha256:first".into(),
    };
    repo.install_case(&record).await.unwrap();
    record.source_path = "/moved/path".into();
    repo.install_case(&record).await.unwrap();
    let installed = repo.list_installed_cases().await.unwrap();
    assert_eq!(installed[0].source_path, "/moved/path");

    record.template_content_hash = "sha256:different".into();
    let error = repo
        .install_case(&record)
        .await
        .expect_err("same version cannot change semantic content");
    assert!(matches!(error, StorageError::Constraint(_)));
}

#[tokio::test]
async fn frozen_instance_is_insert_only() {
    let repo = SqliteRepository::new_in_memory().await.unwrap();
    let instance = frozen_instance(42);
    repo.save_case_instance(&instance).await.unwrap();
    let error = repo
        .save_case_instance(&instance)
        .await
        .expect_err("instance ID must be immutable");
    assert!(matches!(error, StorageError::Constraint(_)));
}

#[tokio::test]
async fn corrupted_compiled_snapshot_is_rejected_before_storage() {
    let repo = SqliteRepository::new_in_memory().await.unwrap();
    let mut instance = frozen_instance(42);
    instance.compiled_case.definition.title = "tampered".into();
    let error = repo
        .save_case_instance(&instance)
        .await
        .expect_err("hash mismatch must fail");
    assert!(matches!(error, StorageError::Constraint(_)));
}

#[tokio::test]
async fn session_instance_foreign_key_requires_frozen_snapshot_first() {
    let repo = SqliteRepository::new_in_memory().await.unwrap();
    let case = load_case();
    repo.save_case(&case).await.unwrap();
    let instance = frozen_instance(42);
    let mut state = session(&case);
    state.instance_id = Some(instance.instance_id);
    let error = repo
        .create_session(&state, &[created_event(&state)])
        .await
        .expect_err("session cannot reference a missing instance");
    assert!(matches!(error, StorageError::Constraint(_)));
}

#[tokio::test]
async fn installed_case_update_does_not_change_existing_session_instance() {
    let repo = SqliteRepository::new_in_memory().await.unwrap();
    let original = load_case();
    repo.save_case(&original).await.unwrap();
    let instance = frozen_instance(928_341);
    repo.save_case_instance(&instance).await.unwrap();
    let mut state = session(&original);
    state.instance_id = Some(instance.instance_id);
    repo.create_session(&state, &[created_event(&state)])
        .await
        .unwrap();

    let mut updated = original;
    updated.title = "磁盘上更新后的标题".into();
    repo.save_case(&updated).await.unwrap();

    let restored_session = repo.load_session(&state.session_id).await.unwrap();
    let restored_instance = repo
        .load_case_instance(&restored_session.instance_id.unwrap())
        .await
        .unwrap();
    assert_ne!(
        updated.title,
        restored_instance.compiled_case.definition.title
    );
    assert_eq!(
        restored_instance.compiled_case.definition.title,
        instance.compiled_case.definition.title
    );
}

#[tokio::test]
async fn committed_turn_cannot_switch_or_remove_frozen_instance() {
    let repo = SqliteRepository::new_in_memory().await.unwrap();
    let case = load_case();
    repo.save_case(&case).await.unwrap();
    let instance = frozen_instance(42);
    repo.save_case_instance(&instance).await.unwrap();
    let mut state = session(&case);
    state.instance_id = Some(instance.instance_id);
    repo.create_session(&state, &[created_event(&state)])
        .await
        .unwrap();

    state.instance_id = None;
    state.revision = 1;
    let error = repo
        .commit_session(0, &state, &[])
        .await
        .expect_err("instance binding is immutable");
    assert!(matches!(error, StorageError::Constraint(_)));
    let restored = repo.load_session(&state.session_id).await.unwrap();
    assert_eq!(restored.instance_id, Some(instance.instance_id));
    assert_eq!(restored.revision, 0);
}

#[tokio::test]
async fn migrations_are_idempotent_across_repository_reopen() {
    let path = std::env::temp_dir().join(format!("narrastate-{}.db", Uuid::new_v4()));
    SqliteRepository::new(path.to_str().unwrap()).await.unwrap();
    SqliteRepository::new(path.to_str().unwrap()).await.unwrap();
    std::fs::remove_file(&path).ok();
    std::fs::remove_file(path.with_extension("db-shm")).ok();
    std::fs::remove_file(path.with_extension("db-wal")).ok();
}

#[tokio::test]
async fn legacy_session_backfill_freezes_database_case_before_disk_update() {
    let repo = SqliteRepository::new_in_memory().await.unwrap();
    let original = load_case();
    repo.save_case(&original).await.unwrap();
    let state = session(&original);
    assert!(state.instance_id.is_none());
    repo.create_session(&state, &[created_event(&state)])
        .await
        .unwrap();

    let report = repo.backfill_legacy_session_instances().await.unwrap();
    assert_eq!(report.migrated_sessions, 1);
    assert_eq!(report.limitations.len(), 1);
    let migrated = repo.load_session(&state.session_id).await.unwrap();
    let instance_id = migrated.instance_id.expect("backfilled instance");

    let mut updated = original;
    updated.title = "new disk definition".into();
    repo.save_case(&updated).await.unwrap();
    let recovered = repo.recover_session(&state.session_id).await.unwrap();
    assert_eq!(recovered.instance_id, Some(instance_id));
    let instance = repo.load_case_instance(&instance_id).await.unwrap();
    assert_ne!(instance.compiled_case.definition.title, updated.title);

    let second = repo.backfill_legacy_session_instances().await.unwrap();
    assert_eq!(second.migrated_sessions, 0);
}

#[tokio::test]
async fn g9_client_action_retry_is_idempotent() {
    let (repo, mut state) = repository_with_session().await;
    let action = ClientActionId::new();
    state.current_turn = 1;
    state.revision = 1;
    let response = serde_json::json!({"revision": 1, "utterance": "ok"});
    let event = committed_event(&state, action, 1);

    let first = repo
        .commit_turn(0, &action, &state, &[event], &response)
        .await
        .expect("first commit");
    assert!(matches!(first, CommitOutcome::Committed));

    let retry = repo
        .commit_turn(0, &action, &state, &[], &response)
        .await
        .expect("idempotent retry");
    match retry {
        CommitOutcome::Idempotent(stored) => assert_eq!(stored, response),
        CommitOutcome::Committed => panic!("retry must not commit again"),
    }
    assert_eq!(
        repo.load_session(&state.session_id).await.unwrap().revision,
        1
    );
    assert_eq!(repo.load_events(&state.session_id).await.unwrap().len(), 2);
}

#[tokio::test]
async fn stale_distinct_action_returns_revision_conflict() {
    let (repo, mut state) = repository_with_session().await;
    let first = ClientActionId::new();
    state.revision = 1;
    repo.commit_turn(
        0,
        &first,
        &state,
        &[committed_event(&state, first, 1)],
        &serde_json::json!({"revision":1}),
    )
    .await
    .unwrap();
    let error = repo
        .commit_turn(
            0,
            &ClientActionId::new(),
            &state,
            &[],
            &serde_json::json!({}),
        )
        .await
        .expect_err("stale revision must fail");
    assert!(matches!(
        error,
        StorageError::RevisionConflict { actual: 1, .. }
    ));
}

#[tokio::test]
async fn event_failure_rolls_back_session_update() {
    let (repo, mut state) = repository_with_session().await;
    state.revision = 1;
    let duplicate_sequence = committed_event(&state, ClientActionId::new(), 0);
    let error = repo
        .commit_session(0, &state, &[duplicate_sequence])
        .await
        .expect_err("duplicate event sequence must abort transaction");
    assert!(matches!(error, StorageError::Constraint(_)));
    assert_eq!(
        repo.load_session(&state.session_id).await.unwrap().revision,
        0
    );
}

#[tokio::test]
async fn g10_replay_matches_latest_committed_state() {
    let (repo, mut state) = repository_with_session().await;
    for revision in 1..=3 {
        let action = ClientActionId::new();
        state.current_turn = revision as u32;
        state.revision = revision;
        repo.commit_turn(
            revision - 1,
            &action,
            &state,
            &[committed_event(&state, action, revision)],
            &serde_json::json!({"revision":revision}),
        )
        .await
        .unwrap();
    }
    let recovered = repo
        .recover_session(&state.session_id)
        .await
        .expect("replay");
    assert_eq!(recovered.revision, state.revision);
    assert_eq!(recovered.current_turn, state.current_turn);
    assert_eq!(
        serde_json::to_value(recovered).unwrap(),
        serde_json::to_value(state).unwrap()
    );
}

#[tokio::test]
async fn successful_tenth_revision_creates_snapshot() {
    let (repo, mut state) = repository_with_session().await;
    for revision in 1..=10 {
        let action = ClientActionId::new();
        state.current_turn = revision as u32;
        state.revision = revision;
        repo.commit_turn(
            revision - 1,
            &action,
            &state,
            &[committed_event(&state, action, revision)],
            &serde_json::json!({"revision":revision}),
        )
        .await
        .unwrap();
    }
    let (revision, snapshot) = repo
        .load_latest_snapshot(&state.session_id)
        .await
        .unwrap()
        .expect("automatic snapshot");
    assert_eq!(revision, 10);
    assert_eq!(snapshot.revision, 10);
}

#[tokio::test]
async fn generation_job_roundtrips_and_restart_fails_non_terminal_work() {
    let repo = SqliteRepository::new_in_memory().await.unwrap();
    let job_id = GenerationJobId::new();
    let job = GenerationJobRecord {
        job_id,
        status: GenerationStatus::Repairing,
        request_json: "{}".into(),
        drafts_json: "[]".into(),
        status_events_json: "[]".into(),
        validation_report_json: None,
        result_path: None,
        attempt_count: 2,
        repair_count: 1,
        error_code: None,
        error_message: None,
        created_at: "2026-07-15T00:00:00Z".into(),
        updated_at: "2026-07-15T00:00:01Z".into(),
    };
    repo.save_generation_job(&job).await.unwrap();
    let loaded = repo.load_generation_job(&job.job_id).await.unwrap();
    assert_eq!(loaded.status, GenerationStatus::Repairing);
    assert_eq!(loaded.repair_count, 1);

    assert_eq!(repo.fail_interrupted_generation_jobs().await.unwrap(), 1);
    let failed = repo.load_generation_job(&job.job_id).await.unwrap();
    assert_eq!(failed.status, GenerationStatus::Failed);
    assert_eq!(failed.error_code.as_deref(), Some("GENERATION_INTERRUPTED"));
    assert_eq!(repo.fail_interrupted_generation_jobs().await.unwrap(), 0);
}
