use narrastate_core::evidence::EvidenceUse;
use narrastate_core::id::{EvidenceId, FactId};
use narrastate_core::transition::{InterpretedAction, PlayerIntent, PlayerTone};
use narrastate_core::ClaimId;

use crate::planner::DialoguePlan;

pub struct MockInterpreter;

impl MockInterpreter {
    pub fn interpret(&self, text: &str, attached_evidence: &[EvidenceId]) -> InterpretedAction {
        let lower = text.to_lowercase();
        let intent = if lower.contains("accuse")
            || lower.contains("指控")
            || lower.contains("是你偷的")
        {
            PlayerIntent::Accuse
        } else if lower.contains("challenge") || lower.contains("证据") || lower.contains("证明")
        {
            PlayerIntent::Challenge
        } else if !attached_evidence.is_empty() {
            PlayerIntent::PresentEvidence
        } else if lower.contains("clarify") || lower.contains("解释") {
            PlayerIntent::Clarify
        } else {
            PlayerIntent::Ask
        };

        let evidence_usage: Vec<EvidenceUse> = attached_evidence
            .iter()
            .map(|eid| EvidenceUse {
                evidence_id: eid.clone(),
                usage: narrastate_core::evidence::EvidenceUsageKind::DirectReference,
            })
            .collect();

        InterpretedAction {
            intent,
            topics: vec![text.to_string()],
            referenced_entities: vec![],
            referenced_claims: vec![],
            evidence_usage,
            asserted_propositions: vec![],
            tone: PlayerTone::Neutral,
            confidence: 1.0,
        }
    }
}

pub struct MockRenderer;

impl MockRenderer {
    pub fn render(&self, plan: &DialoguePlan) -> MockUtterance {
        let utterance = match plan.act {
            narrastate_core::DialogueAct::Answer => "I am answering your question.".to_string(),
            narrastate_core::DialogueAct::Deny => "That's not true. I deny it.".to_string(),
            narrastate_core::DialogueAct::Evade => "I don't recall that clearly.".to_string(),
            narrastate_core::DialogueAct::Reframe => {
                "Let me explain the situation differently.".to_string()
            }
            narrastate_core::DialogueAct::ChallengeEvidence => {
                "That evidence doesn't prove anything.".to_string()
            }
            narrastate_core::DialogueAct::ShiftBlame => {
                "Someone else must have done it.".to_string()
            }
            narrastate_core::DialogueAct::PartialAdmission => {
                if let Some(ref _did) = plan.newly_revealed {
                    "Alright, I admit that much.".to_string()
                } else {
                    "I admit to some involvement, but not everything.".to_string()
                }
            }
            narrastate_core::DialogueAct::FullAdmission => "I confess. I did it.".to_string(),
            narrastate_core::DialogueAct::AskForClarification => {
                "What exactly are you asking?".to_string()
            }
            narrastate_core::DialogueAct::Silence => "...".to_string(),
        };

        MockUtterance {
            utterance,
            expressed_claim_ids: plan.allowed_claims.clone(),
            acknowledged_fact_ids: plan.allowed_facts.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MockUtterance {
    pub utterance: String,
    pub expressed_claim_ids: Vec<ClaimId>,
    pub acknowledged_fact_ids: Vec<FactId>,
}
