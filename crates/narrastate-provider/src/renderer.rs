use narrastate_core::character::CharacterDefinition;
use narrastate_core::id::{ClaimId, FactId};
use narrastate_runtime::ports::{ChatMessage, LlmProvider, ProviderError, TokenUsage};
use narrastate_runtime::DialoguePlan;
use std::sync::Arc;

use crate::validator::OutputValidator;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RendererOutput {
    pub utterance: String,
    pub expressed_claim_ids: Vec<ClaimId>,
    pub acknowledged_fact_ids: Vec<FactId>,
    pub tone: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RendererStatus {
    Model,
    Repaired,
    TemplateFallback,
}

pub struct LlmRenderer {
    provider: Arc<dyn LlmProvider>,
}

impl LlmRenderer {
    pub fn new(provider: Arc<dyn LlmProvider>) -> Self {
        Self { provider }
    }

    pub async fn render_validated(
        &self,
        plan: &DialoguePlan,
        character: &CharacterDefinition,
        recent_dialogue: &[(String, String)],
    ) -> (RendererOutput, RendererStatus) {
        let (output, status, _) = self
            .render_validated_with_usage(plan, character, recent_dialogue)
            .await;
        (output, status)
    }

    pub async fn render_validated_with_usage(
        &self,
        plan: &DialoguePlan,
        character: &CharacterDefinition,
        recent_dialogue: &[(String, String)],
    ) -> (RendererOutput, RendererStatus, TokenUsage) {
        let validator = OutputValidator::new();
        match self.render(plan, character, recent_dialogue, None).await {
            Ok((output, usage)) if validator.validate(&output, plan).is_ok() => {
                (output, RendererStatus::Model, usage)
            }
            Ok((output, initial_usage)) => {
                let repair = format!("The previous JSON violated the allow-list or dialogue act. Correct only format and authorization errors. Previous output: {}", serde_json::to_string(&output).unwrap_or_default());
                match self
                    .render(plan, character, recent_dialogue, Some(&repair))
                    .await
                {
                    Ok((repaired, repair_usage)) if validator.validate(&repaired, plan).is_ok() => {
                        (
                            repaired,
                            RendererStatus::Repaired,
                            initial_usage.combine(repair_usage),
                        )
                    }
                    Ok((_, repair_usage)) => (
                        validator.template_fallback(plan),
                        RendererStatus::TemplateFallback,
                        initial_usage.combine(repair_usage),
                    ),
                    Err(_) => (
                        validator.template_fallback(plan),
                        RendererStatus::TemplateFallback,
                        initial_usage,
                    ),
                }
            }
            Err(_) => (
                validator.template_fallback(plan),
                RendererStatus::TemplateFallback,
                TokenUsage::default(),
            ),
        }
    }

    async fn render(
        &self,
        plan: &DialoguePlan,
        character: &CharacterDefinition,
        recent_dialogue: &[(String, String)],
        repair: Option<&str>,
    ) -> Result<(RendererOutput, TokenUsage), ProviderError> {
        let system = format!(
            "Role-play as {} ({}) with traits {}. Render only the deterministic plan. Required act: {:?}. Allowed claim IDs: {:?}. Allowed fact IDs: {:?}. Forbidden fact IDs: {:?}. Never claim system/model identity or invent facts. Return strict JSON.",
            character.name, character.role, character.personality.traits.join(", "), plan.act,
            plan.allowed_claims, plan.allowed_facts, plan.forbidden_facts
        );
        let mut context = recent_dialogue
            .iter()
            .rev()
            .take(12)
            .rev()
            .map(|(speaker, text)| format!("{speaker}: {text}"))
            .collect::<Vec<_>>()
            .join("\n");
        if let Some(instruction) = repair {
            context.push_str(&format!("\nREPAIR_INSTRUCTION: {instruction}"));
        }
        let schema = serde_json::json!({
            "type":"object","additionalProperties":false,
            "properties":{
                "utterance":{"type":"string","minLength":1,"maxLength":500},
                "expressed_claim_ids":{"type":"array","items":{"type":"string"}},
                "acknowledged_fact_ids":{"type":"array","items":{"type":"string"}},
                "tone":{"type":"string","enum":["neutral","defensive","agitated","controlled_defensive","evasive","resigned","confessional","angry","calm"]}
            },
            "required":["utterance","expressed_claim_ids","acknowledged_fact_ids","tone"]
        });
        let response = self
            .provider
            .chat_structured(
                &[
                    ChatMessage::system(system),
                    ChatMessage::user(format!("RECENT_DIALOGUE_DATA:\n{context}")),
                ],
                &schema,
            )
            .await?;
        let output = serde_json::from_value(response.output)
            .map_err(|error| ProviderError::InvalidResponse(format!("renderer output: {error}")))?;
        Ok((output, response.usage))
    }
}
