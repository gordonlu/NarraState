use async_trait::async_trait;
use narrastate_core::case::CaseDefinition;
use narrastate_core::id::{CaseId, ClientActionId, SessionId};
use narrastate_core::session::{NarrativeEvent, SessionState};
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
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.openai.com/v1".into(),
            model: "gpt-4o-mini".into(),
            api_key: String::new(),
            timeout_secs: 30,
            max_retries: 1,
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
    async fn record_llm_call(&self, call: &LlmCallMetadata) -> Result<(), StorageError>;
}
