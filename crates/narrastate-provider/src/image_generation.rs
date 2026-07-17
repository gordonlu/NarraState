use async_trait::async_trait;
use base64::Engine;
use narrastate_runtime::ports::{
    GeneratedImageAsset, ImageGenerationProvider, ImageGenerationRequest, LlmConfig, ProviderError,
};
use serde::Deserialize;

pub struct OpenAiCompatibleImageProvider {
    client: reqwest::Client,
    config: LlmConfig,
}

impl OpenAiCompatibleImageProvider {
    pub fn new(config: LlmConfig) -> Result<Self, ProviderError> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .map_err(|error| ProviderError::Network(error.to_string()))?;
        Ok(Self { client, config })
    }
}

#[derive(Deserialize)]
struct ImageResponse {
    data: Vec<ImageData>,
}

#[derive(Deserialize)]
struct ImageData {
    b64_json: Option<String>,
}

#[async_trait]
impl ImageGenerationProvider for OpenAiCompatibleImageProvider {
    async fn generate_image(
        &self,
        request: &ImageGenerationRequest,
    ) -> Result<GeneratedImageAsset, ProviderError> {
        let url = format!(
            "{}/images/generations",
            self.config.base_url.trim_end_matches('/')
        );
        let response = self
            .client
            .post(url)
            .bearer_auth(&self.config.api_key)
            .json(&image_request_body(&self.config, request))
            .send()
            .await
            .map_err(|error| {
                if error.is_timeout() {
                    ProviderError::Timeout
                } else {
                    ProviderError::Network(error.to_string())
                }
            })?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(ProviderError::Unauthorized);
        }
        if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(ProviderError::RateLimited);
        }
        if !response.status().is_success() {
            let status = response.status();
            let detail = response
                .text()
                .await
                .ok()
                .and_then(|body| safe_provider_error_message(&body, &self.config.api_key));
            return Err(ProviderError::InvalidResponse(format!(
                "image endpoint returned {status}{}",
                detail
                    .map(|message| format!(": {message}"))
                    .unwrap_or_default()
            )));
        }
        let body: ImageResponse = response
            .json()
            .await
            .map_err(|error| ProviderError::InvalidResponse(error.to_string()))?;
        let encoded = body
            .data
            .first()
            .and_then(|item| item.b64_json.as_ref())
            .ok_or_else(|| ProviderError::InvalidResponse("missing data[0].b64_json".into()))?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|error| ProviderError::InvalidResponse(error.to_string()))?;
        if bytes.is_empty() {
            return Err(ProviderError::InvalidResponse(
                "generated image was empty".into(),
            ));
        }
        Ok(GeneratedImageAsset {
            mime_type: "image/png".into(),
            bytes,
        })
    }
}

fn image_request_body(config: &LlmConfig, request: &ImageGenerationRequest) -> serde_json::Value {
    let seedream = config.model.to_ascii_lowercase().contains("seedream");
    let mut body = serde_json::json!({
        "model": config.model,
        "prompt": request.prompt,
        "size": if seedream {
            "2K".to_string()
        } else {
            format!("{}x{}", request.width, request.height)
        },
        "response_format": "b64_json"
    });
    if seedream {
        body["sequential_image_generation"] = serde_json::json!("disabled");
        body["stream"] = serde_json::json!(false);
        body["watermark"] = serde_json::json!(false);
    }
    body
}

fn safe_provider_error_message(body: &str, api_key: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let message = value
        .pointer("/error/message")
        .or_else(|| value.get("message"))
        .and_then(serde_json::Value::as_str)?;
    let normalized = message.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return None;
    }
    let redacted = if api_key.is_empty() {
        normalized
    } else {
        normalized.replace(api_key, "[redacted]")
    };
    Some(redacted.chars().take(280).collect())
}

#[cfg(test)]
mod tests {
    use super::{image_request_body, safe_provider_error_message};
    use narrastate_core::GeneratedVisualType;
    use narrastate_runtime::ports::{ImageGenerationRequest, LlmConfig};

    #[test]
    fn provider_error_detail_is_structured_bounded_and_redacted() {
        let body = r#"{"error":{"message":"model does not support images with secret-key"}}"#;
        assert_eq!(
            safe_provider_error_message(body, "secret-key").as_deref(),
            Some("model does not support images with [redacted]")
        );
        assert!(safe_provider_error_message("<html>bad gateway</html>", "secret-key").is_none());
        let long = format!(r#"{{"error":{{"message":"{}"}}}}"#, "x".repeat(400));
        assert_eq!(safe_provider_error_message(&long, "").unwrap().len(), 280);
    }

    #[test]
    fn seedream_uses_ark_compatible_single_image_parameters() {
        let config = LlmConfig {
            model: "doubao-seedream-4-5-251128".into(),
            ..LlmConfig::default()
        };
        let request = ImageGenerationRequest {
            visual_type: GeneratedVisualType::CharacterPortrait,
            prompt: "中性人物头像".into(),
            alt_text: "人物头像".into(),
            width: 512,
            height: 512,
        };
        let body = image_request_body(&config, &request);
        assert_eq!(body["size"], "2K");
        assert_eq!(body["sequential_image_generation"], "disabled");
        assert_eq!(body["stream"], false);
        assert_eq!(body["watermark"], false);
        assert_eq!(body["response_format"], "b64_json");
    }

    #[test]
    fn generic_openai_image_provider_preserves_requested_dimensions() {
        let request = ImageGenerationRequest {
            visual_type: GeneratedVisualType::CaseCover,
            prompt: "中性封面".into(),
            alt_text: "封面".into(),
            width: 1024,
            height: 1024,
        };
        let body = image_request_body(&LlmConfig::default(), &request);
        assert_eq!(body["size"], "1024x1024");
        assert!(body.get("sequential_image_generation").is_none());
    }
}
