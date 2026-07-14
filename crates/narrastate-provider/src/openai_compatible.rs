use narrastate_runtime::ports::{ChatMessage, LlmConfig, LlmProvider, ProviderError};

pub struct OpenAiProvider {
    config: LlmConfig,
    client: reqwest::blocking::Client,
}

impl OpenAiProvider {
    pub fn new(config: LlmConfig) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .expect("Failed to build HTTP client");
        Self { config, client }
    }
}

impl LlmProvider for OpenAiProvider {
    fn chat(&self, messages: &[ChatMessage]) -> Result<String, ProviderError> {
        let url = format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        );
        let body = serde_json::json!({
            "model": self.config.model,
            "messages": messages,
            "temperature": 0.7,
            "max_tokens": 2048,
        });

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .json(&body)
            .send()
            .map_err(|e| {
                if e.is_timeout() {
                    ProviderError::Timeout
                } else if e.is_connect() {
                    ProviderError::Network(e.to_string())
                } else {
                    ProviderError::Unknown(e.to_string())
                }
            })?;

        let status = resp.status();
        let json: serde_json::Value = resp.json().map_err(|e| {
            ProviderError::InvalidResponse(format!("Failed to parse response: {e}"))
        })?;

        if status.is_success() {
            let content = json["choices"][0]["message"]["content"]
                .as_str()
                .ok_or_else(|| ProviderError::InvalidResponse("No content in response".into()))?
                .to_string();
            Ok(content)
        } else {
            let err_msg = json["error"]["message"].as_str().unwrap_or("unknown");
            match status.as_u16() {
                401 => Err(ProviderError::Unauthorized),
                429 => Err(ProviderError::RateLimited),
                500..=599 => Err(ProviderError::Unknown(err_msg.into())),
                _ => Err(ProviderError::Unknown(err_msg.into())),
            }
        }
    }

    fn chat_structured(
        &self,
        messages: &[ChatMessage],
        response_schema: &serde_json::Value,
    ) -> Result<serde_json::Value, ProviderError> {
        let url = format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        );
        let body = serde_json::json!({
            "model": self.config.model,
            "messages": messages,
            "temperature": 0.3,
            "max_tokens": 4096,
            "response_format": {
                "type": "json_schema",
                "json_schema": {
                    "name": "structured_output",
                    "schema": response_schema,
                    "strict": true
                }
            }
        });

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .json(&body)
            .send()
            .map_err(|e| {
                if e.is_timeout() {
                    ProviderError::Timeout
                } else if e.is_connect() {
                    ProviderError::Network(e.to_string())
                } else {
                    ProviderError::Unknown(e.to_string())
                }
            })?;

        let status = resp.status();
        let json: serde_json::Value = resp.json().map_err(|e| {
            ProviderError::InvalidResponse(format!("Failed to parse response: {e}"))
        })?;

        if status.is_success() {
            let content = json["choices"][0]["message"]["content"]
                .as_str()
                .ok_or_else(|| ProviderError::InvalidResponse("No content in response".into()))?;
            serde_json::from_str(content).map_err(|e| {
                ProviderError::InvalidResponse(format!("Response is not valid JSON: {e}"))
            })
        } else {
            let err_msg = json["error"]["message"].as_str().unwrap_or("unknown");
            match status.as_u16() {
                401 => Err(ProviderError::Unauthorized),
                429 => Err(ProviderError::RateLimited),
                500..=599 => Err(ProviderError::Unknown(err_msg.into())),
                _ => Err(ProviderError::Unknown(err_msg.into())),
            }
        }
    }
}
