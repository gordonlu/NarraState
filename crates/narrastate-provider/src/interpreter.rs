use narrastate_core::character::CharacterDefinition;
use narrastate_core::evidence::{EvidenceDefinition, EvidenceUsageKind, EvidenceUse};
use narrastate_core::fact::FactValue;
use narrastate_core::id::{ClaimId, EvidenceId};
use narrastate_core::transition::{InterpretedAction, PlayerIntent, PlayerTone};
use narrastate_runtime::ports::{ChatMessage, LlmProvider, ProviderError};

pub struct LlmInterpreter<P: LlmProvider> {
    provider: P,
}

impl<P: LlmProvider> LlmInterpreter<P> {
    pub fn new(provider: P) -> Self {
        Self { provider }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn interpret(
        &self,
        text: &str,
        attached_evidence: &[EvidenceId],
        character: &CharacterDefinition,
        player_known_evidence: &[EvidenceDefinition],
        available_claims: &[ClaimId],
    ) -> Result<InterpretedAction, ProviderError> {
        let evidence_descriptions: Vec<String> = player_known_evidence
            .iter()
            .map(|e| format!("  - {}: {}", e.id, e.title))
            .collect();

        let claim_descriptions: Vec<String> = available_claims
            .iter()
            .filter_map(|cid| {
                character.claims.iter().find(|c| &c.id == cid).map(|c| {
                    format!(
                        "  - {}: {} {} {}",
                        c.id,
                        c.proposition.subject,
                        c.proposition.predicate,
                        json_fact_value(&c.proposition.object)
                    )
                })
            })
            .collect();

        let evidence_attached: Vec<String> = attached_evidence
            .iter()
            .map(|eid| format!("  - {eid}"))
            .collect();

        let system_prompt = r#"You are an AI interpreter for an interrogation game. Analyze the player's input and produce a structured JSON interpretation.

INTENT options: Ask, Clarify, Challenge, PresentEvidence, Accuse, Empathize, Threaten, ChangeSubject
TONE options: Neutral, Aggressive, Friendly, Sarcastic, Desperate, Accusatory

RULES:
- If player attaches evidence, intent MUST be PresentEvidence
- Only reference evidence and claims from the provided lists
- "是你偷的"/"accuse" / direct accusation language → Accuse intent
- Friendly/empathetic language → Friendly tone
- Aggressive/confrontational language → Aggressive or Accusatory tone
- Repeated questions with no new evidence → confidence below 0.5

Output ONLY valid JSON matching the schema."#;

        let user_prompt = format!(
            r#"CHARACTER: {} ({})
PLAYER TEXT: "{}"
EVIDENCE ATTACHED: [{}]

AVAILABLE EVIDENCE:
{}

AVAILABLE CLAIMS:
{}"#,
            character.name,
            character.id,
            text,
            evidence_attached.join(", "),
            evidence_descriptions.join("\n"),
            claim_descriptions.join("\n"),
        );

        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "intent": {
                    "type": "string",
                    "enum": ["Ask", "Clarify", "Challenge", "PresentEvidence", "Accuse", "Empathize", "Threaten", "ChangeSubject"]
                },
                "topics": { "type": "array", "items": { "type": "string" } },
                "referenced_claims": { "type": "array", "items": { "type": "string" } },
                "evidence_usage": { "type": "array", "items": { "type": "string" } },
                "tone": {
                    "type": "string",
                    "enum": ["Neutral", "Aggressive", "Friendly", "Sarcastic", "Desperate", "Accusatory"]
                },
                "confidence": { "type": "number", "minimum": 0.0, "maximum": 1.0 }
            },
            "required": ["intent", "topics", "referenced_claims", "evidence_usage", "tone", "confidence"]
        });

        let result = self.provider.chat_structured(
            &[
                ChatMessage::system(system_prompt),
                ChatMessage::user(&user_prompt),
            ],
            &schema,
        )?;

        let intent: PlayerIntent = serde_json::from_value(result["intent"].clone())
            .map_err(|e| ProviderError::InvalidResponse(format!("Bad intent: {e}")))?;

        let topics: Vec<String> = serde_json::from_value(result["topics"].clone())
            .map_err(|e| ProviderError::InvalidResponse(format!("Bad topics: {e}")))?;

        let referenced_claims: Vec<ClaimId> =
            serde_json::from_value(result["referenced_claims"].clone())
                .map_err(|e| ProviderError::InvalidResponse(format!("Bad claims: {e}")))?;

        let evidence_ids: Vec<EvidenceId> =
            serde_json::from_value(result["evidence_usage"].clone())
                .map_err(|e| ProviderError::InvalidResponse(format!("Bad evidence: {e}")))?;

        let tone: PlayerTone = serde_json::from_value(result["tone"].clone())
            .map_err(|e| ProviderError::InvalidResponse(format!("Bad tone: {e}")))?;

        let confidence: f32 = serde_json::from_value(result["confidence"].clone())
            .map_err(|e| ProviderError::InvalidResponse(format!("Bad confidence: {e}")))?;

        let evidence_usage: Vec<EvidenceUse> = evidence_ids
            .into_iter()
            .map(|eid| EvidenceUse {
                evidence_id: eid,
                usage: EvidenceUsageKind::DirectReference,
            })
            .collect();

        Ok(InterpretedAction {
            intent,
            topics,
            referenced_entities: vec![],
            referenced_claims,
            evidence_usage,
            asserted_propositions: vec![],
            tone,
            confidence,
        })
    }
}

fn json_fact_value(value: &FactValue) -> String {
    match value {
        FactValue::String(s) => format!("\"{s}\""),
        FactValue::Number(n) => n.to_string(),
        FactValue::Boolean(b) => b.to_string(),
        FactValue::Entity(e) => e.to_string(),
    }
}
