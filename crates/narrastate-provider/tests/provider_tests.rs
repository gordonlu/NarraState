use async_trait::async_trait;
use narrastate_core::{ClaimId, DialogueAct, EvidenceId};
use narrastate_provider::interpreter::LlmInterpreter;
use narrastate_provider::renderer::{LlmRenderer, RendererStatus};
use narrastate_runtime::ports::{
    ChatMessage, LlmProvider, ProviderError, ProviderResponse, TokenUsage,
};
use narrastate_runtime::DialoguePlan;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

struct ScriptedProvider {
    responses: Mutex<VecDeque<Result<serde_json::Value, ProviderError>>>,
}

impl ScriptedProvider {
    fn new(responses: Vec<Result<serde_json::Value, ProviderError>>) -> Self {
        Self {
            responses: Mutex::new(responses.into()),
        }
    }
}

#[async_trait]
impl LlmProvider for ScriptedProvider {
    async fn chat(
        &self,
        _messages: &[ChatMessage],
    ) -> Result<ProviderResponse<String>, ProviderError> {
        Err(ProviderError::Unknown("not scripted".into()))
    }

    async fn chat_structured(
        &self,
        _messages: &[ChatMessage],
        _response_schema: &serde_json::Value,
    ) -> Result<ProviderResponse<serde_json::Value>, ProviderError> {
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .expect("scripted response")
            .map(|output| ProviderResponse {
                output,
                usage: TokenUsage::default(),
            })
    }
}

fn case() -> narrastate_core::CaseDefinition {
    serde_json::from_str(include_str!("../../../cases/rain-gallery/case.json")).unwrap()
}

#[tokio::test]
async fn interpreter_enforces_claim_allow_list() {
    let case = case();
    let character = &case.characters[0];
    let provider = Arc::new(ScriptedProvider::new(vec![Ok(serde_json::json!({
        "intent":"Challenge",
        "topics":["x"],
        "referenced_claims":["invented_claim"],
        "tone":"Neutral",
        "confidence":1.0
    }))]));
    let error = LlmInterpreter::new(provider)
        .interpret(
            "解释",
            &[],
            character,
            &case.evidence,
            &[ClaimId::from("claim_never_left")],
        )
        .await
        .expect_err("unknown claim must be rejected");
    assert!(matches!(error, ProviderError::InvalidResponse(_)));
}

#[tokio::test]
async fn interpreter_preserves_authoritative_evidence_attachment() {
    let case = case();
    let character = &case.characters[0];
    let provider = Arc::new(ScriptedProvider::new(vec![Ok(serde_json::json!({
        "intent":"Ask",
        "topics":["门禁"],
        "referenced_claims":[],
        "tone":"Neutral",
        "confidence":0.9
    }))]));
    let evidence = EvidenceId::from("ev_card_log");
    let result = LlmInterpreter::new(provider)
        .interpret(
            "解释门禁记录",
            std::slice::from_ref(&evidence),
            character,
            &case.evidence,
            &[ClaimId::from("claim_never_left")],
        )
        .await
        .unwrap();
    assert_eq!(
        result.intent,
        narrastate_core::PlayerIntent::PresentEvidence
    );
    assert_eq!(result.evidence_usage[0].evidence_id, evidence);
}

#[tokio::test]
async fn invalid_renderer_output_repairs_once_then_uses_template() {
    let case = case();
    let character = &case.characters[0];
    let invalid = serde_json::json!({
        "utterance":"我认罪，是我做的。",
        "expressed_claim_ids":[],
        "acknowledged_fact_ids":["fact_painting_hidden"],
        "tone":"confessional"
    });
    let provider = Arc::new(ScriptedProvider::new(vec![
        Ok(invalid.clone()),
        Ok(invalid),
    ]));
    let plan = DialoguePlan {
        act: DialogueAct::Deny,
        strategy: None,
        allowed_claims: Vec::new(),
        allowed_facts: Vec::new(),
        newly_revealed: None,
        forbidden_facts: vec![narrastate_core::FactId::from("fact_painting_hidden")],
    };
    let (output, status) = LlmRenderer::new(provider)
        .render_validated(&plan, character, &[])
        .await;
    assert_eq!(status, RendererStatus::TemplateFallback);
    assert!(!output.utterance.is_empty());
    assert!(output.acknowledged_fact_ids.is_empty());
}
