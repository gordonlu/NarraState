use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use narrastate_core::case::CaseDefinition;
use narrastate_core::character::CharacterRuntimeState;
use narrastate_core::evidence::EvidenceDefinition;
use narrastate_core::id::{CaseId, CharacterId, EvidenceId, SessionId, TurnId};
use narrastate_core::phase::InterrogationPhase;
use narrastate_core::session::{
    AccusationResult, DialogueEntry, DialogueSpeaker, NarrativeEvent, NarrativeEventKind,
    SessionState, SessionStatus,
};
use narrastate_core::transition::TransitionTuning;
use narrastate_runtime::mock::{MockInterpreter, MockRenderer};
use narrastate_runtime::ports::{Repository, StorageError};
use narrastate_runtime::{DialoguePlanner, TransitionEngine};
use uuid::Uuid;

pub struct AppState {
    pub repo: Box<dyn Repository>,
    pub engine: TransitionEngine,
    pub planner: DialoguePlanner,
    pub interpreter: MockInterpreter,
    pub renderer: MockRenderer,
}

impl AppState {
    pub fn new(repo: Box<dyn Repository>) -> Self {
        Self {
            repo,
            engine: TransitionEngine::new(TransitionTuning::default()),
            planner: DialoguePlanner,
            interpreter: MockInterpreter,
            renderer: MockRenderer,
        }
    }
}

pub fn router(state: Arc<RwLock<AppState>>) -> Router {
    Router::new()
        .route("/api/v1/health", get(health))
        .route("/api/v1/cases", get(list_cases))
        .route("/api/v1/cases/{case_id}", get(get_case))
        .route("/api/v1/cases/validate", post(validate_case))
        .route("/api/v1/sessions", post(create_session))
        .route("/api/v1/sessions/{session_id}", get(get_session))
        .route(
            "/api/v1/sessions/{session_id}/events",
            get(get_session_events),
        )
        .route(
            "/api/v1/sessions/{session_id}/actions",
            post(process_action),
        )
        .route(
            "/api/v1/sessions/{session_id}/accusations",
            post(make_accusation),
        )
        .route(
            "/api/v1/sessions/{session_id}/restart",
            post(restart_session),
        )
        .with_state(state)
}

// ── Models ────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

#[derive(Serialize)]
pub struct CaseSummary {
    pub id: CaseId,
    pub title: String,
    pub summary: String,
    pub locale: String,
    pub character_count: usize,
    pub evidence_count: usize,
}

#[derive(Serialize)]
pub struct SessionResponse {
    pub session_id: SessionId,
    pub case_id: CaseId,
    pub status: SessionStatus,
    pub current_turn: u32,
    pub active_character: Option<CharacterId>,
    pub phase: Option<InterrogationPhase>,
    pub stress: Option<u8>,
    pub composure: Option<u8>,
    pub trust: Option<i8>,
    pub defense_budget: Option<u8>,
    pub discovered_facts: Vec<String>,
    pub discovered_evidence: Vec<String>,
    pub conversation: Vec<DialogueEntry>,
    pub accusations: Vec<narrastate_core::session::Accusation>,
    pub revision: u64,
}

impl From<SessionState> for SessionResponse {
    fn from(s: SessionState) -> Self {
        let (phase, stress, composure, trust, defense_budget) = match s
            .active_character
            .as_ref()
            .and_then(|cid| s.character_states.get(cid))
        {
            Some(cs) => (
                Some(cs.phase),
                Some(cs.stress),
                Some(cs.composure),
                Some(cs.trust),
                Some(cs.defense_budget),
            ),
            None => (None, None, None, None, None),
        };

        Self {
            session_id: s.session_id,
            case_id: s.case_id,
            status: s.status,
            current_turn: s.current_turn,
            active_character: s.active_character,
            phase,
            stress,
            composure,
            trust,
            defense_budget,
            discovered_facts: s
                .discovered_facts
                .into_iter()
                .map(|f| f.to_string())
                .collect(),
            discovered_evidence: s
                .discovered_evidence
                .into_iter()
                .map(|e| e.to_string())
                .collect(),
            conversation: s.conversation,
            accusations: s.accusations,
            revision: s.revision,
        }
    }
}

#[derive(Deserialize)]
pub struct CreateSessionRequest {
    pub case_id: CaseId,
    pub character_id: Option<CharacterId>,
}

#[derive(Serialize)]
pub struct CreateSessionResponse {
    pub session_id: SessionId,
    pub case_id: CaseId,
}

#[derive(Deserialize)]
pub struct ActionRequest {
    pub text: String,
    pub evidence_ids: Vec<EvidenceId>,
}

#[derive(Serialize)]
pub struct ActionResponse {
    pub session_id: SessionId,
    pub turn_id: TurnId,
    pub turn_number: u32,
    pub phase_before: Option<InterrogationPhase>,
    pub phase_after: Option<InterrogationPhase>,
    pub stress_delta: Option<i32>,
    pub composure_delta: Option<i32>,
    pub defense_delta: Option<i32>,
    pub trust_delta: Option<i32>,
    pub newly_revealed_disclosures: Vec<String>,
    pub newly_contradicted_claims: Vec<String>,
    pub utterance: String,
    pub dialogue_act: String,
}

// ── Error handling ────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct ApiError {
    pub error: String,
    pub detail: Option<String>,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let status = if self.error == "not_found" {
            StatusCode::NOT_FOUND
        } else if self.error == "conflict" {
            StatusCode::CONFLICT
        } else if self.error == "validation" {
            StatusCode::UNPROCESSABLE_ENTITY
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        };
        (status, Json(self)).into_response()
    }
}

fn internal_error(e: StorageError) -> ApiError {
    ApiError {
        error: "internal".into(),
        detail: Some(e.to_string()),
    }
}

fn parse_session_id(s: &str) -> Result<SessionId, ApiError> {
    Uuid::parse_str(s).map(SessionId).map_err(|_| ApiError {
        error: "not_found".into(),
        detail: Some(format!("Invalid session ID: {s}")),
    })
}

fn not_found(msg: impl Into<String>) -> ApiError {
    ApiError {
        error: "not_found".into(),
        detail: Some(msg.into()),
    }
}

fn conflict(msg: impl Into<String>) -> ApiError {
    ApiError {
        error: "conflict".into(),
        detail: Some(msg.into()),
    }
}

fn validation_error(msg: impl Into<String>) -> ApiError {
    ApiError {
        error: "validation".into(),
        detail: Some(msg.into()),
    }
}

// ── Handlers ──────────────────────────────────────────────────────────────

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".into(),
        version: "0.1.0".into(),
    })
}

async fn list_cases(
    State(state): State<Arc<RwLock<AppState>>>,
) -> Result<Json<Vec<CaseSummary>>, ApiError> {
    let app = state.read().await;
    let cases = app.repo.list_cases().map_err(internal_error)?;
    let summaries: Vec<CaseSummary> = cases
        .into_iter()
        .map(|c| CaseSummary {
            id: c.id,
            title: c.title,
            summary: c.summary,
            locale: c.locale,
            character_count: c.characters.len(),
            evidence_count: c.evidence.len(),
        })
        .collect();
    Ok(Json(summaries))
}

async fn get_case(
    State(state): State<Arc<RwLock<AppState>>>,
    Path(case_id): Path<String>,
) -> Result<Json<CaseDefinition>, ApiError> {
    let app = state.read().await;
    let case_id = CaseId::from(case_id.as_str());
    let case = app.repo.load_case(&case_id).map_err(|e| match e {
        StorageError::NotFound(_) => not_found(format!("Case {} not found", case_id)),
        _ => internal_error(e),
    })?;
    Ok(Json(case))
}

async fn validate_case(
    Json(case): Json<CaseDefinition>,
) -> Result<Json<serde_json::Value>, ApiError> {
    match case.validate() {
        Ok(()) => Ok(Json(serde_json::json!({"valid": true}))),
        Err(errors) => Ok(Json(serde_json::json!({
            "valid": false,
            "errors": errors.iter().map(|e| e.to_string()).collect::<Vec<_>>()
        }))),
    }
}

async fn create_session(
    State(state): State<Arc<RwLock<AppState>>>,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Json<CreateSessionResponse>, ApiError> {
    let app = state.read().await;

    let case = app.repo.load_case(&req.case_id).map_err(|e| match e {
        StorageError::NotFound(_) => not_found(format!("Case {} not found", req.case_id)),
        _ => internal_error(e),
    })?;

    let character_id = req.character_id.unwrap_or_else(|| {
        case.characters
            .first()
            .map(|c| c.id.clone())
            .unwrap_or_else(|| CharacterId::from("unknown"))
    });

    let character_def = case
        .characters
        .iter()
        .find(|c| c.id == character_id)
        .ok_or_else(|| not_found(format!("Character {character_id} not found in case")))?;

    let session_id = SessionId::new();
    let mut character_states = BTreeMap::new();
    character_states.insert(
        character_id.clone(),
        CharacterRuntimeState::new(character_def.resilience),
    );

    let session = SessionState {
        session_id,
        case_id: req.case_id.clone(),
        status: SessionStatus::Active,
        current_turn: 0,
        active_character: Some(character_id.clone()),
        discovered_facts: case
            .initial_player_knowledge
            .fact_ids
            .iter()
            .cloned()
            .collect(),
        discovered_evidence: case
            .initial_player_knowledge
            .evidence_ids
            .iter()
            .cloned()
            .collect(),
        character_states,
        conversation: Vec::new(),
        accusations: Vec::new(),
        revision: 0,
    };

    let sid = session_id;
    app.repo.create_session(&session).map_err(|e| match e {
        StorageError::Constraint(_) => conflict(e.to_string()),
        _ => internal_error(e),
    })?;

    let event = NarrativeEvent {
        event_id: Uuid::new_v4(),
        session_id: sid,
        turn_id: None,
        sequence: 0,
        event_type: NarrativeEventKind::SessionCreated,
        schema_version: 1,
        payload: serde_json::json!({
            "case_id": req.case_id.to_string(),
            "active_character": character_id.to_string(),
        }),
    };
    if let Err(e) = app.repo.append_events(&session_id, &[event]) {
        tracing::warn!("Failed to record session_created event: {e}");
    }

    Ok(Json(CreateSessionResponse {
        session_id,
        case_id: req.case_id,
    }))
}

async fn get_session(
    State(state): State<Arc<RwLock<AppState>>>,
    Path(session_id): Path<String>,
) -> Result<Json<SessionResponse>, ApiError> {
    let app = state.read().await;
    let sid = parse_session_id(&session_id)?;
    let session = app.repo.load_session(&sid).map_err(|e| match e {
        StorageError::NotFound(_) => not_found(format!("Session {session_id} not found")),
        _ => internal_error(e),
    })?;
    Ok(Json(SessionResponse::from(session)))
}

async fn get_session_events(
    State(state): State<Arc<RwLock<AppState>>>,
    Path(session_id): Path<String>,
) -> Result<Json<Vec<NarrativeEvent>>, ApiError> {
    let app = state.read().await;
    let sid = parse_session_id(&session_id)?;
    let events = app.repo.load_events(&sid).map_err(internal_error)?;
    Ok(Json(events))
}

async fn process_action(
    State(state): State<Arc<RwLock<AppState>>>,
    Path(session_id): Path<String>,
    Json(req): Json<ActionRequest>,
) -> Result<Json<ActionResponse>, ApiError> {
    let app = state.write().await;
    let sid = parse_session_id(&session_id)?;

    let session = app.repo.load_session(&sid).map_err(|e| match e {
        StorageError::NotFound(_) => not_found(format!("Session {session_id} not found")),
        _ => internal_error(e),
    })?;

    if session.status != SessionStatus::Active {
        return Err(validation_error("Session is not active"));
    }

    let case = app.repo.load_case(&session.case_id).map_err(|e| match e {
        StorageError::NotFound(_) => {
            internal_error(StorageError::NotFound("Case not found for session".into()))
        }
        _ => internal_error(e),
    })?;

    let evidence_map: BTreeMap<EvidenceId, EvidenceDefinition> = case
        .evidence
        .iter()
        .map(|e| (e.id.clone(), e.clone()))
        .collect();

    let facts: BTreeSet<_> = case.facts.iter().map(|f| f.id.clone()).collect();

    let character_id = session
        .active_character
        .as_ref()
        .ok_or_else(|| validation_error("No active character"))?
        .clone();

    let character_def = case
        .characters
        .iter()
        .find(|c| c.id == character_id)
        .ok_or_else(|| {
            internal_error(StorageError::NotFound("Character not found in case".into()))
        })?;

    let action = app.interpreter.interpret(&req.text, &req.evidence_ids);
    let engine_turn_id = TurnId::new();

    let mut state_clone = session
        .character_states
        .get(&character_id)
        .cloned()
        .ok_or_else(|| {
            internal_error(StorageError::NotFound("Character state not found".into()))
        })?;

    let result = app.engine.process(
        &action,
        &mut state_clone,
        character_def,
        &evidence_map,
        &facts,
        engine_turn_id,
    );

    let plan = app
        .planner
        .plan(&action, &state_clone, character_def, &evidence_map);
    let utterance = app.renderer.render(&plan);

    let phase_before = result.diff.phase_before;
    let phase_after = result.diff.phase_after;

    let entry_turn_id = TurnId::new();
    let mut updated_session = session;
    updated_session.current_turn += 1;
    updated_session
        .character_states
        .insert(character_id.clone(), state_clone);
    updated_session.conversation.push(DialogueEntry {
        turn_id: entry_turn_id,
        speaker: DialogueSpeaker::Player,
        text: req.text.clone(),
        attached_evidence: req.evidence_ids.clone(),
    });
    updated_session.conversation.push(DialogueEntry {
        turn_id: entry_turn_id,
        speaker: DialogueSpeaker::Character(character_id),
        text: utterance.utterance.clone(),
        attached_evidence: vec![],
    });

    let mut updated = updated_session.clone();
    updated.revision += 1;
    let events = vec![NarrativeEvent {
        event_id: Uuid::new_v4(),
        session_id: sid,
        turn_id: Some(entry_turn_id),
        sequence: updated_session.current_turn as u64,
        event_type: NarrativeEventKind::TurnProcessed,
        schema_version: 1,
        payload: serde_json::json!({
            "phase_before": format!("{:?}", phase_before),
            "phase_after": format!("{:?}", phase_after),
        }),
    }];

    app.repo.update_session(&updated).map_err(|e| match e {
        StorageError::RevisionConflict { .. } => {
            conflict("Session was modified by another request. Retry.")
        }
        _ => internal_error(e),
    })?;

    if let Err(e) = app.repo.append_events(&sid, &events) {
        tracing::warn!("Failed to record turn_processed event: {e}");
    }

    let stress_delta = result.diff.stress_after as i32 - result.diff.stress_before as i32;
    let composure_delta = result.diff.composure_after as i32 - result.diff.composure_before as i32;
    let defense_delta =
        result.diff.defense_budget_after as i32 - result.diff.defense_budget_before as i32;
    let trust_delta = result.diff.trust_after as i32 - result.diff.trust_before as i32;

    Ok(Json(ActionResponse {
        session_id: sid,
        turn_id: entry_turn_id,
        turn_number: updated_session.current_turn,
        phase_before: Some(phase_before),
        phase_after: Some(phase_after),
        stress_delta: Some(stress_delta),
        composure_delta: Some(composure_delta),
        defense_delta: Some(defense_delta),
        trust_delta: Some(trust_delta),
        newly_revealed_disclosures: result
            .diff
            .newly_revealed_disclosures
            .iter()
            .map(|d| d.to_string())
            .collect(),
        newly_contradicted_claims: result
            .contradictory_claims
            .iter()
            .map(|c| c.to_string())
            .collect(),
        utterance: utterance.utterance,
        dialogue_act: format!("{:?}", plan.act),
    }))
}

async fn make_accusation(
    State(state): State<Arc<RwLock<AppState>>>,
    Path(session_id): Path<String>,
    Json(req): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let app = state.write().await;
    let sid = parse_session_id(&session_id)?;

    let target_str = req
        .get("target")
        .and_then(|v| v.as_str())
        .ok_or_else(|| validation_error("Missing target"))?;
    let target = CharacterId::from(target_str);

    let evidence_ids: Vec<EvidenceId> = req
        .get("evidence_ids")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(EvidenceId::from))
                .collect()
        })
        .unwrap_or_default();

    let reasoning = req
        .get("reasoning")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let mut session = app.repo.load_session(&sid).map_err(|e| match e {
        StorageError::NotFound(_) => not_found(format!("Session {session_id} not found")),
        _ => internal_error(e),
    })?;

    let turn_id = TurnId::new();
    let turn_id_str = turn_id.to_string();
    let accusation = narrastate_core::session::Accusation {
        turn_id,
        target: target.clone(),
        evidence_ids: evidence_ids.clone(),
        reasoning,
        result: AccusationResult::WrongSuspect,
    };

    session.accusations.push(accusation);
    session.revision += 1;
    app.repo.update_session(&session).map_err(|e| match e {
        StorageError::RevisionConflict { .. } => conflict("Session was modified"),
        _ => internal_error(e),
    })?;

    let event = NarrativeEvent {
        event_id: Uuid::new_v4(),
        session_id: sid,
        turn_id: Some(TurnId::new()),
        sequence: session.revision,
        event_type: NarrativeEventKind::AccusationMade,
        schema_version: 1,
        payload: serde_json::json!({"target": target_str}),
    };
    if let Err(e) = app.repo.append_events(&sid, &[event]) {
        tracing::warn!("Failed to record accusation event: {e}");
    }

    Ok(Json(
        serde_json::json!({"status": "accusation_recorded", "turn_id": turn_id_str}),
    ))
}

async fn restart_session(
    State(state): State<Arc<RwLock<AppState>>>,
    Path(session_id): Path<String>,
) -> Result<Json<SessionResponse>, ApiError> {
    let app = state.read().await;
    let sid = parse_session_id(&session_id)?;

    let session = app.repo.load_session(&sid).map_err(|e| match e {
        StorageError::NotFound(_) => not_found(format!("Session {session_id} not found")),
        _ => internal_error(e),
    })?;

    let case_def = app.repo.load_case(&session.case_id).map_err(|e| match e {
        StorageError::NotFound(_) => {
            internal_error(StorageError::NotFound("Case not found".into()))
        }
        _ => internal_error(e),
    })?;

    let character_id = session.active_character.unwrap_or_else(|| {
        case_def
            .characters
            .first()
            .map(|c| c.id.clone())
            .unwrap_or_else(|| CharacterId::from("unknown"))
    });

    let resilience = case_def
        .characters
        .iter()
        .find(|c| c.id == character_id)
        .map(|c| c.resilience)
        .unwrap_or(5);

    let mut character_states = BTreeMap::new();
    character_states.insert(character_id.clone(), CharacterRuntimeState::new(resilience));

    let new_session = SessionState {
        session_id: session.session_id,
        case_id: session.case_id,
        status: SessionStatus::Active,
        current_turn: 0,
        active_character: Some(character_id),
        discovered_facts: case_def
            .initial_player_knowledge
            .fact_ids
            .iter()
            .cloned()
            .collect(),
        discovered_evidence: case_def
            .initial_player_knowledge
            .evidence_ids
            .iter()
            .cloned()
            .collect(),
        character_states,
        conversation: Vec::new(),
        accusations: Vec::new(),
        revision: 0,
    };

    if let Err(e) = app.repo.update_session(&new_session) {
        tracing::warn!("Failed to persist restarted session: {e}");
    }

    Ok(Json(SessionResponse::from(new_session)))
}
