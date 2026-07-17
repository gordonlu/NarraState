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
        let native = self.send(serde_json::json!({
            "model": self.config.model,
            "messages": messages,
            "temperature": 0.3,
            "max_tokens": self.config.structured_output_max_tokens,
            "response_format": {"type": "json_schema", "json_schema": {"name": "structured_output", "schema": response_schema, "strict": true}}
        })).await;
        let (value, native_schema_supported) = match native {
            Ok(value) => (value, true),
            Err(error) if should_retry_without_json_schema(&error) => {
                let mut fallback_messages = vec![ChatMessage::system(format!(
                    "当前 Provider 不支持原生 JSON Schema 模式。请只返回一个严格符合下列 Schema 的数据实例，不要使用 Markdown，不要添加额外字段。不得把 properties、type、$ref、oneOf、anyOf、required 或 definitions 等 Schema 关键字当作字段值输出。枚举字段必须是允许的单个标量值，不得输出 Schema 对象：\n{}",
                    serde_json::to_string(response_schema).unwrap_or_default()
                ))];
                fallback_messages.extend(messages.iter().cloned());
                let value = self
                    .send(serde_json::json!({
                        "model": self.config.model,
                        "messages": fallback_messages,
                        "temperature": 0.3,
                        "max_tokens": self.config.structured_output_max_tokens,
                        "response_format": {"type": "json_object"}
                    }))
                    .await?;
                (value, false)
            }
            Err(error) => return Err(error),
        };
        let mut usage = token_usage(&value);
        let output = match parse_structured_output(&value) {
            Ok(output) => output,
            Err(ProviderError::OutputTruncated) => {
                let mut correction_messages = vec![ChatMessage::system(format!(
                    "上一次响应达到了 Provider 的输出上限。请从原始请求重新生成完整且明显更紧凑的 JSON 对象。使用短字符串，删除风格化扩写和重复描述，通过 ID 复用共享定义。必须保留所有必需事实、证据链接、引用、披露前置、结案要求和指定变体。不要续写被截断的响应，不要返回补丁。必须符合的 Schema：\n{}",
                    serde_json::to_string(response_schema).unwrap_or_default()
                ))];
                correction_messages.extend(messages.iter().cloned());
                let response_format = if native_schema_supported {
                    serde_json::json!({"type": "json_schema", "json_schema": {"name": "structured_output", "schema": response_schema, "strict": true}})
                } else {
                    serde_json::json!({"type": "json_object"})
                };
                let corrected = self
                    .send(serde_json::json!({
                        "model": self.config.model,
                        "messages": correction_messages,
                        "temperature": 0.15,
                        "max_tokens": self.config.structured_output_max_tokens,
                        "response_format": response_format
                    }))
                    .await?;
                usage = usage.combine(token_usage(&corrected));
                parse_structured_output(&corrected)?
            }
            Err(first_error) if is_repairable_structured_parse(&first_error) => {
                let mut correction_messages = vec![ChatMessage::system(format!(
                    "上一次响应不是请求结构所需的有效 JSON。请从原始请求重新生成完整数据实例，只返回一个 JSON 对象；不要引用或修补上一次响应，不得把 JSON Schema 关键字当作数据。解析错误：{first_error}\n必须符合的 Schema：\n{}",
                    serde_json::to_string(response_schema).unwrap_or_default()
                ))];
                correction_messages.extend(messages.iter().cloned());
                let corrected = self
                    .send(serde_json::json!({
                        "model": self.config.model,
                        "messages": correction_messages,
                        "temperature": 0.2,
                        "max_tokens": self.config.structured_output_max_tokens,
                        "response_format": {"type": "json_object"}
                    }))
                    .await?;
                usage = usage.combine(token_usage(&corrected));
                parse_structured_output(&corrected)?
            }
            Err(error) => return Err(error),
        };
        Ok(ProviderResponse { output, usage })
    }
}

fn parse_structured_output(value: &serde_json::Value) -> Result<serde_json::Value, ProviderError> {
    if value
        .pointer("/choices/0/finish_reason")
        .and_then(|reason| reason.as_str())
        .is_some_and(|reason| reason == "length")
    {
        return Err(ProviderError::OutputTruncated);
    }
    let content = value
        .pointer("/choices/0/message/content")
        .and_then(|content| content.as_str())
        .ok_or_else(|| {
            ProviderError::InvalidResponse("missing choices[0].message.content".into())
        })?;
    serde_json::from_str(content).map_err(|error| {
        if error.is_eof() {
            ProviderError::OutputTruncated
        } else {
            ProviderError::InvalidResponse(format!("structured response JSON: {error}"))
        }
    })
}

fn is_repairable_structured_parse(error: &ProviderError) -> bool {
    matches!(error, ProviderError::InvalidResponse(_))
}

fn should_retry_without_json_schema(error: &ProviderError) -> bool {
    let ProviderError::Unknown(message) = error else {
        return false;
    };
    let message = message.to_ascii_lowercase();
    [
        "json_schema",
        "response_format",
        "unsupported",
        "not supported",
        "parameter",
        "not valid",
    ]
    .iter()
    .any(|needle| message.contains(needle))
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

#[cfg(test)]
mod tests {
    use super::{
        is_repairable_structured_parse, parse_structured_output, should_retry_without_json_schema,
    };
    use narrastate_runtime::ports::ProviderError;

    #[test]
    fn unsupported_structured_output_errors_use_compatible_json_mode() {
        assert!(should_retry_without_json_schema(&ProviderError::Unknown(
            "A parameter specified in the request is not valid".into(),
        )));
        assert!(!should_retry_without_json_schema(
            &ProviderError::Unauthorized
        ));
    }

    #[test]
    fn eof_json_errors_are_classified_as_truncated_output() {
        let incomplete = serde_json::json!({
            "choices": [{"finish_reason": "stop", "message": {"content": "{\"case\":{\"title\":\"unfinished\"}"}}]
        });
        assert!(matches!(
            parse_structured_output(&incomplete),
            Err(ProviderError::OutputTruncated)
        ));

        let length = serde_json::json!({
            "choices": [{"finish_reason": "length", "message": {"content": "{}"}}]
        });
        assert!(matches!(
            parse_structured_output(&length),
            Err(ProviderError::OutputTruncated)
        ));
        assert!(!is_repairable_structured_parse(
            &ProviderError::OutputTruncated
        ));

        assert!(is_repairable_structured_parse(
            &ProviderError::InvalidResponse("malformed JSON".into())
        ));
    }
}
