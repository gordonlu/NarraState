use narrastate_core::{
    ClientActionId, NarrativeEvent, NarrativeEventKind, NarrativeEventPayload, SessionId,
    SessionMode, SessionState, SessionStatus,
};
use narrastate_runtime::ports::{
    CommitOutcome, LlmCallMetadata, ProviderSettings, Repository, StorageError,
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
