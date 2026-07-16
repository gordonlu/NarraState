use async_trait::async_trait;
use narrastate_core::case::CaseDefinition;
use narrastate_core::id::{CaseId, ClientActionId, SessionId};
use narrastate_core::session::{NarrativeEvent, SessionState};
use narrastate_core::{
    CaseInstance, GeneratedCaseDraft, GeneratedVisualType, GenerationJobId,
    GenerationRepairRequest, GenerationRequest, GenerationStatus,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
pub enum ProviderError {
    #[error("Authentication failed")]
    Unauthorized,
    #[error("Rate limited")]
    RateLimited,
    #[error("Request timed out")]
    Timeout,
    #[error("Network error: {0}")]
    Network(String),
    #[error("Invalid response from model: {0}")]
    InvalidResponse(String),
    #[error("Model output was truncated before the structured response completed")]
    OutputTruncated,
    #[error("Context window exceeded")]
    ContextTooLong,
    #[error("Content was rejected by safety filters")]
    SafetyRejected,
    #[error("Unknown error: {0}")]
    Unknown(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    pub timeout_secs: u64,
    pub max_retries: u32,
    pub structured_output_max_tokens: u32,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.openai.com/v1".into(),
            model: "gpt-4o-mini".into(),
            api_key: String::new(),
            timeout_secs: 30,
            max_retries: 1,
            structured_output_max_tokens: 4_096,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

impl TokenUsage {
    pub fn combine(self, other: Self) -> Self {
        Self {
            input_tokens: combine_optional(self.input_tokens, other.input_tokens),
            output_tokens: combine_optional(self.output_tokens, other.output_tokens),
        }
    }
}

fn combine_optional(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.saturating_add(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderResponse<T> {
    pub output: T,
    pub usage: TokenUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    System,
    User,
    Assistant,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::System,
            content: content.into(),
        }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::User,
            content: content.into(),
        }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Assistant,
            content: content.into(),
        }
    }
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn chat(
        &self,
        messages: &[ChatMessage],
    ) -> Result<ProviderResponse<String>, ProviderError>;
    async fn chat_structured(
        &self,
        messages: &[ChatMessage],
        response_schema: &serde_json::Value,
    ) -> Result<ProviderResponse<serde_json::Value>, ProviderError>;
}

#[async_trait]
pub trait CaseGenerationProvider: Send + Sync {
    async fn generate_draft(
        &self,
        request: &GenerationRequest,
    ) -> Result<ProviderResponse<GeneratedCaseDraft>, ProviderError>;

    async fn repair_draft(
        &self,
        request: &GenerationRepairRequest,
    ) -> Result<ProviderResponse<GeneratedCaseDraft>, ProviderError>;
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GenerationProgressStage {
    Blueprint,
    SharedContent,
    Variants,
    Assembling,
    RepairingShared,
    RepairingVariants,
    RepairingFull,
    GeneratingVisuals,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct GenerationProgressUpdate {
    pub stage: GenerationProgressStage,
    pub completed: Option<u32>,
    pub total: Option<u32>,
}

#[async_trait]
pub trait GenerationProgressReporter: Send + Sync {
    async fn report(&self, update: GenerationProgressUpdate) -> Result<(), ProviderError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageGenerationRequest {
    pub visual_type: GeneratedVisualType,
    pub prompt: String,
    pub alt_text: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedImageAsset {
    pub mime_type: String,
    pub bytes: Vec<u8>,
}

#[async_trait]
pub trait ImageGenerationProvider: Send + Sync {
    async fn generate_image(
        &self,
        request: &ImageGenerationRequest,
    ) -> Result<GeneratedImageAsset, ProviderError>;
}

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
pub enum StorageError {
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Revision conflict: expected {expected}, got {actual}")]
    RevisionConflict { expected: u64, actual: u64 },
    #[error("Database error: {0}")]
    Database(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("Migration error: {0}")]
    Migration(String),
    #[error("Constraint violation: {0}")]
    Constraint(String),
    #[error("Internal error: {0}")]
    Internal(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSettings {
    pub base_url: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageProviderSettings {
    pub enabled: bool,
    pub base_url: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmCallMetadata {
    pub call_id: String,
    pub session_id: SessionId,
    pub turn_id: Option<String>,
    pub purpose: String,
    pub provider: String,
    pub model: String,
    pub prompt_hash: String,
    pub latency_ms: u64,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub status: String,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstalledCaseRecord {
    pub case_id: CaseId,
    pub case_version: String,
    pub source_path: String,
    pub schema_version: String,
    pub template_content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LegacyBackfillReport {
    pub migrated_sessions: usize,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationJobRecord {
    pub job_id: GenerationJobId,
    pub status: GenerationStatus,
    pub request_json: String,
    pub drafts_json: String,
    pub status_events_json: String,
    pub validation_report_json: Option<String>,
    pub result_path: Option<String>,
    pub attempt_count: u32,
    pub repair_count: u32,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub enum CommitOutcome {
    Committed,
    Idempotent(serde_json::Value),
}

#[async_trait]
pub trait Repository: Send + Sync {
    async fn create_session(
        &self,
        session: &SessionState,
        events: &[NarrativeEvent],
    ) -> Result<(), StorageError>;
    async fn load_session(&self, session_id: &SessionId) -> Result<SessionState, StorageError>;
    async fn recover_session(&self, session_id: &SessionId) -> Result<SessionState, StorageError>;
    async fn commit_turn(
        &self,
        expected_revision: u64,
        client_action_id: &ClientActionId,
        session: &SessionState,
        events: &[NarrativeEvent],
        response: &serde_json::Value,
    ) -> Result<CommitOutcome, StorageError>;
    async fn commit_session(
        &self,
        expected_revision: u64,
        session: &SessionState,
        events: &[NarrativeEvent],
    ) -> Result<(), StorageError>;
    async fn load_action_result(
        &self,
        session_id: &SessionId,
        client_action_id: &ClientActionId,
    ) -> Result<Option<serde_json::Value>, StorageError>;

    async fn save_case(&self, case: &CaseDefinition) -> Result<(), StorageError>;
    async fn load_case(&self, case_id: &CaseId) -> Result<CaseDefinition, StorageError>;
    async fn list_cases(&self) -> Result<Vec<CaseDefinition>, StorageError>;
    async fn install_case(&self, case: &InstalledCaseRecord) -> Result<(), StorageError>;
    async fn list_installed_cases(&self) -> Result<Vec<InstalledCaseRecord>, StorageError>;
    async fn save_case_instance(&self, instance: &CaseInstance) -> Result<(), StorageError>;
    async fn load_case_instance(
        &self,
        instance_id: &narrastate_core::CaseInstanceId,
    ) -> Result<CaseInstance, StorageError>;
    async fn backfill_legacy_session_instances(&self)
        -> Result<LegacyBackfillReport, StorageError>;
    async fn append_events(
        &self,
        session_id: &SessionId,
        events: &[NarrativeEvent],
    ) -> Result<(), StorageError>;
    async fn load_events(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<NarrativeEvent>, StorageError>;
    async fn save_snapshot(
        &self,
        session_id: &SessionId,
        revision: u64,
        state: &SessionState,
    ) -> Result<(), StorageError>;
    async fn load_latest_snapshot(
        &self,
        session_id: &SessionId,
    ) -> Result<Option<(u64, SessionState)>, StorageError>;
    async fn save_provider_settings(&self, settings: &ProviderSettings)
        -> Result<(), StorageError>;
    async fn load_provider_settings(&self) -> Result<Option<ProviderSettings>, StorageError>;
    async fn save_image_provider_settings(
        &self,
        settings: &ImageProviderSettings,
    ) -> Result<(), StorageError>;
    async fn load_image_provider_settings(
        &self,
    ) -> Result<Option<ImageProviderSettings>, StorageError>;
    async fn save_generation_job(&self, job: &GenerationJobRecord) -> Result<(), StorageError>;
    async fn load_generation_job(
        &self,
        job_id: &GenerationJobId,
    ) -> Result<GenerationJobRecord, StorageError>;
    async fn fail_interrupted_generation_jobs(&self) -> Result<u64, StorageError>;
    async fn record_llm_call(&self, call: &LlmCallMetadata) -> Result<(), StorageError>;
    async fn load_llm_calls(
        &self,
        session_id: &SessionId,
    ) -> Result<Vec<LlmCallMetadata>, StorageError>;
}
