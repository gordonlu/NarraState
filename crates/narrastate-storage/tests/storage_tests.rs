use narrastate_core::case::CaseDefinition;
use narrastate_core::character::CharacterRuntimeState;
use narrastate_core::id::{CaseId, CharacterId, FactId, SessionId, TurnId};
use narrastate_core::session::{
    Accusation, AccusationResult, DialogueEntry, DialogueSpeaker, NarrativeEvent,
    NarrativeEventKind, SessionState, SessionStatus,
};
use narrastate_runtime::ports::{Repository, StorageError};
use narrastate_storage::SqliteRepository;

fn make_session() -> SessionState {
    SessionState {
        session_id: SessionId::new(),
        case_id: CaseId::from("test-case"),
        status: SessionStatus::Active,
        current_turn: 0,
        active_character: Some(CharacterId::from("suspect")),
        discovered_facts: [FactId::from("fact_a")].into(),
        discovered_evidence: Default::default(),
        character_states: Default::default(),
        conversation: vec![DialogueEntry {
            turn_id: TurnId::new(),
            speaker: DialogueSpeaker::Player,
            text: "Hello".into(),
            attached_evidence: vec![],
        }],
        accusations: vec![Accusation {
            turn_id: TurnId::new(),
            target: CharacterId::from("suspect"),
            evidence_ids: vec![],
            reasoning: "test".into(),
            result: AccusationResult::WrongSuspect,
        }],
        revision: 0,
    }
}

#[test]
fn test_session_roundtrip() {
    let repo = SqliteRepository::new_in_memory().unwrap();
    let session = make_session();

    repo.create_session(&session).unwrap();

    let loaded = repo.load_session(&session.session_id).unwrap();
    assert_eq!(loaded.session_id, session.session_id);
    assert_eq!(loaded.case_id, session.case_id);
    assert_eq!(loaded.status, session.status);
    assert_eq!(loaded.current_turn, session.current_turn);
    assert_eq!(loaded.conversation.len(), session.conversation.len());
    assert_eq!(loaded.accusations.len(), session.accusations.len());
    assert_eq!(loaded.revision, 0);
}

#[test]
fn test_session_not_found() {
    let repo = SqliteRepository::new_in_memory().unwrap();
    let err = repo.load_session(&SessionId::new()).unwrap_err();
    assert!(matches!(err, StorageError::NotFound(_)));
}

#[test]
fn test_session_duplicate_fails() {
    let repo = SqliteRepository::new_in_memory().unwrap();
    let session = make_session();
    repo.create_session(&session).unwrap();
    let err = repo.create_session(&session).unwrap_err();
    assert!(matches!(err, StorageError::Constraint(_)));
}

#[test]
fn test_session_update_and_revision_conflict() {
    let repo = SqliteRepository::new_in_memory().unwrap();
    let mut session = make_session();
    repo.create_session(&session).unwrap();

    session.revision = 1;
    session.current_turn = 5;
    repo.update_session(&session).unwrap();

    let loaded = repo.load_session(&session.session_id).unwrap();
    assert_eq!(loaded.current_turn, 5);

    session.revision = 1;
    session.current_turn = 10;
    let err = repo.update_session(&session).unwrap_err();
    assert!(matches!(err, StorageError::RevisionConflict { .. }));
}

#[test]
fn test_case_roundtrip() {
    let repo = SqliteRepository::new_in_memory().unwrap();
    let json = r#"{
        "schema_version": "0.1.0",
        "id": "rain-gallery",
        "title": "Rain Gallery",
        "summary": "A test case",
        "locale": "zh-CN",
        "required_case_elements": ["Identity","Opportunity","Means"],
        "entities": [],
        "facts": [
            {
                "id": "fact_a",
                "subject": "gallery",
                "predicate": "is_raining",
                "object": "true",
                "truth": "True",
                "visibility": "PublicAtStart",
                "tags": []
            }
        ],
        "evidence": [],
        "characters": [
            {
                "id": "suspect",
                "name": "Suspect",
                "role": "Suspect",
                "public_profile": "",
                "personality": {"traits": ["nervous"], "speech_style": null},
                "goals": [],
                "knowledge": [],
                "initial_beliefs": [],
                "claims": [],
                "defenses": [],
                "disclosure_graph": {
                    "nodes": [],
                    "edges": []
                },
                "resilience": 5
            }
        ],
        "initial_player_knowledge": {"fact_ids": [], "evidence_ids": []},
        "ending": null
    }"#;
    let case: CaseDefinition = serde_json::from_str(json).unwrap();

    repo.save_case(&case).unwrap();
    let loaded = repo.load_case(&CaseId::from("rain-gallery")).unwrap();
    assert_eq!(loaded.id, case.id);
    assert_eq!(loaded.title, "Rain Gallery");
    assert_eq!(loaded.facts.len(), 1);

    let list = repo.list_cases().unwrap();
    assert_eq!(list.len(), 1);
}

#[test]
fn test_case_not_found() {
    let repo = SqliteRepository::new_in_memory().unwrap();
    let err = repo.load_case(&CaseId::from("nonexistent")).unwrap_err();
    assert!(matches!(err, StorageError::NotFound(_)));
}

#[test]
fn test_event_append_and_load() {
    let repo = SqliteRepository::new_in_memory().unwrap();
    let session = make_session();
    repo.create_session(&session).unwrap();

    let events = vec![
        NarrativeEvent {
            event_id: uuid::Uuid::new_v4(),
            session_id: session.session_id,
            turn_id: None,
            sequence: 0,
            event_type: NarrativeEventKind::SessionCreated,
            schema_version: 1,
            payload: serde_json::json!({}),
        },
        NarrativeEvent {
            event_id: uuid::Uuid::new_v4(),
            session_id: session.session_id,
            turn_id: Some(TurnId::new()),
            sequence: 1,
            event_type: NarrativeEventKind::TurnProcessed,
            schema_version: 1,
            payload: serde_json::json!({"turn": 1}),
        },
    ];

    repo.append_events(&session.session_id, &events).unwrap();

    let loaded = repo.load_events(&session.session_id).unwrap();
    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded[0].sequence, 0);
    assert_eq!(loaded[0].event_type, NarrativeEventKind::SessionCreated);
    assert_eq!(loaded[1].sequence, 1);
    assert_eq!(loaded[1].event_type, NarrativeEventKind::TurnProcessed);
}

#[test]
fn test_snapshot_roundtrip() {
    let repo = SqliteRepository::new_in_memory().unwrap();
    let session = make_session();
    repo.create_session(&session).unwrap();

    let snapshot = repo.load_latest_snapshot(&session.session_id).unwrap();
    assert!(snapshot.is_none());

    let mut updated = session.clone();
    updated.current_turn = 10;
    updated.revision = 1;
    repo.save_snapshot(&session.session_id, 1, &updated)
        .unwrap();

    let (rev, state) = repo
        .load_latest_snapshot(&session.session_id)
        .unwrap()
        .expect("should have snapshot");
    assert_eq!(rev, 1);
    assert_eq!(state.current_turn, 10);

    let mut newer = session.clone();
    newer.current_turn = 20;
    newer.revision = 2;
    repo.save_snapshot(&session.session_id, 2, &newer).unwrap();

    let (rev2, _) = repo
        .load_latest_snapshot(&session.session_id)
        .unwrap()
        .expect("should have later snapshot");
    assert_eq!(rev2, 2);
}

#[test]
fn test_session_concurrent_creation_fails() {
    let repo = SqliteRepository::new_in_memory().unwrap();
    let session = make_session();
    repo.create_session(&session).unwrap();

    let session2 = SessionState {
        session_id: session.session_id,
        ..make_session()
    };
    let err = repo.create_session(&session2).unwrap_err();
    assert!(matches!(err, StorageError::Constraint(_)));
}

#[test]
fn test_character_runtime_state_in_session() {
    let repo = SqliteRepository::new_in_memory().unwrap();
    let mut session = make_session();
    let char_id = CharacterId::from("suspect");
    let char_state = CharacterRuntimeState::new(5);
    session.character_states.insert(char_id.clone(), char_state);

    repo.create_session(&session).unwrap();

    let loaded = repo.load_session(&session.session_id).unwrap();
    let loaded_char = loaded.character_states.get(&char_id).unwrap();
    assert_eq!(loaded_char.stress, 0);
    assert_eq!(loaded_char.composure, 100);
    assert_eq!(loaded_char.defense_budget, 100);
    assert_eq!(loaded_char.trust, 0);
}
