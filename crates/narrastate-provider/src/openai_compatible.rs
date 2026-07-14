use async_trait::async_trait;
use narrastate_runtime::ports::{
    ChatMessage, LlmConfig, LlmProvider, ProviderError, ProviderResponse, TokenUsage,
};

pub struct OpenAiProvider {
    config: LlmConfig,
    client: reqwest::Client,
}

impl OpenAiProvider {
    pub fn new(config: LlmConfig) -> Result<Self, ProviderError> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .map_err(|error| ProviderError::Unknown(format!("HTTP client: {error}")))?;
        Ok(Self { config, client })
    }

    async fn send(&self, body: serde_json::Value) -> Result<serde_json::Value, ProviderError> {
        let url = format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        );
        let mut attempt = 0;
        loop {
            let result = self
                .client
                .post(&url)
                .bearer_auth(&self.config.api_key)
                .json(&body)
                .send()
                .await;
            match result {
                Ok(response) => {
                    let status = response.status();
                    let value: serde_json::Value = response.json().await.map_err(|error| {
                        ProviderError::InvalidResponse(format!("response JSON: {error}"))
                    })?;
                    if status.is_success() {
                        return Ok(value);
                    }
                    let message = value
                        .pointer("/error/message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown provider error");
                    let error = match status.as_u16() {
                        401 | 403 => ProviderError::Unauthorized,
                        408 => ProviderError::Timeout,
                        429 => ProviderError::RateLimited,
                        400 if message.to_ascii_lowercase().contains("context") => {
                            ProviderError::ContextTooLong
                        }
                        400 if message.to_ascii_lowercase().contains("safety") => {
                            ProviderError::SafetyRejected
                        }
                        500..=599 => ProviderError::Network(message.into()),
                        _ => ProviderError::Unknown(message.into()),
                    };
                    if attempt < self.config.max_retries
                        && matches!(
                            error,
                            ProviderError::RateLimited | ProviderError::Network(_)
                        )
                    {
                        attempt += 1;
                        continue;
                    }
                    return Err(error);
                }
                Err(error) => {
                    let classified = if error.is_timeout() {
                        ProviderError::Timeout
                    } else if error.is_connect() {
                        ProviderError::Network(error.to_string())
                    } else {
                        ProviderError::Unknown(error.to_string())
                    };
                    if attempt < self.config.max_retries
                        && matches!(classified, ProviderError::Network(_))
                    {
                        attempt += 1;
                        continue;
                    }
                    return Err(classified);
                }
            }
        }
    }
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    async fn chat(
        &self,
        messages: &[ChatMessage],
    ) -> Result<ProviderResponse<String>, ProviderError> {
        let value = self
            .send(serde_json::json!({
                "model": self.config.model,
                "messages": messages,
                "temperature": 0.7,
                "max_tokens": 2048
            }))
            .await?;
        let output = value
            .pointer("/choices/0/message/content")
            .and_then(|content| content.as_str())
            .map(ToOwned::to_owned)
            .ok_or_else(|| {
                ProviderError::InvalidResponse("missing choices[0].message.content".into())
            })?;
        Ok(ProviderResponse {
            output,
            usage: token_usage(&value),
        })
    }

    async fn chat_structured(
        &self,
        messages: &[ChatMessage],
        response_schema: &serde_json::Value,
    ) -> Result<ProviderResponse<serde_json::Value>, ProviderError> {
        let value = self.send(serde_json::json!({
            "model": self.config.model,
            "messages": messages,
            "temperature": 0.3,
            "max_tokens": 4096,
            "response_format": {"type": "json_schema", "json_schema": {"name": "structured_output", "schema": response_schema, "strict": true}}
        })).await?;
        let content = value
            .pointer("/choices/0/message/content")
            .and_then(|content| content.as_str())
            .ok_or_else(|| {
                ProviderError::InvalidResponse("missing choices[0].message.content".into())
            })?;
        let output = serde_json::from_str(content).map_err(|error| {
            ProviderError::InvalidResponse(format!("structured response JSON: {error}"))
        })?;
        Ok(ProviderResponse {
            output,
            usage: token_usage(&value),
        })
    }
}

fn token_usage(value: &serde_json::Value) -> TokenUsage {
    TokenUsage {
        input_tokens: value
            .pointer("/usage/prompt_tokens")
            .and_then(|v| v.as_u64()),
        output_tokens: value
            .pointer("/usage/completion_tokens")
            .and_then(|v| v.as_u64()),
    }
}
