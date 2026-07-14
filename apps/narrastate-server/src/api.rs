use std::collections::{BTreeMap, BTreeSet};
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Instant;

use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use narrastate_core::case::CaseDefinition;
use narrastate_core::character::CharacterRuntimeState;
use narrastate_core::evidence::{DiscoveryRule, EvidenceDefinition};
use narrastate_core::fact::{Fact, FactVisibility};
use narrastate_core::id::{CaseId, CharacterId, ClientActionId, EvidenceId, SessionId, TurnId};
use narrastate_core::session::{
    Accusation, AccusationResult, DialogueEntry, DialogueSpeaker, NarrativeEvent,
    NarrativeEventKind, NarrativeEventPayload, SessionMode, SessionState, SessionStatus,
};
use narrastate_core::transition::{InterpretedAction, PlayerIntent, PlayerTone, TransitionTuning};
use narrastate_provider::interpreter::LlmInterpreter;
use narrastate_provider::openai_compatible::OpenAiProvider;
use narrastate_provider::renderer::{LlmRenderer, RendererStatus};
use narrastate_runtime::evaluator::covered_elements;
use narrastate_runtime::mock::{MockInterpreter, MockRenderer};
use narrastate_runtime::ports::{
    ChatMessage, CommitOutcome, LlmCallMetadata, LlmConfig, LlmProvider, ProviderError,
    ProviderSettings, Repository, StorageError, TokenUsage,
};
use narrastate_runtime::{DialoguePlanner, TransitionEngine};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use uuid::Uuid;

pub struct AppState {
    pub repo: Arc<dyn Repository>,
    engine: TransitionEngine,
    planner: DialoguePlanner,
    mock_interpreter: MockInterpreter,
    mock_renderer: MockRenderer,
}

impl AppState {
    pub fn new(repo: Arc<dyn Repository>) -> Self {
        Self {
            repo,
            engine: TransitionEngine::new(TransitionTuning::default()),
            planner: DialoguePlanner,
            mock_interpreter: MockInterpreter,
            mock_renderer: MockRenderer,
        }
    }

    async fn llm_provider(&self) -> Result<(Arc<dyn LlmProvider>, ProviderSettings), ApiError> {
        let settings = self
            .repo
            .load_provider_settings()
            .await
            .map_err(ApiError::from_storage)?
            .unwrap_or(ProviderSettings {
                base_url: std::env::var("NARRASTATE_BASE_URL")
                    .unwrap_or_else(|_| "https://api.openai.com/v1".into()),
                model: std::env::var("NARRASTATE_MODEL").unwrap_or_else(|_| "gpt-4o-mini".into()),
            });
        let api_key = std::env::var("NARRASTATE_API_KEY")
            .or_else(|_| std::env::var("OPENAI_API_KEY"))
            .map_err(|_| {
                ApiError::validation("LLM mode requires NARRASTATE_API_KEY or OPENAI_API_KEY")
            })?;
        let provider = OpenAiProvider::new(LlmConfig {
            base_url: settings.base_url.clone(),
            model: settings.model.clone(),
            api_key,
            ..LlmConfig::default()
        })
        .map_err(|error| ApiError::internal(error.to_string()))?;
        Ok((Arc::new(provider), settings))
    }
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/v1/health", get(health))
        .route("/api/v1/config/public", get(public_config))
        .route("/api/v1/config/test-provider", post(test_provider))
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
            "/api/v1/sessions/{session_id}/debug",
            get(get_session_debug),
        )
        .route(
            "/api/v1/sessions/{session_id}/conclusion",
            get(get_conclusion),
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

#[derive(Debug, Serialize)]
pub struct ProblemDetails {
    #[serde(rename = "type")]
    kind: &'static str,
    title: &'static str,
    status: u16,
    detail: String,
}

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    title: &'static str,
    detail: String,
}

impl ApiError {
    fn validation(detail: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            title: "Validation failed",
            detail: detail.into(),
        }
    }
    fn not_found(detail: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            title: "Resource not found",
            detail: detail.into(),
        }
    }
    fn conflict(detail: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            title: "Revision conflict",
            detail: detail.into(),
        }
    }
    fn internal(detail: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            title: "Internal server error",
            detail: detail.into(),
        }
    }
    fn from_storage(error: StorageError) -> Self {
        match error {
            StorageError::NotFound(detail) => Self::not_found(detail),
            StorageError::RevisionConflict { expected, actual } => Self::conflict(format!(
                "expected revision {expected}, actual revision {actual}"
            )),
            StorageError::Constraint(detail) => Self::validation(detail),
            other => Self::internal(other.to_string()),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = ProblemDetails {
            kind: "about:blank",
            title: self.title,
            status: self.status.as_u16(),
            detail: self.detail,
        };
        let mut response = (self.status, Json(body)).into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/problem+json"),
        );
        response
    }
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
}

#[derive(Serialize)]
struct PublicConfig {
    configured: bool,
    base_url: String,
    model: String,
    api_key: &'static str,
}

#[derive(Deserialize)]
struct TestProviderRequest {
    base_url: String,
    model: String,
    api_key: Option<String>,
}

#[derive(Serialize)]
struct CaseSummary {
    id: CaseId,
    title: String,
    summary: String,
    locale: String,
    character_count: usize,
    evidence_count: usize,
}

#[derive(Serialize)]
struct PublicCharacter {
    id: CharacterId,
    name: String,
    role: String,
    public_profile: String,
}

#[derive(Debug, Clone, Serialize)]
struct PublicEvidence {
    id: EvidenceId,
    title: String,
    description: String,
}

#[derive(Serialize)]
struct PublicCase {
    id: CaseId,
    title: String,
    summary: String,
    locale: String,
    facts: Vec<Fact>,
    evidence: Vec<PublicEvidence>,
    characters: Vec<PublicCharacter>,
}

#[derive(Debug, Clone, Serialize)]
struct PublicSession {
    session_id: SessionId,
    case_id: CaseId,
    mode: SessionMode,
    status: SessionStatus,
    current_turn: u32,
    active_character: Option<CharacterId>,
    discovered_facts: Vec<Fact>,
    discovered_evidence: Vec<PublicEvidence>,
    conversation: Vec<DialogueEntry>,
    accusations: Vec<Accusation>,
    revision: u64,
}

#[derive(Debug, Serialize)]
struct DebugSessionResponse {
    character_states: BTreeMap<CharacterId, CharacterRuntimeState>,
    events: Vec<NarrativeEvent>,
    llm_calls: Vec<LlmCallMetadata>,
}

#[derive(Debug, Serialize)]
struct ConclusionResponse {
    result: AccusationResult,
    epilogue: String,
    truth_timeline: Vec<Fact>,
    decisive_evidence: Vec<PublicEvidence>,
    reasoning: String,
    confessed: bool,
    turn_count: u32,
}

#[derive(Deserialize)]
struct CreateSessionRequest {
    case_id: CaseId,
    mode: SessionMode,
    target_character_id: Option<CharacterId>,
}

#[derive(Serialize)]
struct PublicEvent {
    sequence: u64,
    turn_id: Option<TurnId>,
    event_type: NarrativeEventKind,
    schema_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ActionRequest {
    client_action_id: ClientActionId,
    expected_revision: u64,
    target_character_id: CharacterId,
    text: String,
    #[serde(default)]
    attached_evidence_ids: Vec<EvidenceId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PublicTurnResult {
    session_id: SessionId,
    turn_id: TurnId,
    revision: u64,
    utterance: String,
    degraded: bool,
}

#[derive(Deserialize)]
struct AccusationRequest {
    expected_revision: u64,
    target_character_id: CharacterId,
    #[serde(default)]
    evidence_ids: Vec<EvidenceId>,
    reasoning: String,
}

#[derive(Serialize)]
struct AccusationResponse {
    result: AccusationResult,
    session: PublicSession,
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: "0.1.0",
    })
}

async fn public_config(State(state): State<Arc<AppState>>) -> Result<Json<PublicConfig>, ApiError> {
    let settings = state
        .repo
        .load_provider_settings()
        .await
        .map_err(ApiError::from_storage)?;
    let configured = std::env::var("NARRASTATE_API_KEY")
        .or_else(|_| std::env::var("OPENAI_API_KEY"))
        .is_ok();
    let settings = settings.unwrap_or(ProviderSettings {
        base_url: std::env::var("NARRASTATE_BASE_URL")
            .unwrap_or_else(|_| "https://api.openai.com/v1".into()),
        model: std::env::var("NARRASTATE_MODEL").unwrap_or_else(|_| "gpt-4o-mini".into()),
    });
    Ok(Json(PublicConfig {
        configured,
        base_url: settings.base_url,
        model: settings.model,
        api_key: if configured { "********" } else { "" },
    }))
}

async fn test_provider(
    State(state): State<Arc<AppState>>,
    Json(request): Json<TestProviderRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if request.base_url.trim().is_empty() || request.model.trim().is_empty() {
        return Err(ApiError::validation("base_url and model are required"));
    }
    let api_key = request
        .api_key
        .or_else(|| std::env::var("NARRASTATE_API_KEY").ok())
        .or_else(|| std::env::var("OPENAI_API_KEY").ok())
        .ok_or_else(|| ApiError::validation("api_key is required for connectivity test"))?;
    let provider = OpenAiProvider::new(LlmConfig {
        base_url: request.base_url.clone(),
        model: request.model.clone(),
        api_key,
        timeout_secs: 10,
        max_retries: 0,
    })
    .map_err(|error| ApiError::internal(error.to_string()))?;
    provider
        .chat(&[ChatMessage::user("Reply with OK")])
        .await
        .map_err(|error| ApiError::validation(format!("provider test failed: {error}")))?;
    state
        .repo
        .save_provider_settings(&ProviderSettings {
            base_url: request.base_url,
            model: request.model,
        })
        .await
        .map_err(ApiError::from_storage)?;
    Ok(Json(serde_json::json!({"ok": true})))
}

async fn list_cases(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<CaseSummary>>, ApiError> {
    let cases = state
        .repo
        .list_cases()
        .await
        .map_err(ApiError::from_storage)?;
    Ok(Json(
        cases
            .into_iter()
            .map(|case| CaseSummary {
                id: case.id,
                title: case.title,
                summary: case.summary,
                locale: case.locale,
                character_count: case.characters.len(),
                evidence_count: case.evidence.len(),
            })
            .collect(),
    ))
}

async fn get_case(
    State(state): State<Arc<AppState>>,
    Path(case_id): Path<String>,
) -> Result<Json<PublicCase>, ApiError> {
    let case = state
        .repo
        .load_case(&CaseId::from(case_id))
        .await
        .map_err(ApiError::from_storage)?;
    Ok(Json(public_case(&case)))
}

async fn validate_case(Json(case): Json<CaseDefinition>) -> Json<serde_json::Value> {
    match case.validate() {
        Ok(()) => Json(serde_json::json!({"valid":true,"errors":[]})),
        Err(errors) => Json(
            serde_json::json!({"valid":false,"errors":errors.into_iter().map(|error| error.to_string()).collect::<Vec<_>>()}),
        ),
    }
}

async fn create_session(
    State(state): State<Arc<AppState>>,
    Json(request): Json<CreateSessionRequest>,
) -> Result<Json<PublicSession>, ApiError> {
    let case = state
        .repo
        .load_case(&request.case_id)
        .await
        .map_err(ApiError::from_storage)?;
    let session = new_session(&case, request.mode, request.target_character_id)?;
    let event = NarrativeEvent {
        event_id: Uuid::new_v4(),
        session_id: session.session_id,
        turn_id: None,
        sequence: 0,
        event_type: NarrativeEventKind::SessionCreated,
        schema_version: 1,
        payload: NarrativeEventPayload::SessionCreated {
            state: Box::new(session.clone()),
        },
    };
    state
        .repo
        .create_session(&session, &[event])
        .await
        .map_err(ApiError::from_storage)?;
    Ok(Json(public_session(&session, &case)))
}

async fn get_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<Json<PublicSession>, ApiError> {
    let session_id = parse_session_id(&session_id)?;
    let session = state
        .repo
        .recover_session(&session_id)
        .await
        .map_err(ApiError::from_storage)?;
    let case = state
        .repo
        .load_case(&session.case_id)
        .await
        .map_err(ApiError::from_storage)?;
    Ok(Json(public_session(&session, &case)))
}

async fn get_session_events(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<Json<Vec<PublicEvent>>, ApiError> {
    let session_id = parse_session_id(&session_id)?;
    state
        .repo
        .load_session(&session_id)
        .await
        .map_err(ApiError::from_storage)?;
    let events = state
        .repo
        .load_events(&session_id)
        .await
        .map_err(ApiError::from_storage)?;
    Ok(Json(
        events
            .into_iter()
            .map(|event| PublicEvent {
                sequence: event.sequence,
                turn_id: event.turn_id,
                event_type: event.event_type,
                schema_version: event.schema_version,
            })
            .collect(),
    ))
}

async fn get_session_debug(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<Json<DebugSessionResponse>, ApiError> {
    let session_id = parse_session_id(&session_id)?;
    let session = state
        .repo
        .recover_session(&session_id)
        .await
        .map_err(ApiError::from_storage)?;
    let events = state
        .repo
        .load_events(&session_id)
        .await
        .map_err(ApiError::from_storage)?;
    let llm_calls = state
        .repo
        .load_llm_calls(&session_id)
        .await
        .map_err(ApiError::from_storage)?;
    Ok(Json(DebugSessionResponse {
        character_states: session.character_states,
        events,
        llm_calls,
    }))
}

async fn get_conclusion(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<Json<ConclusionResponse>, ApiError> {
    let session_id = parse_session_id(&session_id)?;
    let session = state
        .repo
        .recover_session(&session_id)
        .await
        .map_err(ApiError::from_storage)?;
    if session.status != SessionStatus::Resolved {
        return Err(ApiError::validation("session has not been resolved"));
    }
    let case = state
        .repo
        .load_case(&session.case_id)
        .await
        .map_err(ApiError::from_storage)?;
    let accusation = session
        .accusations
        .last()
        .ok_or_else(|| ApiError::internal("resolved session has no accusation"))?;
    let selected: BTreeSet<_> = accusation.evidence_ids.iter().collect();
    let decisive_evidence = case
        .evidence
        .iter()
        .filter(|item| selected.contains(&item.id))
        .map(public_evidence)
        .collect();
    let confessed = accusation.result == AccusationResult::CaseProvenWithConfession;
    Ok(Json(ConclusionResponse {
        result: accusation.result.clone(),
        epilogue: case
            .ending
            .as_ref()
            .map(|ending| ending.epilogue.clone())
            .unwrap_or_else(|| "案件已经结束。".into()),
        truth_timeline: case
            .facts
            .iter()
            .filter(|fact| fact.truth == narrastate_core::fact::TruthValue::True)
            .cloned()
            .collect(),
        decisive_evidence,
        reasoning: accusation.reasoning.clone(),
        confessed,
        turn_count: session.current_turn,
    }))
}

async fn process_action(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(request): Json<ActionRequest>,
) -> Result<Response, ApiError> {
    let session_id = parse_session_id(&session_id)?;
    if request.text.chars().count() == 0 || request.text.chars().count() > 2000 {
        return Err(ApiError::validation(
            "text must contain 1..=2000 Unicode scalar values",
        ));
    }
    let (sender, receiver) = mpsc::channel(8);
    tokio::spawn(async move {
        send_sse(
            &sender,
            "turn.accepted",
            &serde_json::json!({"client_action_id":request.client_action_id}),
        )
        .await;
        send_sse(
            &sender,
            "turn.progress",
            &serde_json::json!({"stage":"processing"}),
        )
        .await;
        match execute_action(&state, session_id, request).await {
            Ok(result) => {
                send_sse(
                    &sender,
                    "dialogue.delta",
                    &serde_json::json!({"text":result.utterance}),
                )
                .await;
                send_sse(
                    &sender,
                    "state.public_changed",
                    &serde_json::json!({"revision":result.revision}),
                )
                .await;
                send_sse(&sender, "turn.completed", &result).await;
            }
            Err(error) => {
                send_sse(
                    &sender,
                    "turn.failed",
                    &ProblemDetails {
                        kind: "about:blank",
                        title: error.title,
                        status: error.status.as_u16(),
                        detail: error.detail,
                    },
                )
                .await
            }
        }
    });
    Ok(Sse::new(ReceiverStream::new(receiver))
        .keep_alive(KeepAlive::default())
        .into_response())
}

async fn execute_action(
    state: &AppState,
    session_id: SessionId,
    request: ActionRequest,
) -> Result<PublicTurnResult, ApiError> {
    if let Some(value) = state
        .repo
        .load_action_result(&session_id, &request.client_action_id)
        .await
        .map_err(ApiError::from_storage)?
    {
        return serde_json::from_value(value)
            .map_err(|error| ApiError::internal(format!("stored action result: {error}")));
    }
    let mut session = state
        .repo
        .recover_session(&session_id)
        .await
        .map_err(ApiError::from_storage)?;
    if session.status != SessionStatus::Active {
        return Err(ApiError::validation("session is not active"));
    }
    if session.revision != request.expected_revision {
        return Err(ApiError::conflict(format!(
            "expected revision {}, actual revision {}",
            request.expected_revision, session.revision
        )));
    }
    let case = state
        .repo
        .load_case(&session.case_id)
        .await
        .map_err(ApiError::from_storage)?;
    let character = case
        .characters
        .iter()
        .find(|character| character.id == request.target_character_id)
        .ok_or_else(|| ApiError::validation("target_character_id is not part of this case"))?;
    if let Some(id) = request
        .attached_evidence_ids
        .iter()
        .find(|id| !session.discovered_evidence.contains(id))
    {
        return Err(ApiError::validation(format!(
            "evidence {id} has not been discovered"
        )));
    }
    let evidence_map: BTreeMap<_, _> = case
        .evidence
        .iter()
        .cloned()
        .map(|item| (item.id.clone(), item))
        .collect();
    let known_evidence: Vec<_> = case
        .evidence
        .iter()
        .filter(|item| session.discovered_evidence.contains(&item.id))
        .cloned()
        .collect();
    let available_claims = character
        .claims
        .iter()
        .map(|claim| claim.id.clone())
        .collect::<Vec<_>>();
    let turn_id = TurnId::new();
    let mut degraded = false;
    let action = match session.mode {
        SessionMode::Mock => state
            .mock_interpreter
            .interpret(&request.text, &request.attached_evidence_ids),
        SessionMode::Llm => match state.llm_provider().await {
            Ok((provider, settings)) => {
                let started = Instant::now();
                match LlmInterpreter::new(provider)
                    .interpret_with_usage(
                        &request.text,
                        &request.attached_evidence_ids,
                        character,
                        &known_evidence,
                        &available_claims,
                    )
                    .await
                {
                    Ok((action, usage)) => {
                        record_llm_call(
                            state,
                            &session.session_id,
                            turn_id,
                            "interpreter",
                            &settings,
                            &request.text,
                            started,
                            usage,
                            "success",
                            None,
                        )
                        .await?;
                        action
                    }
                    Err(error) => {
                        record_llm_call(
                            state,
                            &session.session_id,
                            turn_id,
                            "interpreter",
                            &settings,
                            &request.text,
                            started,
                            TokenUsage::default(),
                            "failed",
                            Some(provider_error_code(&error)),
                        )
                        .await?;
                        degraded = true;
                        safe_fallback_action()
                    }
                }
            }
            Err(_) => {
                degraded = true;
                safe_fallback_action()
            }
        },
    };
    let character_state = session
        .character_states
        .get_mut(&character.id)
        .ok_or_else(|| ApiError::internal("character runtime state missing"))?;
    let transition = state.engine.process_with_requirements(
        &action,
        character_state,
        character,
        &evidence_map,
        &case.required_case_elements,
        turn_id,
    );
    let newly_revealed = transition.diff.newly_revealed_disclosures.first();
    let plan = state.planner.plan_with_context(
        &action,
        character_state,
        character,
        &evidence_map,
        &session.discovered_facts,
        newly_revealed,
    );
    let recent = session
        .conversation
        .iter()
        .map(|entry| (format!("{:?}", entry.speaker), entry.text.clone()))
        .chain(std::iter::once(("Player".into(), request.text.clone())))
        .collect::<Vec<_>>();
    let utterance = match session.mode {
        SessionMode::Mock => state.mock_renderer.render(&plan).utterance,
        SessionMode::Llm => match state.llm_provider().await {
            Ok((provider, settings)) => {
                let started = Instant::now();
                let (output, status, usage) = LlmRenderer::new(provider)
                    .render_validated_with_usage(&plan, character, &recent)
                    .await;
                record_llm_call(
                    state,
                    &session.session_id,
                    turn_id,
                    "renderer",
                    &settings,
                    &format!("{:?}:{:?}", plan.act, plan.allowed_facts),
                    started,
                    usage,
                    if status == RendererStatus::TemplateFallback {
                        "degraded"
                    } else {
                        "success"
                    },
                    (status == RendererStatus::TemplateFallback)
                        .then_some("validation_or_provider_failure"),
                )
                .await?;
                degraded |= status != RendererStatus::Model;
                output.utterance
            }
            Err(_) => {
                degraded = true;
                narrastate_provider::validator::OutputValidator::new()
                    .template_fallback(&plan)
                    .utterance
            }
        },
    };
    for disclosure in &transition.diff.newly_revealed_disclosures {
        if let Some(node) = character
            .disclosure_graph
            .nodes
            .iter()
            .find(|node| &node.id == disclosure)
        {
            session
                .discovered_facts
                .extend(node.reveals.iter().cloned());
        }
    }
    session.active_character = Some(character.id.clone());
    session.current_turn = session.current_turn.saturating_add(1);
    session.conversation.push(DialogueEntry {
        turn_id,
        speaker: DialogueSpeaker::Player,
        text: request.text.clone(),
        attached_evidence: request.attached_evidence_ids.clone(),
    });
    session.conversation.push(DialogueEntry {
        turn_id,
        speaker: DialogueSpeaker::Character(character.id.clone()),
        text: utterance.clone(),
        attached_evidence: Vec::new(),
    });
    if transition.diff.newly_revealed_disclosures.iter().any(|id| {
        character
            .disclosure_graph
            .confession_node()
            .is_some_and(|node| node.id == *id)
    }) {
        character_state
            .set_phase(narrastate_core::InterrogationPhase::Resolved, turn_id)
            .map_err(|error| ApiError::internal(error.to_string()))?;
        session.status = SessionStatus::Resolved;
    }
    session.revision = session.revision.saturating_add(1);
    let result = PublicTurnResult {
        session_id,
        turn_id,
        revision: session.revision,
        utterance,
        degraded,
    };
    let response =
        serde_json::to_value(&result).map_err(|error| ApiError::internal(error.to_string()))?;
    let events = turn_events(
        &session,
        &request,
        &action,
        &transition,
        &plan,
        turn_id,
        state
            .repo
            .load_events(&session_id)
            .await
            .map_err(ApiError::from_storage)?
            .len() as u64,
    );
    match state
        .repo
        .commit_turn(
            request.expected_revision,
            &request.client_action_id,
            &session,
            &events,
            &response,
        )
        .await
        .map_err(ApiError::from_storage)?
    {
        CommitOutcome::Committed => Ok(result),
        CommitOutcome::Idempotent(value) => serde_json::from_value(value)
            .map_err(|error| ApiError::internal(format!("stored action result: {error}"))),
    }
}

async fn make_accusation(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(request): Json<AccusationRequest>,
) -> Result<Json<AccusationResponse>, ApiError> {
    let session_id = parse_session_id(&session_id)?;
    let mut session = state
        .repo
        .recover_session(&session_id)
        .await
        .map_err(ApiError::from_storage)?;
    if session.status != SessionStatus::Active {
        return Err(ApiError::validation("session is not active"));
    }
    if session.revision != request.expected_revision {
        return Err(ApiError::conflict(format!(
            "expected revision {}, actual revision {}",
            request.expected_revision, session.revision
        )));
    }
    if request.reasoning.chars().count() > 4000 {
        return Err(ApiError::validation(
            "reasoning must be at most 4000 characters",
        ));
    }
    let case = state
        .repo
        .load_case(&session.case_id)
        .await
        .map_err(ApiError::from_storage)?;
    let target = case
        .characters
        .iter()
        .find(|character| character.id == request.target_character_id)
        .ok_or_else(|| ApiError::validation("target_character_id is not part of this case"))?;
    if let Some(id) = request
        .evidence_ids
        .iter()
        .find(|id| !session.discovered_evidence.contains(id))
    {
        return Err(ApiError::validation(format!(
            "evidence {id} has not been discovered"
        )));
    }
    let evidence_map: BTreeMap<_, _> = case
        .evidence
        .iter()
        .cloned()
        .map(|item| (item.id.clone(), item))
        .collect();
    let selected: BTreeSet<_> = request.evidence_ids.iter().cloned().collect();
    let coverage = covered_elements(&selected, &evidence_map);
    let confessed = target
        .disclosure_graph
        .confession_node()
        .is_some_and(|node| {
            session
                .character_states
                .get(&target.id)
                .is_some_and(|runtime| runtime.revealed_disclosures.contains(&node.id))
        });
    let result = if target.disclosure_graph.confession_node().is_none() {
        AccusationResult::WrongSuspect
    } else if !case.required_case_elements.is_subset(&coverage) {
        AccusationResult::CorrectButInsufficient
    } else if confessed {
        AccusationResult::CaseProvenWithConfession
    } else {
        AccusationResult::CaseProvenWithoutConfession
    };
    let turn_id = TurnId::new();
    session.accusations.push(Accusation {
        turn_id,
        target: target.id.clone(),
        evidence_ids: request.evidence_ids,
        reasoning: request.reasoning,
        result: result.clone(),
    });
    if matches!(
        result,
        AccusationResult::CaseProvenWithConfession | AccusationResult::CaseProvenWithoutConfession
    ) {
        session.status = SessionStatus::Resolved;
        if let Some(runtime) = session.character_states.get_mut(&target.id) {
            runtime
                .set_phase(narrastate_core::InterrogationPhase::Resolved, turn_id)
                .map_err(|error| ApiError::internal(error.to_string()))?;
        }
    }
    session.revision = session.revision.saturating_add(1);
    let sequence = state
        .repo
        .load_events(&session_id)
        .await
        .map_err(ApiError::from_storage)?
        .len() as u64;
    let mut events = vec![NarrativeEvent {
        event_id: Uuid::new_v4(),
        session_id,
        turn_id: Some(turn_id),
        sequence,
        event_type: NarrativeEventKind::AccusationSubmitted,
        schema_version: 1,
        payload: NarrativeEventPayload::AccusationSubmitted {
            state: Box::new(session.clone()),
        },
    }];
    if session.status == SessionStatus::Resolved {
        events.push(NarrativeEvent {
            event_id: Uuid::new_v4(),
            session_id,
            turn_id: Some(turn_id),
            sequence: sequence + 1,
            event_type: NarrativeEventKind::CaseResolved,
            schema_version: 1,
            payload: NarrativeEventPayload::CaseResolved {
                state: Box::new(session.clone()),
            },
        });
    }
    state
        .repo
        .commit_session(request.expected_revision, &session, &events)
        .await
        .map_err(ApiError::from_storage)?;
    Ok(Json(AccusationResponse {
        result,
        session: public_session(&session, &case),
    }))
}

async fn restart_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<Json<PublicSession>, ApiError> {
    let session_id = parse_session_id(&session_id)?;
    let old = state
        .repo
        .recover_session(&session_id)
        .await
        .map_err(ApiError::from_storage)?;
    let case = state
        .repo
        .load_case(&old.case_id)
        .await
        .map_err(ApiError::from_storage)?;
    let session = new_session(&case, old.mode, old.active_character)?;
    let event = NarrativeEvent {
        event_id: Uuid::new_v4(),
        session_id: session.session_id,
        turn_id: None,
        sequence: 0,
        event_type: NarrativeEventKind::SessionCreated,
        schema_version: 1,
        payload: NarrativeEventPayload::SessionCreated {
            state: Box::new(session.clone()),
        },
    };
    state
        .repo
        .create_session(&session, &[event])
        .await
        .map_err(ApiError::from_storage)?;
    Ok(Json(public_session(&session, &case)))
}

fn new_session(
    case: &CaseDefinition,
    mode: SessionMode,
    target: Option<CharacterId>,
) -> Result<SessionState, ApiError> {
    let active = target
        .or_else(|| {
            case.characters
                .first()
                .map(|character| character.id.clone())
        })
        .ok_or_else(|| ApiError::validation("case has no characters"))?;
    if !case
        .characters
        .iter()
        .any(|character| character.id == active)
    {
        return Err(ApiError::validation(
            "target_character_id is not part of this case",
        ));
    }
    let discovered_evidence = case
        .initial_player_knowledge
        .evidence_ids
        .iter()
        .cloned()
        .chain(
            case.evidence
                .iter()
                .filter(|item| {
                    item.discoverable_by
                        .iter()
                        .any(|rule| matches!(rule, DiscoveryRule::StartingEvidence))
                })
                .map(|item| item.id.clone()),
        )
        .collect();
    Ok(SessionState {
        session_id: SessionId::new(),
        case_id: case.id.clone(),
        mode,
        status: SessionStatus::Active,
        current_turn: 0,
        active_character: Some(active),
        discovered_facts: case
            .initial_player_knowledge
            .fact_ids
            .iter()
            .cloned()
            .collect(),
        discovered_evidence,
        character_states: case
            .characters
            .iter()
            .map(|character| {
                (
                    character.id.clone(),
                    CharacterRuntimeState::new(character.resilience),
                )
            })
            .collect(),
        conversation: Vec::new(),
        accusations: Vec::new(),
        revision: 0,
    })
}

fn public_case(case: &CaseDefinition) -> PublicCase {
    let evidence = case
        .evidence
        .iter()
        .filter(|item| {
            item.discoverable_by
                .iter()
                .any(|rule| matches!(rule, DiscoveryRule::StartingEvidence))
        })
        .map(public_evidence)
        .collect();
    PublicCase {
        id: case.id.clone(),
        title: case.title.clone(),
        summary: case.summary.clone(),
        locale: case.locale.clone(),
        facts: case
            .facts
            .iter()
            .filter(|fact| fact.visibility == FactVisibility::PublicAtStart)
            .cloned()
            .collect(),
        evidence,
        characters: case
            .characters
            .iter()
            .map(|character| PublicCharacter {
                id: character.id.clone(),
                name: character.name.clone(),
                role: character.role.clone(),
                public_profile: character.public_profile.clone(),
            })
            .collect(),
    }
}

fn public_session(session: &SessionState, case: &CaseDefinition) -> PublicSession {
    PublicSession {
        session_id: session.session_id,
        case_id: session.case_id.clone(),
        mode: session.mode,
        status: session.status,
        current_turn: session.current_turn,
        active_character: session.active_character.clone(),
        discovered_facts: case
            .facts
            .iter()
            .filter(|fact| session.discovered_facts.contains(&fact.id))
            .cloned()
            .collect(),
        discovered_evidence: case
            .evidence
            .iter()
            .filter(|item| session.discovered_evidence.contains(&item.id))
            .map(public_evidence)
            .collect(),
        conversation: session.conversation.clone(),
        accusations: session.accusations.clone(),
        revision: session.revision,
    }
}

fn public_evidence(item: &EvidenceDefinition) -> PublicEvidence {
    PublicEvidence {
        id: item.id.clone(),
        title: item.title.clone(),
        description: item.description.clone(),
    }
}

#[allow(clippy::too_many_arguments)]
async fn record_llm_call(
    state: &AppState,
    session_id: &SessionId,
    turn_id: TurnId,
    purpose: &str,
    settings: &ProviderSettings,
    prompt_material: &str,
    started: Instant,
    usage: TokenUsage,
    status: &str,
    error_code: Option<&str>,
) -> Result<(), ApiError> {
    let latency_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    state
        .repo
        .record_llm_call(&LlmCallMetadata {
            call_id: Uuid::new_v4().to_string(),
            session_id: *session_id,
            turn_id: Some(turn_id.to_string()),
            purpose: purpose.into(),
            provider: "openai-compatible".into(),
            model: settings.model.clone(),
            prompt_hash: format!("{:x}", Sha256::digest(prompt_material.as_bytes())),
            latency_ms,
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            status: status.into(),
            error_code: error_code.map(str::to_owned),
        })
        .await
        .map_err(ApiError::from_storage)
}

fn provider_error_code(error: &ProviderError) -> &'static str {
    match error {
        ProviderError::Unauthorized => "unauthorized",
        ProviderError::RateLimited => "rate_limited",
        ProviderError::Timeout => "timeout",
        ProviderError::Network(_) => "network",
        ProviderError::InvalidResponse(_) => "invalid_response",
        ProviderError::ContextTooLong => "context_too_long",
        ProviderError::SafetyRejected => "safety_rejected",
        ProviderError::Unknown(_) => "unknown",
    }
}

fn safe_fallback_action() -> InterpretedAction {
    InterpretedAction {
        intent: PlayerIntent::Ask,
        topics: vec!["unknown".into()],
        referenced_entities: Vec::new(),
        referenced_claims: Vec::new(),
        evidence_usage: Vec::new(),
        asserted_propositions: Vec::new(),
        tone: PlayerTone::Neutral,
        confidence: 0.0,
    }
}

fn parse_session_id(value: &str) -> Result<SessionId, ApiError> {
    Uuid::parse_str(value)
        .map(SessionId)
        .map_err(|_| ApiError::not_found("invalid session ID"))
}

async fn send_sse(
    sender: &mpsc::Sender<Result<Event, Infallible>>,
    name: &'static str,
    value: &impl Serialize,
) {
    if let Ok(data) = serde_json::to_string(value) {
        let _ = sender
            .send(Ok(Event::default().event(name).data(data)))
            .await;
    }
}

fn turn_events(
    session: &SessionState,
    request: &ActionRequest,
    action: &InterpretedAction,
    transition: &narrastate_runtime::TransitionResult,
    plan: &narrastate_runtime::DialoguePlan,
    turn_id: TurnId,
    start: u64,
) -> Vec<NarrativeEvent> {
    let mut events = Vec::new();
    let mut push = |kind, payload| {
        let sequence = start + events.len() as u64;
        events.push(NarrativeEvent {
            event_id: Uuid::new_v4(),
            session_id: session.session_id,
            turn_id: Some(turn_id),
            sequence,
            event_type: kind,
            schema_version: 1,
            payload,
        });
    };
    push(
        NarrativeEventKind::PlayerActionAccepted,
        NarrativeEventPayload::PlayerActionAccepted {
            client_action_id: request.client_action_id,
            target: request.target_character_id.clone(),
            attached_evidence: request.attached_evidence_ids.clone(),
        },
    );
    push(
        NarrativeEventKind::ActionInterpreted,
        NarrativeEventPayload::ActionInterpreted {
            action: action.clone(),
        },
    );
    if !request.attached_evidence_ids.is_empty() {
        push(
            NarrativeEventKind::EvidencePresented,
            NarrativeEventPayload::EvidencePresented {
                evidence_ids: request.attached_evidence_ids.clone(),
            },
        );
    }
    if !transition.contradictory_claims.is_empty() {
        push(
            NarrativeEventKind::ClaimContradicted,
            NarrativeEventPayload::ClaimContradicted {
                claim_ids: transition.contradictory_claims.clone(),
            },
        );
    }
    push(
        NarrativeEventKind::CharacterStateChanged,
        NarrativeEventPayload::CharacterStateChanged {
            character_id: request.target_character_id.clone(),
            reason: transition.transition_reason,
        },
    );
    if !transition.diff.newly_revealed_disclosures.is_empty() {
        push(
            NarrativeEventKind::DisclosureUnlocked,
            NarrativeEventPayload::DisclosureUnlocked {
                disclosure_ids: transition.diff.newly_revealed_disclosures.clone(),
            },
        );
    }
    push(
        NarrativeEventKind::DialoguePlanned,
        NarrativeEventPayload::DialoguePlanned { act: plan.act },
    );
    push(
        NarrativeEventKind::DialogueRendered,
        NarrativeEventPayload::DialogueRendered,
    );
    push(
        NarrativeEventKind::TurnCommitted,
        NarrativeEventPayload::TurnCommitted {
            client_action_id: request.client_action_id,
            state: Box::new(session.clone()),
        },
    );
    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use narrastate_storage::SqliteRepository;

    async fn fixture() -> (Arc<AppState>, SessionState, CaseDefinition) {
        let case: CaseDefinition =
            serde_json::from_str(include_str!("../../../cases/rain-gallery/case.json")).unwrap();
        let repository = Arc::new(SqliteRepository::new_in_memory().await.unwrap());
        repository.save_case(&case).await.unwrap();
        let session = new_session(&case, SessionMode::Mock, None).unwrap();
        repository
            .create_session(
                &session,
                &[NarrativeEvent {
                    event_id: Uuid::new_v4(),
                    session_id: session.session_id,
                    turn_id: None,
                    sequence: 0,
                    event_type: NarrativeEventKind::SessionCreated,
                    schema_version: 1,
                    payload: NarrativeEventPayload::SessionCreated {
                        state: Box::new(session.clone()),
                    },
                }],
            )
            .await
            .unwrap();
        (Arc::new(AppState::new(repository)), session, case)
    }

    fn action(
        revision: u64,
        client_action_id: ClientActionId,
        evidence: &[&str],
        text: &str,
    ) -> ActionRequest {
        ActionRequest {
            client_action_id,
            expected_revision: revision,
            target_character_id: CharacterId::from("luo-cheng"),
            text: text.into(),
            attached_evidence_ids: evidence.iter().map(|id| EvidenceId::from(*id)).collect(),
        }
    }

    #[tokio::test]
    async fn public_dtos_redact_world_truth_and_runtime_values() {
        let (_, session, case) = fixture().await;
        let case_json = serde_json::to_string(&public_case(&case)).unwrap();
        assert!(!case_json.contains("fact_painting_hidden"));
        assert!(!case_json.contains("disclosure_graph"));
        assert!(!case_json.contains("resilience"));

        let session_json = serde_json::to_string(&public_session(&session, &case)).unwrap();
        for forbidden in [
            "stress",
            "composure",
            "defense_budget",
            "revealed_disclosures",
        ] {
            assert!(!session_json.contains(forbidden));
        }
    }

    #[tokio::test]
    async fn action_retry_returns_stored_result_without_second_turn() {
        let (state, session, _) = fixture().await;
        let id = ClientActionId::new();
        let request = action(0, id, &["ev_card_log"], "门禁记录怎么解释？");
        let first = execute_action(&state, session.session_id, request.clone())
            .await
            .unwrap();
        let retry = execute_action(&state, session.session_id, request)
            .await
            .unwrap();
        assert_eq!(first.turn_id, retry.turn_id);
        assert_eq!(first.revision, 1);
        let stored = state.repo.load_session(&session.session_id).await.unwrap();
        assert_eq!(stored.current_turn, 1);
        assert_eq!(stored.revision, 1);
    }

    #[tokio::test]
    async fn sse_action_emits_contract_order() {
        let (state, session, _) = fixture().await;
        let response = process_action(
            State(state),
            Path(session.session_id.to_string()),
            Json(action(
                0,
                ClientActionId::new(),
                &["ev_card_log"],
                "门禁记录怎么解释？",
            )),
        )
        .await
        .unwrap();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body = String::from_utf8(bytes.to_vec()).unwrap();
        let names = [
            "event: turn.accepted",
            "event: turn.progress",
            "event: dialogue.delta",
            "event: state.public_changed",
            "event: turn.completed",
        ];
        let mut previous = 0;
        for name in names {
            let position = body
                .find(name)
                .unwrap_or_else(|| panic!("missing {name}: {body}"));
            assert!(position >= previous);
            previous = position;
        }
    }

    #[tokio::test]
    async fn demo_follows_d1_to_d6_without_skipping() {
        let (state, session, _) = fixture().await;
        let evidence = [
            "ev_card_log",
            "ev_sensor_cmd",
            "ev_mud_track",
            "ev_fiber",
            "ev_pawn_contact",
        ];
        for (index, evidence_id) in evidence.iter().enumerate() {
            execute_action(
                &state,
                session.session_id,
                action(
                    index as u64,
                    ClientActionId::new(),
                    &[*evidence_id],
                    "请解释这份证据",
                ),
            )
            .await
            .unwrap();
            let current = state.repo.load_session(&session.session_id).await.unwrap();
            let runtime = current
                .character_states
                .get(&CharacterId::from("luo-cheng"))
                .unwrap();
            assert_eq!(runtime.revealed_disclosures.len(), index + 1);
            assert!(!runtime
                .revealed_disclosures
                .contains(&narrastate_core::DisclosureId::from("d6_confession")));
        }
        execute_action(
            &state,
            session.session_id,
            action(5, ClientActionId::new(), &[], "是你做的，说明全部事实"),
        )
        .await
        .unwrap();
        let current = state.repo.load_session(&session.session_id).await.unwrap();
        assert_eq!(current.status, SessionStatus::Resolved);
        let runtime = current
            .character_states
            .get(&CharacterId::from("luo-cheng"))
            .unwrap();
        assert!(runtime
            .revealed_disclosures
            .contains(&narrastate_core::DisclosureId::from("d6_confession")));
    }

    #[tokio::test]
    async fn accusation_distinguishes_insufficient_and_proven_without_confession() {
        let (state, session, _) = fixture().await;
        let insufficient = make_accusation(
            State(state.clone()),
            Path(session.session_id.to_string()),
            Json(AccusationRequest {
                expected_revision: 0,
                target_character_id: CharacterId::from("luo-cheng"),
                evidence_ids: vec![EvidenceId::from("ev_card_log")],
                reasoning: "只有机会证据".into(),
            }),
        )
        .await
        .unwrap();
        assert_eq!(
            insufficient.0.result,
            AccusationResult::CorrectButInsufficient
        );
        assert_eq!(insufficient.0.session.status, SessionStatus::Active);

        let proven = make_accusation(
            State(state),
            Path(session.session_id.to_string()),
            Json(AccusationRequest {
                expected_revision: 1,
                target_character_id: CharacterId::from("luo-cheng"),
                evidence_ids: vec![
                    EvidenceId::from("ev_card_log"),
                    EvidenceId::from("ev_fiber"),
                    EvidenceId::from("ev_pawn_contact"),
                ],
                reasoning: "身份、机会、行为和意图均已闭合".into(),
            }),
        )
        .await
        .unwrap();
        assert_eq!(
            proven.0.result,
            AccusationResult::CaseProvenWithoutConfession
        );
        assert_eq!(proven.0.session.status, SessionStatus::Resolved);
    }

    #[tokio::test]
    async fn developer_endpoint_is_explicit_and_contains_trace_data() {
        let (state, session, _) = fixture().await;
        execute_action(
            &state,
            session.session_id,
            action(
                0,
                ClientActionId::new(),
                &["ev_card_log"],
                "门禁记录怎么解释？",
            ),
        )
        .await
        .unwrap();
        let debug = get_session_debug(State(state), Path(session.session_id.to_string()))
            .await
            .unwrap()
            .0;
        assert!(debug
            .character_states
            .contains_key(&CharacterId::from("luo-cheng")));
        assert!(debug
            .events
            .iter()
            .any(|event| event.event_type == NarrativeEventKind::ActionInterpreted));
        assert!(debug.llm_calls.is_empty());
    }

    #[tokio::test]
    async fn conclusion_is_hidden_until_resolved_then_returns_selected_evidence() {
        let (state, session, _) = fixture().await;
        let active_error =
            get_conclusion(State(state.clone()), Path(session.session_id.to_string()))
                .await
                .expect_err("active session must not expose truth");
        assert_eq!(active_error.status, StatusCode::UNPROCESSABLE_ENTITY);

        let _ = make_accusation(
            State(state.clone()),
            Path(session.session_id.to_string()),
            Json(AccusationRequest {
                expected_revision: 0,
                target_character_id: CharacterId::from("luo-cheng"),
                evidence_ids: vec![
                    EvidenceId::from("ev_card_log"),
                    EvidenceId::from("ev_fiber"),
                    EvidenceId::from("ev_pawn_contact"),
                ],
                reasoning: "完整证据链".into(),
            }),
        )
        .await
        .unwrap();
        let report = get_conclusion(State(state), Path(session.session_id.to_string()))
            .await
            .unwrap()
            .0;
        assert_eq!(report.result, AccusationResult::CaseProvenWithoutConfession);
        assert_eq!(report.decisive_evidence.len(), 3);
        assert!(!report.epilogue.is_empty());
        assert!(!report.truth_timeline.is_empty());
    }
}
