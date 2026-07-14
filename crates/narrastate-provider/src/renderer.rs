use narrastate_core::character::CharacterDefinition;
use narrastate_core::id::{ClaimId, FactId};
use narrastate_runtime::ports::{ChatMessage, LlmProvider, ProviderError};
use narrastate_runtime::DialoguePlan;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RendererOutput {
    pub utterance: String,
    pub expressed_claim_ids: Vec<ClaimId>,
    pub acknowledged_fact_ids: Vec<FactId>,
    pub tone: String,
}

pub struct LlmRenderer<P: LlmProvider> {
    provider: P,
}

impl<P: LlmProvider> LlmRenderer<P> {
    pub fn new(provider: P) -> Self {
        Self { provider }
    }

    pub fn render(
        &self,
        plan: &DialoguePlan,
        character: &CharacterDefinition,
        recent_dialogue: &[(String, String)],
    ) -> Result<RendererOutput, ProviderError> {
        let dialogue_window: Vec<String> = recent_dialogue
            .iter()
            .map(|(speaker, text)| format!("{speaker}: {text}"))
            .collect();

        let system_prompt = format!(
            r#"You are role-playing as "{}" ({}) — {}.

PERSONALITY: {}

DIALOGUE RULES:
1. You may only reference claims and facts that the player has already discovered
2. Never reveal information beyond what is in the plan
3. Stay in character based on your personality and role
4. Match your response to the required dialogue act

REQUIRED DIALOGUE ACT: {:?}
DEFENSE STRATEGY: {}

Your response must be JSON:
{{
  "utterance": "your dialogue line",
  "expressed_claim_ids": ["claim_xxx"],
  "acknowledged_fact_ids": ["fact_xxx"],
  "tone": "controlled_defensive"
}}

Available claims you can reference:
{}

Available facts you can acknowledge:
{}

You MUST NOT reference these facts: {}"#,
            character.name,
            character.id,
            character.role,
            character.personality.traits.join(", "),
            plan.act,
            plan.strategy
                .as_ref()
                .map(|s| s.to_string())
                .unwrap_or_default(),
            plan.allowed_claims
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(", "),
            plan.allowed_facts
                .iter()
                .map(|f| f.to_string())
                .collect::<Vec<_>>()
                .join(", "),
            plan.forbidden_facts
                .iter()
                .map(|f| f.to_string())
                .collect::<Vec<_>>()
                .join(", "),
        );

        let user_prompt = if dialogue_window.is_empty() {
            "The interrogation is just beginning. Respond to the player's first question.".into()
        } else {
            format!("Recent conversation:\n{}", dialogue_window.join("\n"))
        };

        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "utterance": { "type": "string", "maxLength": 500 },
                "expressed_claim_ids": { "type": "array", "items": { "type": "string" } },
                "acknowledged_fact_ids": { "type": "array", "items": { "type": "string" } },
                "tone": {
                    "type": "string",
                    "enum": ["neutral", "defensive", "agitated", "controlled_defensive", "evasive", "resigned", "confessional", "angry", "calm"]
                }
            },
            "required": ["utterance", "expressed_claim_ids", "acknowledged_fact_ids", "tone"]
        });

        let result = self.provider.chat_structured(
            &[
                ChatMessage::system(&system_prompt),
                ChatMessage::user(&user_prompt),
            ],
            &schema,
        )?;

        let output: RendererOutput = serde_json::from_value(result)
            .map_err(|e| ProviderError::InvalidResponse(format!("Bad renderer output: {e}")))?;

        Ok(output)
    }
}
