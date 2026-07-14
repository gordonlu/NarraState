use narrastate_core::evidence::EvidenceUse;
use narrastate_core::id::{EvidenceId, FactId};
use narrastate_core::transition::{InterpretedAction, PlayerIntent, PlayerTone};
use narrastate_core::ClaimId;

use crate::planner::DialoguePlan;

pub struct MockInterpreter;

impl MockInterpreter {
    pub fn interpret(&self, text: &str, attached_evidence: &[EvidenceId]) -> InterpretedAction {
        let lower = text.to_lowercase();
        let intent = if !attached_evidence.is_empty() {
            PlayerIntent::PresentEvidence
        } else if lower.contains("accuse")
            || lower.contains("指控")
            || lower.contains("是你偷的")
            || lower.contains("是你做的")
            || lower.contains("认罪")
        {
            PlayerIntent::Accuse
        } else if lower.contains("challenge") || lower.contains("证据") || lower.contains("证明")
        {
            PlayerIntent::Challenge
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
            narrastate_core::DialogueAct::Answer => "我会回答你的问题。".to_string(),
            narrastate_core::DialogueAct::Deny => "这不是真的，我否认这一点。".to_string(),
            narrastate_core::DialogueAct::Evade => "这件事我记得不太清楚。".to_string(),
            narrastate_core::DialogueAct::Reframe => "事情并不是你说的那样。".to_string(),
            narrastate_core::DialogueAct::ChallengeEvidence => {
                "这份证据还不能证明你的结论。".to_string()
            }
            narrastate_core::DialogueAct::ShiftBlame => "你应该再查查其他人。".to_string(),
            narrastate_core::DialogueAct::PartialAdmission => {
                if let Some(ref _did) = plan.newly_revealed {
                    "好吧，这一点我承认。".to_string()
                } else {
                    "我承认有所牵涉，但事情不是你想的那样。".to_string()
                }
            }
            narrastate_core::DialogueAct::FullAdmission => "我认罪，是我做的。".to_string(),
            narrastate_core::DialogueAct::AskForClarification => "你具体想问什么？".to_string(),
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
