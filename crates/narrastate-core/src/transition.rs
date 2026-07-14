use crate::evidence::EvidenceUse;
use crate::fact::Proposition;
use crate::id::{ClaimId, EntityRef};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct InterpretedAction {
    pub intent: PlayerIntent,
    pub topics: Vec<String>,
    pub referenced_entities: Vec<EntityRef>,
    pub referenced_claims: Vec<ClaimId>,
    pub evidence_usage: Vec<EvidenceUse>,
    pub asserted_propositions: Vec<Proposition>,
    pub tone: PlayerTone,
    pub confidence: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum PlayerIntent {
    Ask,
    Clarify,
    Challenge,
    PresentEvidence,
    Accuse,
    Empathize,
    Threaten,
    ChangeSubject,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum PlayerTone {
    Neutral,
    Aggressive,
    Friendly,
    Sarcastic,
    Desperate,
    Accusatory,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum TransitionReason {
    NewEvidencePresented,
    PriorClaimContradicted,
    DefenseExhausted,
    DisclosurePrerequisitesMet,
    RepeatedQuestionNoNewInformation,
    DirectChallenge,
    AccusationSubmitted,
    CaseResolved,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TransitionTuning {
    pub reliability_weight: f32,
    pub directness_weight: f32,
    pub exclusivity_weight: f32,
    pub proposition_match_weight: f32,
    pub novelty_multiplier_first: f32,
    pub novelty_multiplier_repeat: f32,
    pub chain_bonus: f32,
    pub min_interpretation_multiplier: f32,
    pub max_interpretation_multiplier: f32,

    pub stress_per_impact_base: f32,
    pub stress_resilience_reduction: f32,
    pub defense_per_impact: f32,
    pub composure_per_impact: f32,
    pub trust_range_min: i32,
    pub trust_range_max: i32,
}

impl Default for TransitionTuning {
    fn default() -> Self {
        Self {
            reliability_weight: 0.35,
            directness_weight: 0.30,
            exclusivity_weight: 0.20,
            proposition_match_weight: 0.15,
            novelty_multiplier_first: 1.0,
            novelty_multiplier_repeat: 0.0,
            chain_bonus: 0.15,
            min_interpretation_multiplier: 0.5,
            max_interpretation_multiplier: 1.0,
            stress_per_impact_base: 35.0,
            stress_resilience_reduction: 0.15,
            defense_per_impact: 30.0,
            composure_per_impact: 20.0,
            trust_range_min: -10,
            trust_range_max: 8,
        }
    }
}
