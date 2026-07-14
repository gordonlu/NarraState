use std::collections::BTreeMap;

use narrastate_core::case::CaseDefinition;
use narrastate_core::character::CharacterRuntimeState;
use narrastate_core::id::*;
use narrastate_core::session::{NarrativeEvent, NarrativeEventKind, SessionState, SessionStatus};
use narrastate_runtime::ports::Repository;

fn load_case() -> CaseDefinition {
    // Path relative to workspace root (where tests run from by default)
    let path = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let json = std::fs::read_to_string(format!("{path}/../../cases/rain-gallery/case.json"))
        .or_else(|_| std::fs::read_to_string("cases/rain-gallery/case.json"))
        .expect("rain-gallery case file not found (run from workspace root)");
    serde_json::from_str(&json).expect("rain-gallery case.json parse failed")
}

#[test]
fn smoke_storage_save_load_case() {
    let case = load_case();
    let repo = narrastate_storage::SqliteRepository::new_in_memory()
        .expect("Failed to create in-memory repo");

    repo.save_case(&case).expect("save_case should succeed");
    let loaded = repo.load_case(&case.id).expect("load_case should succeed");
    assert_eq!(loaded.id, case.id);
    assert_eq!(loaded.evidence.len(), case.evidence.len());
    assert_eq!(loaded.characters.len(), case.characters.len());
    assert_eq!(loaded.facts.len(), case.facts.len());

    let list = repo.list_cases().expect("list_cases should succeed");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, case.id);
}

#[test]
fn smoke_storage_create_and_load_session() {
    let case = load_case();
    let repo = narrastate_storage::SqliteRepository::new_in_memory()
        .expect("Failed to create in-memory repo");
    repo.save_case(&case).expect("save_case");

    let session = create_test_session(&case);
    repo.create_session(&session)
        .expect("create_session should succeed");
    let loaded = repo
        .load_session(&session.session_id)
        .expect("load_session should succeed");

    assert_eq!(loaded.session_id, session.session_id);
    assert_eq!(loaded.case_id, session.case_id);
    assert_eq!(loaded.status, session.status);
    assert_eq!(loaded.current_turn, session.current_turn);
    assert_eq!(loaded.active_character, session.active_character);
    assert_eq!(loaded.accusations.len(), session.accusations.len());
    assert_eq!(loaded.conversation.len(), session.conversation.len());
    assert_eq!(loaded.revision, session.revision);
}

#[test]
fn smoke_storage_update_session_with_optimistic_locking() {
    let case = load_case();
    let repo = narrastate_storage::SqliteRepository::new_in_memory()
        .expect("Failed to create in-memory repo");
    repo.save_case(&case).expect("save_case");

    let mut session = create_test_session(&case);
    repo.create_session(&session).expect("create_session");

    // First update should succeed
    session.revision = 1;
    repo.update_session(&session)
        .expect("first update should succeed");

    // Same revision should fail (conflict)
    let err = repo.update_session(&session).unwrap_err();
    match &err {
        narrastate_runtime::ports::StorageError::RevisionConflict { expected, actual } => {
            assert_eq!(*expected, 1);
            assert_eq!(*actual, 1);
        }
        _ => panic!("Expected RevisionConflict, got: {err}"),
    }

    // Bump revision and it should succeed again
    session.revision = 2;
    repo.update_session(&session)
        .expect("second update should succeed");
}

#[test]
fn smoke_storage_append_and_load_events() {
    let case = load_case();
    let repo = narrastate_storage::SqliteRepository::new_in_memory()
        .expect("Failed to create in-memory repo");
    repo.save_case(&case).expect("save_case");

    let session = create_test_session(&case);
    repo.create_session(&session).expect("create_session");

    let turn_id = TurnId::new();
    let events = vec![NarrativeEvent {
        event_id: uuid::Uuid::new_v4(),
        session_id: session.session_id,
        turn_id: Some(turn_id),
        sequence: 0,
        event_type: NarrativeEventKind::TurnProcessed,
        schema_version: 1,
        payload: serde_json::json!({"turn_text": "你好"}),
    }];

    repo.append_events(&session.session_id, &events)
        .expect("append_events should succeed");

    let loaded = repo
        .load_events(&session.session_id)
        .expect("load_events should succeed");
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].sequence, 0);
    assert_eq!(loaded[0].turn_id, Some(turn_id));
    assert_eq!(loaded[0].event_type, NarrativeEventKind::TurnProcessed);
}

#[test]
fn smoke_storage_snapshot_roundtrip() {
    let case = load_case();
    let repo = narrastate_storage::SqliteRepository::new_in_memory()
        .expect("Failed to create in-memory repo");
    repo.save_case(&case).expect("save_case");

    let session = create_test_session(&case);
    repo.create_session(&session).expect("create_session");

    // No snapshots initially
    let maybe = repo
        .load_latest_snapshot(&session.session_id)
        .expect("load_latest_snapshot should not error");
    assert!(maybe.is_none());

    // Save snapshot at revision 1
    repo.save_snapshot(&session.session_id, 1, &session)
        .expect("save_snapshot should succeed");

    let (rev, loaded) = repo
        .load_latest_snapshot(&session.session_id)
        .expect("load_latest_snapshot should succeed")
        .expect("snapshot should exist");
    assert_eq!(rev, 1);
    assert_eq!(loaded.session_id, session.session_id);

    // Overwrite with revision 2
    repo.save_snapshot(&session.session_id, 2, &session)
        .expect("save_snapshot v2 should succeed");

    let (rev, _) = repo
        .load_latest_snapshot(&session.session_id)
        .expect("load_latest_snapshot should succeed")
        .expect("snapshot should exist");
    assert_eq!(rev, 2, "Should return latest revision");
}

#[test]
fn smoke_storage_new_session_not_found() {
    let repo = narrastate_storage::SqliteRepository::new_in_memory()
        .expect("Failed to create in-memory repo");
    let sid = SessionId::new();
    let err = repo.load_session(&sid).unwrap_err();
    match &err {
        narrastate_runtime::ports::StorageError::NotFound(msg) => {
            assert!(msg.contains(&sid.to_string()));
        }
        _ => panic!("Expected NotFound, got: {err}"),
    }
}

fn create_test_session(case: &CaseDefinition) -> SessionState {
    let character_states: BTreeMap<_, _> = case
        .characters
        .iter()
        .map(|ch| (ch.id.clone(), CharacterRuntimeState::new(ch.resilience)))
        .collect();

    SessionState {
        session_id: SessionId::new(),
        case_id: case.id.clone(),
        status: SessionStatus::Active,
        current_turn: 0,
        active_character: None,
        discovered_facts: case.facts.iter().map(|f| f.id.clone()).collect(),
        discovered_evidence: case.evidence.iter().map(|e| e.id.clone()).collect(),
        character_states,
        conversation: vec![],
        accusations: vec![],
        revision: 0,
    }
}
