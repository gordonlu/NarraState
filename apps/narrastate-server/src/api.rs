use std::collections::{BTreeMap, BTreeSet};
use std::convert::Infallible;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use narrastate_case::{
    adapt_v01, compile, default_visual_specs, freeze_case, generate_optional_visuals,
    install_inline_package, install_inline_package_with_visuals, load_case_package,
    run_generation_pipeline_with_id, select_variant, VariantCandidate,
};
use narrastate_core::case::CaseDefinition;
use narrastate_core::character::CharacterRuntimeState;
use narrastate_core::evidence::{DiscoveryRule, EvidenceDefinition};
use narrastate_core::fact::{Fact, FactVisibility};
use narrastate_core::id::{
    CaseId, CaseInstanceId, CharacterId, ClientActionId, EvidenceId, SessionId, TurnId, VariantId,
};
use narrastate_core::session::{
    Accusation, AccusationResult, DialogueEntry, DialogueSpeaker, NarrativeEvent,
    NarrativeEventKind, NarrativeEventPayload, SessionMode, SessionState, SessionStatus,
};
use narrastate_core::transition::{InterpretedAction, PlayerIntent, PlayerTone, TransitionTuning};
use narrastate_core::{
    CaseManifest, CaseTemplate, GeneratedVisualType, GenerationJobId, GenerationLimits,
    GenerationRequest, GenerationStatus, Seed, VariantSelection,
};
use narrastate_provider::case_generation::OpenAiCompatibleCaseGenerationProvider;
use narrastate_provider::image_generation::OpenAiCompatibleImageProvider;
use narrastate_provider::interpreter::LlmInterpreter;
use narrastate_provider::openai_compatible::OpenAiProvider;
use narrastate_provider::renderer::{LlmRenderer, RendererContext, RendererStatus};
use narrastate_runtime::evaluator::covered_elements;
use narrastate_runtime::mock::{MockInterpreter, MockRenderer};
use narrastate_runtime::ports::{
    CaseGenerationProvider, ChatMessage, CommitOutcome, GenerationJobRecord,
    GenerationProgressReporter, GenerationProgressStage, GenerationProgressUpdate,
    ImageGenerationProvider, ImageProviderSettings, InstalledCaseRecord, LlmCallMetadata,
    LlmConfig, LlmProvider, ProviderError, ProviderSettings, Repository, StorageError, TokenUsage,
};
use narrastate_runtime::{DialoguePlanner, TransitionEngine};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio_stream::wrappers::ReceiverStream;
use uuid::Uuid;

const DEFAULT_GENERATION_PROVIDER_TIMEOUT_SECS: u64 = 180;
const MIN_GENERATION_PROVIDER_TIMEOUT_SECS: u64 = 30;
const MAX_GENERATION_PROVIDER_TIMEOUT_SECS: u64 = 900;
const DEFAULT_GENERATION_OUTPUT_MAX_TOKENS: u32 = 65_536;
const MIN_GENERATION_OUTPUT_MAX_TOKENS: u32 = 4_096;
const MAX_GENERATION_OUTPUT_MAX_TOKENS: u32 = 65_536;

pub struct AppState {
    pub repo: Arc<dyn Repository>,
    engine: TransitionEngine,
    planner: DialoguePlanner,
    mock_interpreter: MockInterpreter,
    mock_renderer: MockRenderer,
    install_root: PathBuf,
    provider_env_path: PathBuf,
    ephemeral_api_key: RwLock<Option<String>>,
    image_provider_env_path: PathBuf,
    ephemeral_image_api_key: RwLock<Option<String>>,
    generation_provider_override: RwLock<Option<Arc<dyn CaseGenerationProvider>>>,
}

struct JobGenerationProgressReporter {
    repo: Arc<dyn Repository>,
    job_id: GenerationJobId,
    write_lock: Mutex<()>,
}

impl JobGenerationProgressReporter {
    fn new(repo: Arc<dyn Repository>, job_id: GenerationJobId) -> Self {
        Self {
            repo,
            job_id,
            write_lock: Mutex::new(()),
        }
    }
}

#[async_trait::async_trait]
impl GenerationProgressReporter for JobGenerationProgressReporter {
    async fn report(&self, update: GenerationProgressUpdate) -> Result<(), ProviderError> {
        let _guard = self.write_lock.lock().await;
        let mut record = self
            .repo
            .load_generation_job(&self.job_id)
            .await
            .map_err(|error| {
                ProviderError::Unknown(format!("generation progress load failed: {error}"))
            })?;
        if record.status.is_terminal() {
            return Err(ProviderError::Unknown(
                "generation progress cannot update a terminal job".into(),
            ));
        }
        let mut events = serde_json::from_str::<Vec<serde_json::Value>>(&record.status_events_json)
            .map_err(|error| {
                ProviderError::InvalidResponse(format!("generation progress events: {error}"))
            })?;
        let progress_event = serde_json::json!({
            "sequence": events.len(),
            "to": "drafting",
            "stage": update.stage,
            "completed": update.completed,
            "total": update.total,
        });
        let replaces_drafting_placeholder = update.stage == GenerationProgressStage::Blueprint
            && events.last().is_some_and(|event| {
                event.get("to").and_then(serde_json::Value::as_str) == Some("drafting")
                    && event.get("stage").is_none()
            });
        if replaces_drafting_placeholder {
            let mut progress_event = progress_event;
            let last_index = events.len().saturating_sub(1);
            progress_event["sequence"] = serde_json::json!(last_index);
            events[last_index] = progress_event;
        } else {
            events.push(progress_event);
        }
        record.status = GenerationStatus::Drafting;
        record.status_events_json = serde_json::to_string(&events).map_err(|error| {
            ProviderError::InvalidResponse(format!("generation progress events: {error}"))
        })?;
        record.updated_at = chrono::Utc::now().to_rfc3339();
        self.repo
            .save_generation_job(&record)
            .await
            .map_err(|error| {
                ProviderError::Unknown(format!("generation progress save failed: {error}"))
            })
    }
}

impl AppState {
    pub fn new(repo: Arc<dyn Repository>) -> Self {
        let provider_env_path = std::env::var("NARRASTATE_PROVIDER_ENV_FILE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("data/provider.env"));
        let persisted_api_key = match load_provider_api_key(&provider_env_path) {
            Ok(key) => key,
            Err(error) => {
                tracing::warn!(path = %provider_env_path.display(), %error, "provider env file could not be loaded");
                None
            }
        };
        let image_provider_env_path = std::env::var("NARRASTATE_IMAGE_PROVIDER_ENV_FILE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("data/image-provider.env"));
        let persisted_image_api_key = load_named_api_key(
            &image_provider_env_path,
            "NARRASTATE_IMAGE_API_KEY",
        )
        .unwrap_or_else(|error| {
            tracing::warn!(path = %image_provider_env_path.display(), %error, "image provider env file could not be loaded");
            None
        });
        Self {
            repo,
            engine: TransitionEngine::new(TransitionTuning::default()),
            planner: DialoguePlanner,
            mock_interpreter: MockInterpreter,
            mock_renderer: MockRenderer,
            install_root: std::env::var("NARRASTATE_CASE_INSTALL_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("data/installed-cases")),
            provider_env_path,
            ephemeral_api_key: RwLock::new(persisted_api_key),
            image_provider_env_path,
            ephemeral_image_api_key: RwLock::new(persisted_image_api_key),
            generation_provider_override: RwLock::new(None),
        }
    }

    #[cfg(test)]
    fn with_install_root(repo: Arc<dyn Repository>, install_root: PathBuf) -> Self {
        let mut state = Self::new(repo);
        state.install_root = install_root;
        state
    }

    async fn llm_provider(&self) -> Result<(Arc<dyn LlmProvider>, ProviderSettings), ApiError> {
        let defaults = LlmConfig::default();
        self.llm_provider_with_limits(defaults.timeout_secs, defaults.structured_output_max_tokens)
            .await
    }

    async fn generation_llm_provider(
        &self,
    ) -> Result<(Arc<dyn LlmProvider>, ProviderSettings), ApiError> {
        let timeout_secs = generation_provider_timeout_secs();
        let structured_output_max_tokens = generation_output_max_tokens();
        tracing::info!(
            timeout_secs,
            structured_output_max_tokens,
            "creating case-generation provider"
        );
        self.llm_provider_with_limits(timeout_secs, structured_output_max_tokens)
            .await
    }

    async fn llm_provider_with_limits(
        &self,
        timeout_secs: u64,
        structured_output_max_tokens: u32,
    ) -> Result<(Arc<dyn LlmProvider>, ProviderSettings), ApiError> {
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
        let api_key = self
            .api_key()
            .await
            .ok_or_else(|| ApiError::validation("LLM mode requires a configured API key"))?;
        let provider = OpenAiProvider::new(LlmConfig {
            base_url: settings.base_url.clone(),
            model: settings.model.clone(),
            api_key,
            timeout_secs,
            max_retries: LlmConfig::default().max_retries,
            structured_output_max_tokens,
        })
        .map_err(|error| ApiError::internal(error.to_string()))?;
        Ok((Arc::new(provider), settings))
    }

    async fn api_key(&self) -> Option<String> {
        if let Some(api_key) = provider_api_key() {
            return Some(api_key);
        }
        self.ephemeral_api_key.read().await.clone()
    }

    async fn image_provider(&self) -> Option<Arc<dyn ImageGenerationProvider>> {
        let settings = match self.repo.load_image_provider_settings().await {
            Ok(Some(settings)) => settings,
            Ok(None) => return None,
            Err(error) => {
                tracing::warn!(%error, "image provider settings could not be loaded; visuals disabled");
                return None;
            }
        };
        if !settings.enabled {
            return None;
        }
        let api_key = if let Some(key) = provider_key_from_environment("NARRASTATE_IMAGE_API_KEY") {
            key
        } else {
            self.ephemeral_image_api_key.read().await.clone()?
        };
        OpenAiCompatibleImageProvider::new(LlmConfig {
            base_url: settings.base_url,
            model: settings.model,
            api_key,
            ..LlmConfig::default()
        })
        .map(|provider| Arc::new(provider) as Arc<dyn ImageGenerationProvider>)
        .map_err(|error| tracing::warn!(%error, "image provider initialization failed"))
        .ok()
    }
}

fn generation_provider_timeout_secs() -> u64 {
    parse_generation_provider_timeout(
        std::env::var("NARRASTATE_GENERATION_TIMEOUT_SECS")
            .ok()
            .as_deref(),
    )
}

fn parse_generation_provider_timeout(value: Option<&str>) -> u64 {
    value
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| {
            (MIN_GENERATION_PROVIDER_TIMEOUT_SECS..=MAX_GENERATION_PROVIDER_TIMEOUT_SECS)
                .contains(value)
        })
        .unwrap_or(DEFAULT_GENERATION_PROVIDER_TIMEOUT_SECS)
}

fn generation_output_max_tokens() -> u32 {
    parse_generation_output_max_tokens(
        std::env::var("NARRASTATE_GENERATION_MAX_TOKENS")
            .ok()
            .as_deref(),
    )
}

fn parse_generation_output_max_tokens(value: Option<&str>) -> u32 {
    value
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|value| {
            (MIN_GENERATION_OUTPUT_MAX_TOKENS..=MAX_GENERATION_OUTPUT_MAX_TOKENS).contains(value)
        })
        .unwrap_or(DEFAULT_GENERATION_OUTPUT_MAX_TOKENS)
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/v1/health", get(health))
        .route("/api/v1/config/public", get(public_config))
        .route("/api/v1/config/provider", post(save_provider_config))
        .route(
            "/api/v1/config/image-provider",
            post(save_image_provider_config),
        )
        .route("/api/v1/config/test-provider", post(test_provider))
        .route("/api/v1/case-generation/jobs", post(create_generation_job))
        .route(
            "/api/v1/case-generation/jobs/{job_id}",
            get(get_generation_job),
        )
        .route(
            "/api/v1/case-generation/jobs/{job_id}/report",
            get(get_generation_report),
        )
        .route("/api/v1/cases", get(list_cases))
        .route("/api/v1/cases/{case_id}", get(get_case))
        .route(
            "/api/v1/cases/{case_id}/visuals/{visual_id}",
            get(get_case_visual),
        )
        .route("/api/v1/cases/validate", post(validate_case))
        .route("/api/v1/cases/install", post(install_case))
        .route("/api/v1/games", post(create_game))
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
    key_persisted: bool,
    base_url: String,
    model: String,
    api_key: &'static str,
    image_provider: PublicImageProviderConfig,
}

#[derive(Serialize)]
struct PublicImageProviderConfig {
    enabled: bool,
    configured: bool,
    key_persisted: bool,
    base_url: String,
    model: String,
}

#[derive(Deserialize)]
struct TestProviderRequest {
    base_url: String,
    model: String,
    api_key: Option<String>,
    #[serde(default)]
    persist_api_key: bool,
}

#[derive(Deserialize)]
struct SaveImageProviderRequest {
    enabled: bool,
    base_url: String,
    model: String,
    api_key: Option<String>,
    #[serde(default)]
    persist_api_key: bool,
}

#[derive(Clone, Serialize)]
struct CreateGenerationJobRequest {
    #[serde(flatten)]
    request: GenerationRequest,
    #[serde(default)]
    generate_visuals: bool,
}

impl<'de> Deserialize<'de> for CreateGenerationJobRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let mut value = serde_json::Value::deserialize(deserializer)?;
        let generate_visuals = value
            .as_object_mut()
            .and_then(|object| object.remove("generate_visuals"))
            .map(serde_json::from_value)
            .transpose()
            .map_err(serde::de::Error::custom)?
            .unwrap_or(false);
        let request = serde_json::from_value(value).map_err(serde::de::Error::custom)?;
        Ok(Self {
            request,
            generate_visuals,
        })
    }
}

#[derive(Serialize)]
struct CaseSummary {
    id: CaseId,
    title: String,
    summary: String,
    locale: String,
    character_count: usize,
    evidence_count: usize,
    cover_url: Option<String>,
}

#[derive(Serialize)]
struct PublicCharacter {
    id: CharacterId,
    name: String,
    role: String,
    public_profile: String,
    portrait_url: Option<String>,
}

#[derive(Serialize)]
struct PublicVisualAsset {
    id: narrastate_core::VisualAssetId,
    visual_type: GeneratedVisualType,
    url: String,
    alt_text: String,
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
    visual_assets: Vec<PublicVisualAsset>,
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

#[derive(Debug, Deserialize)]
struct CreateGameRequest {
    case_id: CaseId,
    case_version: Option<String>,
    variant_selection: VariantSelectionRequest,
    seed: Option<u64>,
    #[serde(default)]
    mode: SessionMode,
    target_character_id: Option<CharacterId>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "mode", rename_all = "lowercase")]
enum VariantSelectionRequest {
    Default,
    Random,
    Specific { variant_id: VariantId },
}

#[derive(Debug, Serialize)]
struct CreateGameResponse {
    session_id: SessionId,
    instance_id: CaseInstanceId,
    case_id: CaseId,
    case_version: String,
    seed: u64,
}

#[derive(Debug, Deserialize)]
struct InstallCaseRequest {
    manifest: CaseManifest,
    template: CaseTemplate,
    generation_report: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct InstallCaseResponse {
    case_id: CaseId,
    case_version: String,
    schema_version: String,
    variant_count: u32,
    template_content_hash: String,
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
    let configured = state.api_key().await.is_some();
    let settings = settings.unwrap_or(ProviderSettings {
        base_url: std::env::var("NARRASTATE_BASE_URL")
            .unwrap_or_else(|_| "https://api.openai.com/v1".into()),
        model: std::env::var("NARRASTATE_MODEL").unwrap_or_else(|_| "gpt-4o-mini".into()),
    });
    let image_settings = state
        .repo
        .load_image_provider_settings()
        .await
        .map_err(ApiError::from_storage)?
        .unwrap_or(ImageProviderSettings {
            enabled: false,
            base_url: String::new(),
            model: String::new(),
        });
    let image_configured = provider_key_from_environment("NARRASTATE_IMAGE_API_KEY").is_some()
        || state.ephemeral_image_api_key.read().await.is_some();
    Ok(Json(PublicConfig {
        configured,
        key_persisted: load_provider_api_key(&state.provider_env_path)
            .ok()
            .flatten()
            .is_some(),
        base_url: settings.base_url,
        model: settings.model,
        api_key: if configured { "********" } else { "" },
        image_provider: PublicImageProviderConfig {
            enabled: image_settings.enabled,
            configured: image_configured,
            key_persisted: load_named_api_key(
                &state.image_provider_env_path,
                "NARRASTATE_IMAGE_API_KEY",
            )
            .ok()
            .flatten()
            .is_some(),
            base_url: image_settings.base_url,
            model: image_settings.model,
        },
    }))
}

async fn save_image_provider_config(
    State(state): State<Arc<AppState>>,
    Json(request): Json<SaveImageProviderRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if request.enabled && (request.base_url.trim().is_empty() || request.model.trim().is_empty()) {
        return Err(ApiError::validation(
            "enabled image provider requires base_url and model",
        ));
    }
    state
        .repo
        .save_image_provider_settings(&ImageProviderSettings {
            enabled: request.enabled,
            base_url: request.base_url,
            model: request.model,
        })
        .await
        .map_err(ApiError::from_storage)?;
    if let Some(api_key) = request.api_key.and_then(non_empty_api_key) {
        if request.persist_api_key {
            persist_named_api_key(
                &state.image_provider_env_path,
                "NARRASTATE_IMAGE_API_KEY",
                &api_key,
            )
            .map_err(|error| ApiError::internal(format!("persist image provider key: {error}")))?;
        }
        *state.ephemeral_image_api_key.write().await = Some(api_key);
    }
    Ok(Json(serde_json::json!({"ok": true})))
}

async fn test_provider(
    State(state): State<Arc<AppState>>,
    Json(request): Json<TestProviderRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if request.base_url.trim().is_empty() || request.model.trim().is_empty() {
        return Err(ApiError::validation("base_url and model are required"));
    }
    let submitted_api_key = request.api_key.and_then(non_empty_api_key);
    let api_key = match submitted_api_key.clone() {
        Some(api_key) => api_key,
        None => state
            .api_key()
            .await
            .ok_or_else(|| ApiError::validation("api_key is required for connectivity test"))?,
    };
    let provider = OpenAiProvider::new(LlmConfig {
        base_url: request.base_url.clone(),
        model: request.model.clone(),
        api_key,
        timeout_secs: 10,
        max_retries: 0,
        structured_output_max_tokens: LlmConfig::default().structured_output_max_tokens,
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
    if let Some(api_key) = submitted_api_key {
        if request.persist_api_key {
            persist_provider_api_key(&state.provider_env_path, &api_key)
                .map_err(|error| ApiError::internal(format!("persist provider key: {error}")))?;
        }
        *state.ephemeral_api_key.write().await = Some(api_key);
    }
    Ok(Json(serde_json::json!({"ok": true})))
}

async fn save_provider_config(
    State(state): State<Arc<AppState>>,
    Json(request): Json<TestProviderRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if request.base_url.trim().is_empty() || request.model.trim().is_empty() {
        return Err(ApiError::validation("base_url and model are required"));
    }
    let submitted_api_key = request.api_key.and_then(non_empty_api_key);
    state
        .repo
        .save_provider_settings(&ProviderSettings {
            base_url: request.base_url,
            model: request.model,
        })
        .await
        .map_err(ApiError::from_storage)?;
    if let Some(api_key) = submitted_api_key {
        if request.persist_api_key {
            persist_provider_api_key(&state.provider_env_path, &api_key)
                .map_err(|error| ApiError::internal(format!("persist provider key: {error}")))?;
        }
        *state.ephemeral_api_key.write().await = Some(api_key);
    }
    Ok(Json(serde_json::json!({"ok": true})))
}

fn provider_api_key() -> Option<String> {
    provider_key_from_environment("NARRASTATE_API_KEY")
}

fn provider_key_from_environment(name: &str) -> Option<String> {
    std::env::var(name).ok().and_then(non_empty_api_key)
}

fn non_empty_api_key(value: String) -> Option<String> {
    (!value.trim().is_empty()).then_some(value)
}

fn load_provider_api_key(path: &std::path::Path) -> Result<Option<String>, std::io::Error> {
    load_named_api_key(path, "NARRASTATE_API_KEY")
}

fn load_named_api_key(
    path: &std::path::Path,
    variable_name: &str,
) -> Result<Option<String>, std::io::Error> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    for line in content.lines() {
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        if name.trim() == variable_name {
            return Ok(non_empty_api_key(value.trim().to_string()));
        }
    }
    Ok(None)
}

fn persist_provider_api_key(path: &std::path::Path, api_key: &str) -> Result<(), std::io::Error> {
    persist_named_api_key(path, "NARRASTATE_API_KEY", api_key)
}

fn persist_named_api_key(
    path: &std::path::Path,
    variable_name: &str,
    api_key: &str,
) -> Result<(), std::io::Error> {
    if api_key.contains(['\n', '\r']) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "API key may not contain line breaks",
        ));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension(format!("tmp-{}", Uuid::new_v4()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary)?;
    file.write_all(format!("{variable_name}={api_key}\n").as_bytes())?;
    file.sync_all()?;
    fs::rename(&temporary, path).inspect_err(|_| {
        let _ = fs::remove_file(&temporary);
    })?;
    Ok(())
}

async fn create_generation_job(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateGenerationJobRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let job_id = GenerationJobId::new();
    let now = chrono::Utc::now().to_rfc3339();
    let request_json =
        serde_json::to_string(&payload).map_err(|error| ApiError::internal(error.to_string()))?;
    let request = payload.request;
    let generate_visuals = payload.generate_visuals;
    let pending = GenerationJobRecord {
        job_id,
        status: GenerationStatus::Pending,
        request_json: request_json.clone(),
        drafts_json: "[]".into(),
        status_events_json: "[]".into(),
        validation_report_json: None,
        result_path: None,
        attempt_count: 0,
        repair_count: 0,
        error_code: None,
        error_message: None,
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    state
        .repo
        .save_generation_job(&pending)
        .await
        .map_err(ApiError::from_storage)?;

    let task_state = state.clone();
    tokio::spawn(async move {
        if let Err(error) = complete_generation_job(
            task_state.clone(),
            job_id,
            request,
            generate_visuals,
            request_json,
            now,
        )
        .await
        {
            tracing::error!(%job_id, detail = %error.detail, "generation background task failed");
            if let Err(storage_error) = fail_background_generation_job(
                &*task_state.repo,
                job_id,
                "GENERATION_BACKGROUND_FAILED",
                &error.detail,
            )
            .await
            {
                tracing::error!(%job_id, %storage_error, "generation background failure could not be persisted");
            }
        }
    });
    Ok(Json(generation_job_json(&pending)?))
}

async fn complete_generation_job(
    state: Arc<AppState>,
    job_id: GenerationJobId,
    request: GenerationRequest,
    generate_visuals: bool,
    request_json: String,
    now: String,
) -> Result<Json<serde_json::Value>, ApiError> {
    let mut drafting = state
        .repo
        .load_generation_job(&job_id)
        .await
        .map_err(ApiError::from_storage)?;
    drafting.status = GenerationStatus::Drafting;
    drafting.status_events_json = serde_json::json!([{
        "sequence": 0,
        "from": "pending",
        "to": "drafting"
    }])
    .to_string();
    drafting.updated_at = chrono::Utc::now().to_rfc3339();
    state
        .repo
        .save_generation_job(&drafting)
        .await
        .map_err(ApiError::from_storage)?;
    let override_provider = state.generation_provider_override.read().await.clone();
    let provider: Arc<dyn CaseGenerationProvider> = match override_provider {
        Some(provider) => provider,
        None => {
            let (llm, _) = match state.generation_llm_provider().await {
                Ok(provider) => provider,
                Err(error) => {
                    let record = GenerationJobRecord {
                        job_id,
                        status: GenerationStatus::Failed,
                        request_json,
                        drafts_json: "[]".into(),
                        status_events_json: "[]".into(),
                        validation_report_json: None,
                        result_path: None,
                        attempt_count: 0,
                        repair_count: 0,
                        error_code: Some("GENERATION_PROVIDER_NOT_CONFIGURED".into()),
                        error_message: Some(error.detail),
                        created_at: now,
                        updated_at: chrono::Utc::now().to_rfc3339(),
                    };
                    state
                        .repo
                        .save_generation_job(&record)
                        .await
                        .map_err(ApiError::from_storage)?;
                    return Ok(Json(generation_job_json(&record)?));
                }
            };
            Arc::new(
                OpenAiCompatibleCaseGenerationProvider::new(llm).with_progress_reporter(Arc::new(
                    JobGenerationProgressReporter::new(state.repo.clone(), job_id),
                )),
            )
        }
    };
    match run_generation_pipeline_with_id(
        provider.as_ref(),
        job_id,
        request.clone(),
        GenerationLimits::default(),
    )
    .await
    {
        Ok(success) => {
            let visuals = if generate_visuals {
                JobGenerationProgressReporter::new(state.repo.clone(), job_id)
                    .report(GenerationProgressUpdate {
                        stage: GenerationProgressStage::GeneratingVisuals,
                        completed: None,
                        total: None,
                    })
                    .await
                    .map_err(|error| ApiError::internal(error.to_string()))?;
                let visual_specs = default_visual_specs(&success.template, &request.setting);
                let image_provider = state.image_provider().await;
                generate_optional_visuals(image_provider.as_deref(), &visual_specs).await
            } else {
                narrastate_case::VisualGenerationReport::default()
            };
            let status_events_json = merge_generation_events(
                state.repo.as_ref(),
                &job_id,
                serde_json::to_value(&success.events)
                    .map_err(|error| ApiError::internal(error.to_string()))?,
            )
            .await?;
            let report = serde_json::json!({
                "generator_version": env!("CARGO_PKG_VERSION"),
                "generation_strategy": "staged-v1",
                "provider": "openai-compatible",
                "generated_at": chrono::Utc::now().to_rfc3339(),
                "request": request,
                "attempts": success.drafts.len(),
                "repairs": success.repairs,
                "validation": success.validation,
                "visuals": { "requested": generate_visuals, "generated": visuals.outputs.len(), "warnings": visuals.warnings },
            });
            let manifest = CaseManifest {
                id: success.template.id.clone(),
                version: success.template.version.clone(),
                schema_version: success.template.schema_version.clone(),
                title: success.template.title.clone(),
                language: success.template.locale.clone(),
                default_variant_id: success.template.default_variant_id.clone(),
                variant_count: success.template.solution_variants.len() as u32,
                generated: true,
                entry: "case.json".into(),
                assets: vec![],
                visual_assets: visuals
                    .outputs
                    .iter()
                    .map(|output| output.manifest.clone())
                    .collect(),
            };
            let installed = match install_inline_package_with_visuals(
                &manifest,
                &success.template,
                Some(&report),
                &visuals.outputs,
                &state.install_root,
            ) {
                Ok(installed) => installed,
                Err(error) => {
                    let record = GenerationJobRecord {
                        job_id,
                        status: GenerationStatus::Failed,
                        request_json,
                        drafts_json: serde_json::to_string(&success.drafts)
                            .map_err(|e| ApiError::internal(e.to_string()))?,
                        status_events_json: status_events_json.clone(),
                        validation_report_json: Some(report.to_string()),
                        result_path: None,
                        attempt_count: success.drafts.len() as u32,
                        repair_count: success.repairs,
                        error_code: Some("GENERATION_PACKAGE_INSTALL_FAILED".into()),
                        error_message: Some(error.to_string()),
                        created_at: now,
                        updated_at: chrono::Utc::now().to_rfc3339(),
                    };
                    state
                        .repo
                        .save_generation_job(&record)
                        .await
                        .map_err(ApiError::from_storage)?;
                    return Ok(Json(generation_job_json(&record)?));
                }
            };
            state
                .repo
                .install_case(&InstalledCaseRecord {
                    case_id: manifest.id,
                    case_version: manifest.version,
                    source_path: installed.root.to_string_lossy().into_owned(),
                    schema_version: manifest.schema_version,
                    template_content_hash: installed.template_content_hash.to_string(),
                })
                .await
                .map_err(ApiError::from_storage)?;
            let record = GenerationJobRecord {
                job_id,
                status: GenerationStatus::Completed,
                request_json,
                drafts_json: serde_json::to_string(&success.drafts)
                    .map_err(|error| ApiError::internal(error.to_string()))?,
                status_events_json,
                validation_report_json: Some(report.to_string()),
                result_path: Some(installed.root.to_string_lossy().into_owned()),
                attempt_count: success.drafts.len() as u32,
                repair_count: success.repairs,
                error_code: None,
                error_message: None,
                created_at: now,
                updated_at: chrono::Utc::now().to_rfc3339(),
            };
            state
                .repo
                .save_generation_job(&record)
                .await
                .map_err(ApiError::from_storage)?;
            Ok(Json(generation_job_json(&record)?))
        }
        Err(failure) => {
            let status_events_json = merge_generation_events(
                state.repo.as_ref(),
                &job_id,
                serde_json::to_value(&failure.events)
                    .map_err(|error| ApiError::internal(error.to_string()))?,
            )
            .await?;
            let record = GenerationJobRecord {
                job_id,
                status: GenerationStatus::Failed,
                request_json,
                drafts_json: serde_json::to_string(&failure.drafts)
                    .map_err(|e| ApiError::internal(e.to_string()))?,
                status_events_json,
                validation_report_json: Some(
                    serde_json::json!({"issues": failure.issues}).to_string(),
                ),
                result_path: None,
                attempt_count: failure.drafts.len() as u32,
                repair_count: failure.repairs,
                error_code: Some(failure.code),
                error_message: Some(failure.message),
                created_at: now,
                updated_at: chrono::Utc::now().to_rfc3339(),
            };
            state
                .repo
                .save_generation_job(&record)
                .await
                .map_err(ApiError::from_storage)?;
            Ok(Json(generation_job_json(&record)?))
        }
    }
}

async fn merge_generation_events(
    repo: &dyn Repository,
    job_id: &GenerationJobId,
    pipeline_events: serde_json::Value,
) -> Result<String, ApiError> {
    let record = repo
        .load_generation_job(job_id)
        .await
        .map_err(ApiError::from_storage)?;
    let mut events = serde_json::from_str::<Vec<serde_json::Value>>(&record.status_events_json)
        .map_err(|error| ApiError::internal(error.to_string()))?;
    let pipeline_events = pipeline_events
        .as_array()
        .ok_or_else(|| ApiError::internal("generation pipeline events must be an array"))?;
    for event in pipeline_events {
        if event.get("to").and_then(serde_json::Value::as_str) == Some("drafting") {
            continue;
        }
        let mut event = event.clone();
        event["sequence"] = serde_json::json!(events.len());
        events.push(event);
    }
    serde_json::to_string(&events).map_err(|error| ApiError::internal(error.to_string()))
}

async fn fail_background_generation_job(
    repo: &dyn Repository,
    job_id: GenerationJobId,
    code: &str,
    message: &str,
) -> Result<(), StorageError> {
    let mut record = repo.load_generation_job(&job_id).await?;
    if record.status.is_terminal() {
        return Ok(());
    }
    record.status = GenerationStatus::Failed;
    record.error_code = Some(code.into());
    record.error_message = Some(message.into());
    record.updated_at = chrono::Utc::now().to_rfc3339();
    repo.save_generation_job(&record).await
}

async fn get_generation_job(
    State(state): State<Arc<AppState>>,
    Path(job_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = GenerationJobId(
        Uuid::parse_str(&job_id).map_err(|_| ApiError::validation("invalid job id"))?,
    );
    let record = state
        .repo
        .load_generation_job(&id)
        .await
        .map_err(ApiError::from_storage)?;
    Ok(Json(generation_job_json(&record)?))
}

async fn get_generation_report(
    State(state): State<Arc<AppState>>,
    Path(job_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let id = GenerationJobId(
        Uuid::parse_str(&job_id).map_err(|_| ApiError::validation("invalid job id"))?,
    );
    let record = state
        .repo
        .load_generation_job(&id)
        .await
        .map_err(ApiError::from_storage)?;
    let report = record
        .validation_report_json
        .ok_or_else(|| ApiError::not_found("generation report not available"))?;
    Ok(Json(
        serde_json::from_str(&report).map_err(|e| ApiError::internal(e.to_string()))?,
    ))
}

fn generation_job_json(record: &GenerationJobRecord) -> Result<serde_json::Value, ApiError> {
    Ok(serde_json::json!({
        "job_id": record.job_id,
        "status": record.status,
        "attempt_count": record.attempt_count,
        "repair_count": record.repair_count,
        "error_code": record.error_code,
        "error_message": record.error_message,
        "result_path": record.result_path,
        "events": serde_json::from_str::<serde_json::Value>(&record.status_events_json).map_err(|e| ApiError::internal(e.to_string()))?,
        "updated_at": record.updated_at,
    }))
}

async fn list_cases(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<CaseSummary>>, ApiError> {
    let cases = state
        .repo
        .list_cases()
        .await
        .map_err(ApiError::from_storage)?;
    let installed = state
        .repo
        .list_installed_cases()
        .await
        .map_err(ApiError::from_storage)?;
    Ok(Json(
        cases
            .into_iter()
            .map(|case| {
                let cover_url = installed
                    .iter()
                    .filter(|record| record.case_id == case.id)
                    .filter_map(|record| match load_case_package(&record.source_path) {
                        Ok(package) => Some(package),
                        Err(error) => {
                            tracing::warn!(
                                path = %record.source_path,
                                %error,
                                "installed case package could not be loaded for cover"
                            );
                            None
                        }
                    })
                    .find_map(|package| {
                        package
                            .manifest
                            .visual_assets
                            .iter()
                            .find(|asset| asset.visual_type == GeneratedVisualType::CaseCover)
                            .map(|asset| format!("/api/v1/cases/{}/visuals/{}", case.id, asset.id))
                    });
                CaseSummary {
                    id: case.id,
                    title: case.title,
                    summary: case.summary,
                    locale: case.locale,
                    character_count: case.characters.len(),
                    evidence_count: case.evidence.len(),
                    cover_url,
                }
            })
            .collect(),
    ))
}

async fn get_case_visual(
    State(state): State<Arc<AppState>>,
    Path((case_id, visual_id)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let installed = state
        .repo
        .list_installed_cases()
        .await
        .map_err(ApiError::from_storage)?;
    for record in installed
        .into_iter()
        .filter(|record| record.case_id.as_ref() == case_id)
    {
        let package = load_case_package(&record.source_path)
            .map_err(|error| ApiError::internal(error.to_string()))?;
        if let Some(asset) = package
            .manifest
            .visual_assets
            .iter()
            .find(|asset| asset.id.as_ref() == visual_id)
        {
            let bytes = fs::read(package.root.join(&asset.path))
                .map_err(|error| ApiError::internal(error.to_string()))?;
            let content_type = if asset.path.ends_with(".webp") {
                "image/webp"
            } else if asset.path.ends_with(".jpg") || asset.path.ends_with(".jpeg") {
                "image/jpeg"
            } else {
                "image/png"
            };
            return Response::builder()
                .header(header::CONTENT_TYPE, content_type)
                .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
                .body(axum::body::Body::from(bytes))
                .map_err(|error| ApiError::internal(error.to_string()));
        }
    }
    Err(ApiError::not_found("visual asset not found"))
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
    let installed = state
        .repo
        .list_installed_cases()
        .await
        .map_err(ApiError::from_storage)?;
    let visual_assets = installed
        .iter()
        .filter(|record| record.case_id == case.id)
        .filter_map(|record| match load_case_package(&record.source_path) {
            Ok(package) => Some(package),
            Err(error) => {
                tracing::warn!(
                    path = %record.source_path,
                    %error,
                    "installed case package could not be loaded for portraits"
                );
                None
            }
        })
        .flat_map(|package| package.manifest.visual_assets.into_iter())
        .collect::<Vec<_>>();
    let portraits: BTreeMap<CharacterId, String> = visual_assets
        .iter()
        .filter(|asset| asset.visual_type == GeneratedVisualType::CharacterPortrait)
        .filter_map(|asset| {
            asset.id.as_ref().strip_prefix("character-").map(|id| {
                (
                    CharacterId::from(id),
                    format!("/api/v1/cases/{}/visuals/{}", case.id, asset.id),
                )
            })
        })
        .collect();
    let public_visuals = visual_assets
        .into_iter()
        .map(|asset| PublicVisualAsset {
            url: format!("/api/v1/cases/{}/visuals/{}", case.id, asset.id),
            id: asset.id,
            visual_type: asset.visual_type,
            alt_text: asset.alt_text,
        })
        .collect();
    Ok(Json(public_case_with_visuals(
        &case,
        &portraits,
        public_visuals,
    )))
}

async fn validate_case(Json(case): Json<CaseDefinition>) -> Json<serde_json::Value> {
    match case.validate() {
        Ok(()) => Json(serde_json::json!({"valid":true,"errors":[]})),
        Err(errors) => Json(
            serde_json::json!({"valid":false,"errors":errors.into_iter().map(|error| error.to_string()).collect::<Vec<_>>()}),
        ),
    }
}

async fn install_case(
    State(state): State<Arc<AppState>>,
    Json(request): Json<InstallCaseRequest>,
) -> Result<Json<InstallCaseResponse>, ApiError> {
    let package = install_inline_package(
        &request.manifest,
        &request.template,
        request.generation_report.as_ref(),
        &state.install_root,
    )
    .map_err(|error| ApiError::validation(error.to_string()))?;
    let default =
        compile(&package.template, &package.manifest.default_variant_id).map_err(|report| {
            ApiError::validation(
                report
                    .errors
                    .into_iter()
                    .map(|issue| format!("{} at {}: {}", issue.code, issue.path, issue.message))
                    .collect::<Vec<_>>()
                    .join("; "),
            )
        })?;
    state
        .repo
        .save_case(&default.definition)
        .await
        .map_err(ApiError::from_storage)?;
    let source_path = std::fs::canonicalize(&package.root)
        .map_err(|error| ApiError::internal(error.to_string()))?
        .to_string_lossy()
        .into_owned();
    state
        .repo
        .install_case(&InstalledCaseRecord {
            case_id: package.manifest.id.clone(),
            case_version: package.manifest.version.clone(),
            source_path,
            schema_version: package.manifest.schema_version.clone(),
            template_content_hash: package.template_content_hash.to_string(),
        })
        .await
        .map_err(ApiError::from_storage)?;
    Ok(Json(InstallCaseResponse {
        case_id: package.manifest.id,
        case_version: package.manifest.version,
        schema_version: package.manifest.schema_version,
        variant_count: package.manifest.variant_count,
        template_content_hash: package.template_content_hash.to_string(),
    }))
}

async fn create_game(
    State(state): State<Arc<AppState>>,
    Json(request): Json<CreateGameRequest>,
) -> Result<Json<CreateGameResponse>, ApiError> {
    let CreateGameRequest {
        case_id,
        case_version,
        variant_selection,
        seed,
        mode,
        target_character_id,
    } = request;
    let selection = match variant_selection {
        VariantSelectionRequest::Default => VariantSelection::Default,
        VariantSelectionRequest::Random => VariantSelection::Random,
        VariantSelectionRequest::Specific { variant_id } => {
            if !specific_variant_allowed() {
                return Err(ApiError::not_found(
                    "specific variant selection is disabled",
                ));
            }
            VariantSelection::Specific(variant_id)
        }
    };
    let seed = Seed(seed.unwrap_or_else(random_seed));
    let installed = state
        .repo
        .list_installed_cases()
        .await
        .map_err(ApiError::from_storage)?;
    let compiled = if let Some(record) =
        select_installed_case(&installed, &case_id, case_version.as_deref())
    {
        let package = load_case_package(&record.source_path)
            .map_err(|error| ApiError::validation(error.to_string()))?;
        if package.template_content_hash.as_ref() != record.template_content_hash {
            return Err(ApiError::internal(format!(
                "installed case {} {} content hash differs from its index",
                record.case_id, record.case_version
            )));
        }
        let candidates: Vec<_> = package
            .template
            .solution_variants
            .iter()
            .filter(|variant| variant.enabled)
            .filter(|variant| {
                package
                    .validation
                    .variant_reports
                    .iter()
                    .any(|report| report.variant_id == variant.id && report.valid)
            })
            .map(|variant| VariantCandidate {
                id: variant.id.clone(),
                weight: variant.weight,
            })
            .collect();
        let variant_id = select_variant(
            &package.template.id,
            &package.template.version,
            &package.template.default_variant_id,
            &selection,
            seed,
            &candidates,
        )
        .map_err(|error| ApiError::validation(error.to_string()))?;
        compile(&package.template, &variant_id)
    } else {
        if case_version
            .as_deref()
            .is_some_and(|version| version != "0.1.0")
        {
            return Err(ApiError::not_found(format!(
                "installed case {case_id} version {}",
                case_version.as_deref().unwrap_or_default()
            )));
        }
        let legacy = state
            .repo
            .load_case(&case_id)
            .await
            .map_err(ApiError::from_storage)?;
        let template = adapt_v01(legacy, "0.1.0", VariantId::from("classic"))
            .map_err(|error| ApiError::validation(error.to_string()))?;
        let candidates = vec![VariantCandidate {
            id: template.default_variant_id.clone(),
            weight: 1,
        }];
        let variant_id = select_variant(
            &template.id,
            &template.version,
            &template.default_variant_id,
            &selection,
            seed,
            &candidates,
        )
        .map_err(|error| ApiError::validation(error.to_string()))?;
        compile(&template, &variant_id)
    }
    .map_err(|report| {
        ApiError::validation(
            report
                .errors
                .into_iter()
                .map(|issue| format!("{} at {}: {}", issue.code, issue.path, issue.message))
                .collect::<Vec<_>>()
                .join("; "),
        )
    })?;
    let instance = freeze_case(compiled, seed);
    state
        .repo
        .save_case_instance(&instance)
        .await
        .map_err(ApiError::from_storage)?;
    let session = new_session(
        &instance.compiled_case.definition,
        mode,
        target_character_id,
        Some(instance.instance_id),
    )?;
    persist_new_session(&*state.repo, &session).await?;
    Ok(Json(CreateGameResponse {
        session_id: session.session_id,
        instance_id: instance.instance_id,
        case_id: instance.case_id,
        case_version: instance.case_version,
        seed: seed.0,
    }))
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
    let seed = Seed(random_seed());
    let template = adapt_v01(case.clone(), "0.1.0", VariantId::from("classic"))
        .map_err(|error| ApiError::validation(error.to_string()))?;
    let compiled = compile(&template, &template.default_variant_id).map_err(|report| {
        ApiError::validation(
            report
                .errors
                .into_iter()
                .map(|issue| format!("{} at {}: {}", issue.code, issue.path, issue.message))
                .collect::<Vec<_>>()
                .join("; "),
        )
    })?;
    let instance = freeze_case(compiled, seed);
    state
        .repo
        .save_case_instance(&instance)
        .await
        .map_err(ApiError::from_storage)?;
    let session = new_session(
        &case,
        request.mode,
        request.target_character_id,
        Some(instance.instance_id),
    )?;
    persist_new_session(&*state.repo, &session).await?;
    Ok(Json(public_session(&session, &case)))
}

async fn persist_new_session(
    repo: &dyn Repository,
    session: &SessionState,
) -> Result<(), ApiError> {
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
    repo.create_session(session, &[event])
        .await
        .map_err(ApiError::from_storage)?;
    Ok(())
}

async fn case_for_session(
    repo: &dyn Repository,
    session: &SessionState,
) -> Result<CaseDefinition, ApiError> {
    if let Some(instance_id) = session.instance_id {
        let instance = repo
            .load_case_instance(&instance_id)
            .await
            .map_err(ApiError::from_storage)?;
        if instance.case_id != session.case_id {
            return Err(ApiError::internal(format!(
                "session {} case ID differs from frozen instance {}",
                session.session_id, instance_id
            )));
        }
        Ok(instance.compiled_case.definition)
    } else {
        repo.load_case(&session.case_id)
            .await
            .map_err(ApiError::from_storage)
    }
}

fn select_installed_case<'a>(
    installed: &'a [InstalledCaseRecord],
    case_id: &CaseId,
    requested_version: Option<&str>,
) -> Option<&'a InstalledCaseRecord> {
    installed
        .iter()
        .filter(|record| &record.case_id == case_id)
        .filter(|record| requested_version.is_none_or(|version| record.case_version == version))
        .max_by_key(|record| version_key(&record.case_version))
}

fn version_key(version: &str) -> (u64, u64, u64) {
    let mut parts = version.split('.').map(|part| part.parse().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

fn random_seed() -> u64 {
    let uuid = Uuid::new_v4();
    u64::from_be_bytes(
        uuid.as_bytes()[..8]
            .try_into()
            .expect("UUID contains at least eight bytes"),
    )
}

fn specific_variant_allowed() -> bool {
    cfg!(debug_assertions)
        || std::env::var("NARRASTATE_DEVELOPER_MODE")
            .is_ok_and(|value| value.eq_ignore_ascii_case("true") || value == "1")
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
    let case = case_for_session(&*state.repo, &session).await?;
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
    let case = case_for_session(&*state.repo, &session).await?;
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
    let case = case_for_session(state.repo.as_ref(), &session).await?;
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
    if narrastate_runtime::is_visual_asset_question(&request.text) {
        return commit_visual_asset_question(state, session, request).await;
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
        .filter(|entry| entry.target_character_id.as_ref() == Some(&character.id))
        .map(|entry| {
            let speaker = match &entry.speaker {
                DialogueSpeaker::Player => "玩家".to_string(),
                DialogueSpeaker::System => "系统事件".to_string(),
                DialogueSpeaker::Character(id) => case
                    .characters
                    .iter()
                    .find(|candidate| &candidate.id == id)
                    .map(|candidate| candidate.name.clone())
                    .unwrap_or_else(|| "角色".to_string()),
            };
            (speaker, entry.text.clone())
        })
        .collect::<Vec<_>>();
    let utterance = match session.mode {
        SessionMode::Mock => state.mock_renderer.render(&plan).utterance,
        SessionMode::Llm => match state.llm_provider().await {
            Ok((provider, settings)) => {
                let started = Instant::now();
                let renderer_context = RendererContext {
                    locale: &case.locale,
                    facts: &case.facts,
                    recent_dialogue: &recent,
                    latest_player_message: &request.text,
                };
                let (output, status, usage) = LlmRenderer::new(provider)
                    .render_validated_with_usage(&plan, character, &renderer_context)
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
        target_character_id: Some(character.id.clone()),
        speaker: DialogueSpeaker::Player,
        text: request.text.clone(),
        attached_evidence: request.attached_evidence_ids.clone(),
    });
    session.conversation.push(DialogueEntry {
        turn_id,
        target_character_id: Some(character.id.clone()),
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

async fn commit_visual_asset_question(
    state: &AppState,
    mut session: SessionState,
    request: ActionRequest,
) -> Result<PublicTurnResult, ApiError> {
    let turn_id = TurnId::new();
    let utterance = narrastate_runtime::VISUAL_ASSET_DISCLAIMER.to_string();
    session.active_character = Some(request.target_character_id.clone());
    session.current_turn = session.current_turn.saturating_add(1);
    session.conversation.push(DialogueEntry {
        turn_id,
        target_character_id: Some(request.target_character_id.clone()),
        speaker: DialogueSpeaker::Player,
        text: request.text.clone(),
        attached_evidence: request.attached_evidence_ids.clone(),
    });
    session.conversation.push(DialogueEntry {
        turn_id,
        target_character_id: Some(request.target_character_id.clone()),
        speaker: DialogueSpeaker::System,
        text: utterance.clone(),
        attached_evidence: Vec::new(),
    });
    session.revision = session.revision.saturating_add(1);

    let result = PublicTurnResult {
        session_id: session.session_id,
        turn_id,
        revision: session.revision,
        utterance,
        degraded: false,
    };
    let response =
        serde_json::to_value(&result).map_err(|error| ApiError::internal(error.to_string()))?;
    let start = state
        .repo
        .load_events(&session.session_id)
        .await
        .map_err(ApiError::from_storage)?
        .len() as u64;
    let events = vec![
        NarrativeEvent {
            event_id: Uuid::new_v4(),
            session_id: session.session_id,
            turn_id: Some(turn_id),
            sequence: start,
            event_type: NarrativeEventKind::PlayerActionAccepted,
            schema_version: 1,
            payload: NarrativeEventPayload::PlayerActionAccepted {
                client_action_id: request.client_action_id,
                target: request.target_character_id.clone(),
                attached_evidence: request.attached_evidence_ids.clone(),
            },
        },
        NarrativeEvent {
            event_id: Uuid::new_v4(),
            session_id: session.session_id,
            turn_id: Some(turn_id),
            sequence: start.saturating_add(1),
            event_type: NarrativeEventKind::DialogueRendered,
            schema_version: 1,
            payload: NarrativeEventPayload::DialogueRendered,
        },
        NarrativeEvent {
            event_id: Uuid::new_v4(),
            session_id: session.session_id,
            turn_id: Some(turn_id),
            sequence: start.saturating_add(2),
            event_type: NarrativeEventKind::TurnCommitted,
            schema_version: 1,
            payload: NarrativeEventPayload::TurnCommitted {
                client_action_id: request.client_action_id,
                state: Box::new(session.clone()),
            },
        },
    ];
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
    let case = case_for_session(&*state.repo, &session).await?;
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
    let case = case_for_session(&*state.repo, &old).await?;
    let session = new_session(&case, old.mode, old.active_character, old.instance_id)?;
    persist_new_session(&*state.repo, &session).await?;
    Ok(Json(public_session(&session, &case)))
}

fn new_session(
    case: &CaseDefinition,
    mode: SessionMode,
    target: Option<CharacterId>,
    instance_id: Option<CaseInstanceId>,
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
        instance_id,
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

#[cfg(test)]
fn public_case(case: &CaseDefinition) -> PublicCase {
    public_case_with_visuals(case, &BTreeMap::new(), Vec::new())
}

fn public_case_with_visuals(
    case: &CaseDefinition,
    portraits: &BTreeMap<CharacterId, String>,
    visual_assets: Vec<PublicVisualAsset>,
) -> PublicCase {
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
                portrait_url: portraits.get(&character.id).cloned(),
            })
            .collect(),
        visual_assets,
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
        ProviderError::OutputTruncated => "output_truncated",
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
    use narrastate_case::load_case_package;
    use narrastate_storage::SqliteRepository;

    #[test]
    fn generation_provider_uses_a_long_but_bounded_timeout() {
        assert_eq!(parse_generation_provider_timeout(None), 180);
        assert_eq!(parse_generation_provider_timeout(Some("300")), 300);
        assert_eq!(parse_generation_provider_timeout(Some("29")), 180);
        assert_eq!(parse_generation_provider_timeout(Some("901")), 180);
        assert_eq!(parse_generation_provider_timeout(Some("invalid")), 180);
        assert_eq!(parse_generation_output_max_tokens(None), 65_536);
        assert_eq!(parse_generation_output_max_tokens(Some("16384")), 16_384);
        assert_eq!(parse_generation_output_max_tokens(Some("2048")), 65_536);
        assert_eq!(parse_generation_output_max_tokens(Some("65536")), 65_536);
        assert_eq!(parse_generation_output_max_tokens(Some("65537")), 65_536);
    }

    #[tokio::test]
    async fn staged_generation_progress_is_persisted_for_job_polling() {
        let repository = Arc::new(SqliteRepository::new_in_memory().await.unwrap());
        let job_id = GenerationJobId::new();
        let now = chrono::Utc::now().to_rfc3339();
        repository
            .save_generation_job(&GenerationJobRecord {
                job_id,
                status: GenerationStatus::Drafting,
                request_json: "{}".into(),
                drafts_json: "[]".into(),
                status_events_json: serde_json::json!([{
                    "sequence": 0,
                    "from": "pending",
                    "to": "drafting"
                }])
                .to_string(),
                validation_report_json: None,
                result_path: None,
                attempt_count: 0,
                repair_count: 0,
                error_code: None,
                error_message: None,
                created_at: now.clone(),
                updated_at: now,
            })
            .await
            .unwrap();
        let reporter = JobGenerationProgressReporter::new(repository.clone(), job_id);

        reporter
            .report(GenerationProgressUpdate {
                stage: GenerationProgressStage::Blueprint,
                completed: None,
                total: None,
            })
            .await
            .unwrap();
        reporter
            .report(GenerationProgressUpdate {
                stage: GenerationProgressStage::Variants,
                completed: Some(2),
                total: Some(3),
            })
            .await
            .unwrap();

        let stored = repository.load_generation_job(&job_id).await.unwrap();
        let events: serde_json::Value = serde_json::from_str(&stored.status_events_json).unwrap();
        assert_eq!(events.as_array().unwrap().len(), 2);
        assert_eq!(events[0]["stage"], "blueprint");
        assert_eq!(events[1]["stage"], "variants");
        assert_eq!(events[1]["completed"], 2);
        assert_eq!(events[1]["total"], 3);
    }

    async fn fixture() -> (Arc<AppState>, SessionState, CaseDefinition) {
        let case: CaseDefinition =
            serde_json::from_str(include_str!("../../../cases/rain-gallery/case.json")).unwrap();
        let repository = Arc::new(SqliteRepository::new_in_memory().await.unwrap());
        repository.save_case(&case).await.unwrap();
        let session = new_session(&case, SessionMode::Mock, None, None).unwrap();
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

    async fn game_fixture() -> Arc<AppState> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../cases/rain-gallery-variants");
        let package = load_case_package(&root).unwrap();
        let repository = Arc::new(SqliteRepository::new_in_memory().await.unwrap());
        let default = compile(&package.template, &package.manifest.default_variant_id).unwrap();
        repository.save_case(&default.definition).await.unwrap();
        repository
            .install_case(&InstalledCaseRecord {
                case_id: package.manifest.id,
                case_version: package.manifest.version,
                source_path: std::fs::canonicalize(root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                schema_version: package.manifest.schema_version,
                template_content_hash: package.template_content_hash.to_string(),
            })
            .await
            .unwrap();
        Arc::new(AppState::new(repository))
    }

    async fn isolated_config_fixture() -> (Arc<AppState>, PathBuf) {
        let root = std::env::temp_dir().join(format!("narrastate-config-{}", Uuid::new_v4()));
        let repository = Arc::new(SqliteRepository::new_in_memory().await.unwrap());
        let mut state = AppState::new(repository);
        state.provider_env_path = root.join("provider.env");
        state.image_provider_env_path = root.join("image-provider.env");
        state.ephemeral_api_key = RwLock::new(None);
        state.ephemeral_image_api_key = RwLock::new(None);
        (Arc::new(state), root)
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

    #[test]
    fn blank_api_key_is_not_treated_as_configured() {
        assert_eq!(non_empty_api_key(String::new()), None);
        assert_eq!(non_empty_api_key("   ".into()), None);
        assert_eq!(non_empty_api_key("sk-test".into()), Some("sk-test".into()));
    }

    #[test]
    fn provider_key_file_roundtrips_without_sqlite() {
        let root = std::env::temp_dir().join(format!("narrastate-provider-key-{}", Uuid::new_v4()));
        let path = root.join("provider.env");
        persist_provider_api_key(&path, "sk-deepseek=test").unwrap();
        assert_eq!(
            load_provider_api_key(&path).unwrap(),
            Some("sk-deepseek=test".into())
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn named_key_files_handle_missing_noise_and_unsafe_values() {
        let root = std::env::temp_dir().join(format!("narrastate-key-parser-{}", Uuid::new_v4()));
        let path = root.join("keys.env");
        assert_eq!(load_named_api_key(&path, "TARGET_KEY").unwrap(), None);

        fs::create_dir_all(&root).unwrap();
        fs::write(
            &path,
            "# comment\nMALFORMED\nOTHER_KEY=ignored\nTARGET_KEY = secret-value \n",
        )
        .unwrap();
        assert_eq!(
            load_named_api_key(&path, "TARGET_KEY").unwrap(),
            Some("secret-value".into())
        );
        assert_eq!(load_named_api_key(&path, "MISSING_KEY").unwrap(), None);

        let error = persist_named_api_key(&path, "TARGET_KEY", "unsafe\nvalue").unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn provider_configuration_persists_and_public_response_is_redacted() {
        let (state, root) = isolated_config_fixture().await;
        let initial = public_config(State(state.clone())).await.unwrap().0;
        assert!(!initial.configured);
        assert_eq!(initial.api_key, "");

        let invalid = save_provider_config(
            State(state.clone()),
            Json(TestProviderRequest {
                base_url: " ".into(),
                model: "model".into(),
                api_key: None,
                persist_api_key: false,
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(invalid.status, StatusCode::UNPROCESSABLE_ENTITY);

        let _ = save_provider_config(
            State(state.clone()),
            Json(TestProviderRequest {
                base_url: "https://provider.example/v1".into(),
                model: "text-model".into(),
                api_key: Some("sk-local-test".into()),
                persist_api_key: true,
            }),
        )
        .await
        .unwrap();
        let public = public_config(State(state.clone())).await.unwrap().0;
        assert!(public.configured);
        assert!(public.key_persisted);
        assert_eq!(public.api_key, "********");
        assert_eq!(public.model, "text-model");
        assert_eq!(
            load_provider_api_key(&state.provider_env_path).unwrap(),
            Some("sk-local-test".into())
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn image_provider_configuration_is_independent_and_validated() {
        let (state, root) = isolated_config_fixture().await;
        let invalid = save_image_provider_config(
            State(state.clone()),
            Json(SaveImageProviderRequest {
                enabled: true,
                base_url: String::new(),
                model: String::new(),
                api_key: None,
                persist_api_key: false,
            }),
        )
        .await
        .unwrap_err();
        assert_eq!(invalid.status, StatusCode::UNPROCESSABLE_ENTITY);

        let _ = save_image_provider_config(
            State(state.clone()),
            Json(SaveImageProviderRequest {
                enabled: true,
                base_url: "https://images.example/v1".into(),
                model: "image-model".into(),
                api_key: Some("image-key".into()),
                persist_api_key: true,
            }),
        )
        .await
        .unwrap();
        let public = public_config(State(state.clone())).await.unwrap().0;
        assert!(public.image_provider.enabled);
        assert!(public.image_provider.configured);
        assert!(public.image_provider.key_persisted);
        assert_eq!(public.image_provider.model, "image-model");
        assert_eq!(public.api_key, "");
        assert!(state.image_provider().await.is_some());
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn health_and_problem_details_follow_http_contract() {
        let response = health().await.0;
        assert_eq!(response.status, "ok");
        assert_eq!(response.version, "0.1.0");

        let mappings = [
            (
                StorageError::NotFound("missing".into()),
                StatusCode::NOT_FOUND,
            ),
            (
                StorageError::RevisionConflict {
                    expected: 2,
                    actual: 3,
                },
                StatusCode::CONFLICT,
            ),
            (
                StorageError::Constraint("invalid".into()),
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                StorageError::Database("offline".into()),
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
        ];
        for (storage_error, expected) in mappings {
            let response = ApiError::from_storage(storage_error).into_response();
            assert_eq!(response.status(), expected);
            assert_eq!(
                response.headers()[header::CONTENT_TYPE],
                "application/problem+json"
            );
        }
    }

    #[tokio::test]
    async fn generation_lookup_rejects_bad_ids_and_unavailable_reports() {
        let (state, _, _) = fixture().await;
        let invalid = get_generation_job(State(state.clone()), Path("not-a-uuid".into()))
            .await
            .unwrap_err();
        assert_eq!(invalid.status, StatusCode::UNPROCESSABLE_ENTITY);

        let id = GenerationJobId::new();
        let record = GenerationJobRecord {
            job_id: id,
            status: GenerationStatus::Pending,
            request_json: "{}".into(),
            drafts_json: "[]".into(),
            status_events_json: "[]".into(),
            validation_report_json: None,
            result_path: None,
            attempt_count: 0,
            repair_count: 0,
            error_code: None,
            error_message: None,
            created_at: "2026-07-16T00:00:00Z".into(),
            updated_at: "2026-07-16T00:00:00Z".into(),
        };
        state.repo.save_generation_job(&record).await.unwrap();
        let public = get_generation_job(State(state.clone()), Path(id.to_string()))
            .await
            .unwrap()
            .0;
        assert_eq!(public["status"], "pending");
        assert!(public.get("drafts").is_none());

        let missing = get_generation_report(State(state.clone()), Path(id.to_string()))
            .await
            .unwrap_err();
        assert_eq!(missing.status, StatusCode::NOT_FOUND);

        let mut malformed = record;
        malformed.validation_report_json = Some("not-json".into());
        state.repo.save_generation_job(&malformed).await.unwrap();
        let error = get_generation_report(State(state), Path(id.to_string()))
            .await
            .unwrap_err();
        assert_eq!(error.status, StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn public_case_and_session_read_endpoints_cover_valid_and_invalid_paths() {
        let (state, session, case) = fixture().await;
        let listed = list_cases(State(state.clone())).await.unwrap().0;
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, case.id);
        assert_eq!(listed[0].character_count, case.characters.len());

        let detail = get_case(State(state.clone()), Path(case.id.to_string()))
            .await
            .unwrap()
            .0;
        assert_eq!(detail.id, case.id);
        assert_eq!(detail.characters.len(), case.characters.len());

        let session_id = session.session_id.to_string();
        let public_session = get_session(State(state.clone()), Path(session_id.clone()))
            .await
            .unwrap()
            .0;
        assert_eq!(public_session.session_id, session.session_id);
        let events = get_session_events(State(state.clone()), Path(session_id.clone()))
            .await
            .unwrap()
            .0;
        assert_eq!(events.len(), 1);
        let debug = get_session_debug(State(state.clone()), Path(session_id))
            .await
            .unwrap()
            .0;
        assert_eq!(debug.events.len(), 1);

        let invalid = get_session(State(state), Path("bad-session-id".into()))
            .await
            .unwrap_err();
        assert_eq!(invalid.status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn case_validation_and_version_selection_are_deterministic() {
        let case: CaseDefinition =
            serde_json::from_str(include_str!("../../../cases/rain-gallery/case.json")).unwrap();
        assert_eq!(validate_case(Json(case.clone())).await.0["valid"], true);
        let mut invalid = case;
        invalid.characters.push(invalid.characters[0].clone());
        let report = validate_case(Json(invalid)).await.0;
        assert_eq!(report["valid"], false);
        assert!(!report["errors"].as_array().unwrap().is_empty());

        let case_id = CaseId::from("case");
        let records = vec![
            InstalledCaseRecord {
                case_id: case_id.clone(),
                case_version: "1.9.0".into(),
                source_path: "old".into(),
                schema_version: "0.2".into(),
                template_content_hash: "old".into(),
            },
            InstalledCaseRecord {
                case_id: case_id.clone(),
                case_version: "2.0.0".into(),
                source_path: "new".into(),
                schema_version: "0.2".into(),
                template_content_hash: "new".into(),
            },
        ];
        assert_eq!(
            select_installed_case(&records, &case_id, None)
                .unwrap()
                .case_version,
            "2.0.0"
        );
        assert_eq!(
            select_installed_case(&records, &case_id, Some("1.9.0"))
                .unwrap()
                .source_path,
            "old"
        );
        assert!(select_installed_case(&records, &CaseId::from("missing"), None).is_none());
        assert_eq!(version_key("broken.2"), (0, 2, 0));
        let _generated_seed = random_seed();
    }

    #[tokio::test]
    async fn create_game_adapts_legacy_case_and_freezes_single_variant() {
        let (state, _, _) = fixture().await;
        let response = create_game(
            State(state.clone()),
            Json(CreateGameRequest {
                case_id: CaseId::from("rain-gallery"),
                case_version: None,
                variant_selection: VariantSelectionRequest::Default,
                seed: Some(7),
                mode: SessionMode::Mock,
                target_character_id: None,
            }),
        )
        .await
        .unwrap()
        .0;

        assert_eq!(response.case_version, "0.1.0");
        assert_eq!(response.seed, 7);
        let session = state.repo.load_session(&response.session_id).await.unwrap();
        assert_eq!(session.instance_id, Some(response.instance_id));
        let instance = state
            .repo
            .load_case_instance(&response.instance_id)
            .await
            .unwrap();
        assert_eq!(instance.case_id, CaseId::from("rain-gallery"));
        assert_eq!(instance.variant_id, VariantId::from("classic"));
    }

    #[tokio::test]
    async fn create_game_freezes_default_variant_without_truth_leak() {
        let state = game_fixture().await;
        let response = create_game(
            State(state.clone()),
            Json(CreateGameRequest {
                case_id: CaseId::from("rain-gallery-variants"),
                case_version: Some("1.0.1".into()),
                variant_selection: VariantSelectionRequest::Default,
                seed: Some(42),
                mode: SessionMode::Mock,
                target_character_id: None,
            }),
        )
        .await
        .unwrap()
        .0;
        let public_json = serde_json::to_string(&response).unwrap();
        assert!(!public_json.contains("\"variant_id\""));
        assert!(!public_json.contains("\"responsible_character_id\""));
        assert_eq!(response.seed, 42);

        let session = state.repo.load_session(&response.session_id).await.unwrap();
        assert_eq!(session.instance_id, Some(response.instance_id));
        let instance = state
            .repo
            .load_case_instance(&response.instance_id)
            .await
            .unwrap();
        assert_eq!(instance.variant_id, VariantId::from("variant-luo"));
    }

    #[tokio::test]
    async fn random_game_selection_repeats_with_same_seed() {
        let state = game_fixture().await;
        let mut ids = Vec::new();
        for _ in 0..2 {
            let response = create_game(
                State(state.clone()),
                Json(CreateGameRequest {
                    case_id: CaseId::from("rain-gallery-variants"),
                    case_version: None,
                    variant_selection: VariantSelectionRequest::Random,
                    seed: Some(928_341),
                    mode: SessionMode::Mock,
                    target_character_id: None,
                }),
            )
            .await
            .unwrap()
            .0;
            let instance = state
                .repo
                .load_case_instance(&response.instance_id)
                .await
                .unwrap();
            ids.push((
                instance.variant_id,
                instance.compiled_content_hash,
                instance.instance_hash,
            ));
        }
        assert_eq!(ids[0], ids[1]);
    }

    #[tokio::test]
    async fn frozen_session_reads_instance_after_installed_case_row_changes() {
        let state = game_fixture().await;
        let response = create_game(
            State(state.clone()),
            Json(CreateGameRequest {
                case_id: CaseId::from("rain-gallery-variants"),
                case_version: None,
                variant_selection: VariantSelectionRequest::Specific {
                    variant_id: VariantId::from("variant-shen"),
                },
                seed: Some(7),
                mode: SessionMode::Mock,
                target_character_id: None,
            }),
        )
        .await
        .unwrap()
        .0;
        let session = state.repo.load_session(&response.session_id).await.unwrap();
        let frozen = case_for_session(&*state.repo, &session).await.unwrap();
        assert_eq!(
            frozen
                .characters
                .iter()
                .find(|character| character.disclosure_graph.confession_node().is_some())
                .unwrap()
                .id,
            CharacterId::from("shen-an")
        );

        let mut latest = frozen.clone();
        latest.title = "later installed content".into();
        state.repo.save_case(&latest).await.unwrap();
        let still_frozen = case_for_session(&*state.repo, &session).await.unwrap();
        assert_ne!(still_frozen.title, latest.title);
    }

    #[tokio::test]
    async fn legacy_session_endpoint_also_creates_frozen_instance() {
        let case: CaseDefinition =
            serde_json::from_str(include_str!("../../../cases/rain-gallery/case.json")).unwrap();
        let repository = Arc::new(SqliteRepository::new_in_memory().await.unwrap());
        repository.save_case(&case).await.unwrap();
        let state = Arc::new(AppState::new(repository));
        let response = create_session(
            State(state.clone()),
            Json(CreateSessionRequest {
                case_id: case.id,
                mode: SessionMode::Mock,
                target_character_id: None,
            }),
        )
        .await
        .unwrap()
        .0;
        let session = state.repo.load_session(&response.session_id).await.unwrap();
        assert!(session.instance_id.is_some());
        state
            .repo
            .load_case_instance(&session.instance_id.unwrap())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn install_api_accepts_content_not_server_path_and_indexes_only_after_validation() {
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../cases/rain-gallery-variants");
        let package = load_case_package(source).unwrap();
        let install_root =
            std::env::temp_dir().join(format!("narrastate-install-{}", Uuid::new_v4()));
        let repository = Arc::new(SqliteRepository::new_in_memory().await.unwrap());
        let state = Arc::new(AppState::with_install_root(
            repository,
            install_root.clone(),
        ));

        let response = install_case(
            State(state.clone()),
            Json(InstallCaseRequest {
                manifest: package.manifest,
                template: package.template,
                generation_report: None,
            }),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(response.variant_count, 3);
        assert_eq!(state.repo.list_installed_cases().await.unwrap().len(), 1);
        assert!(install_root
            .join("rain-gallery-variants/1.0.1/manifest.json")
            .is_file());
        let json = serde_json::to_string(&response).unwrap();
        assert!(!json.contains("responsible_character"));
        assert!(!json.contains("solution_variants"));
        std::fs::remove_dir_all(install_root).ok();
    }

    #[tokio::test]
    async fn generation_api_completes_with_mock_and_persists_report_without_live_api() {
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../cases/rain-gallery-variants");
        let package = load_case_package(source).unwrap();
        let request = GenerationRequest {
            theme: "画廊失窃".into(),
            setting: "现代画廊".into(),
            tone: narrastate_core::NarrativeTone::Realistic,
            target_duration_minutes: 45,
            difficulty: narrastate_core::Difficulty::Medium,
            character_count: 3,
            variant_count: 3,
            realism: narrastate_core::RealismLevel::Grounded,
            confession_policy: narrastate_core::ConfessionPolicy::PartialThenFull,
            content_constraints: vec![],
            language: "zh-CN".into(),
        };
        let draft = narrastate_case::draft_from_template(request.clone(), &package.template);
        let root = std::env::temp_dir().join(format!("narrastate-generation-{}", Uuid::new_v4()));
        let repo = Arc::new(SqliteRepository::new_in_memory().await.unwrap());
        let state = Arc::new(AppState::with_install_root(repo, root.clone()));
        *state.generation_provider_override.write().await = Some(Arc::new(
            narrastate_runtime::mock::MockCaseGenerationProvider::new(vec![Ok(draft)]),
        ));

        let pending = create_generation_job(
            State(state.clone()),
            Json(CreateGenerationJobRequest {
                request,
                generate_visuals: false,
            }),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(pending["status"], "pending");
        let id = GenerationJobId(Uuid::parse_str(pending["job_id"].as_str().unwrap()).unwrap());
        let response = loop {
            let stored = state.repo.load_generation_job(&id).await.unwrap();
            if stored.status.is_terminal() {
                break generation_job_json(&stored).unwrap();
            }
            tokio::task::yield_now().await;
        };
        assert_eq!(response["status"], "completed");
        let public = response.to_string();
        assert!(!public.contains("responsible_character"));
        assert!(!public.contains("solution_variants"));
        assert!(!public.contains("drafts"));
        let stored = state.repo.load_generation_job(&id).await.unwrap();
        assert_eq!(stored.status, GenerationStatus::Completed);
        assert!(stored.validation_report_json.is_some());
        assert!(root.join("rain-gallery-variants/1.0.1/case.json").is_file());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn generation_api_persists_provider_failure_without_truth_leak() {
        let repo = Arc::new(SqliteRepository::new_in_memory().await.unwrap());
        let state = Arc::new(AppState::new(repo));
        *state.generation_provider_override.write().await = Some(Arc::new(
            narrastate_runtime::mock::MockCaseGenerationProvider::new(vec![Err(
                ProviderError::Timeout,
            )]),
        ));
        let request = GenerationRequest {
            theme: "港口失踪".into(),
            setting: "现代港区".into(),
            tone: narrastate_core::NarrativeTone::Realistic,
            target_duration_minutes: 45,
            difficulty: narrastate_core::Difficulty::Medium,
            character_count: 4,
            variant_count: 3,
            realism: narrastate_core::RealismLevel::Grounded,
            confession_policy: narrastate_core::ConfessionPolicy::PartialThenFull,
            content_constraints: vec![],
            language: "zh-CN".into(),
        };
        let pending = create_generation_job(
            State(state.clone()),
            Json(CreateGenerationJobRequest {
                request,
                generate_visuals: false,
            }),
        )
        .await
        .unwrap()
        .0;
        let id = GenerationJobId(Uuid::parse_str(pending["job_id"].as_str().unwrap()).unwrap());
        let failed = loop {
            let record = state.repo.load_generation_job(&id).await.unwrap();
            if record.status.is_terminal() {
                break generation_job_json(&record).unwrap();
            }
            tokio::task::yield_now().await;
        };
        assert_eq!(failed["status"], "failed");
        assert_eq!(failed["error_code"], "GENERATION_PROVIDER_TIMEOUT");
        let public = failed.to_string();
        assert!(!public.contains("drafts"));
        assert!(!public.contains("responsible_character"));
        assert!(!public.contains("solution_variants"));
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
    async fn visual_detail_question_cannot_change_authoritative_case_state() {
        let (state, session, _) = fixture().await;
        let before_characters = serde_json::to_value(&session.character_states).unwrap();
        let before_facts = session.discovered_facts.clone();
        let before_evidence = session.discovered_evidence.clone();
        let before_status = session.status;

        let result = execute_action(
            &state,
            session.session_id,
            action(
                0,
                ClientActionId::new(),
                &["ev_card_log"],
                "背景图里那辆车是不是嫌疑人的？",
            ),
        )
        .await
        .unwrap();

        assert_eq!(
            result.utterance,
            narrastate_runtime::VISUAL_ASSET_DISCLAIMER
        );
        assert!(!result.degraded);
        let recovered = state
            .repo
            .recover_session(&session.session_id)
            .await
            .unwrap();
        assert_eq!(recovered.current_turn, 1);
        assert_eq!(recovered.revision, 1);
        assert_eq!(
            serde_json::to_value(&recovered.character_states).unwrap(),
            before_characters
        );
        assert_eq!(recovered.discovered_facts, before_facts);
        assert_eq!(recovered.discovered_evidence, before_evidence);
        assert_eq!(recovered.status, before_status);
        assert!(matches!(
            recovered.conversation.last().map(|entry| &entry.speaker),
            Some(DialogueSpeaker::System)
        ));

        let events = state.repo.load_events(&session.session_id).await.unwrap();
        assert!(!events.iter().any(|event| matches!(
            event.event_type,
            NarrativeEventKind::ActionInterpreted
                | NarrativeEventKind::EvidencePresented
                | NarrativeEventKind::CharacterStateChanged
                | NarrativeEventKind::DisclosureUnlocked
        )));
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
