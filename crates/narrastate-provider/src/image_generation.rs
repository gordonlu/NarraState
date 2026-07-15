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
            .json(&serde_json::json!({
                "model": self.config.model,
                "prompt": request.prompt,
                "size": format!("{}x{}", request.width, request.height),
                "response_format": "b64_json"
            }))
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
            return Err(ProviderError::InvalidResponse(format!(
                "image endpoint returned {}",
                response.status()
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
