use crate::claim::{ClaimDefinition, SpokenClaim};
use crate::disclosure::DisclosureGraph;
use crate::fact::Proposition as BeliefProposition;
use crate::id::{CharacterId, DefenseStrategyId, DisclosureId, EvidenceId, FactId, TurnId};
use crate::phase::InterrogationPhase;
use crate::strategy::DefenseStrategy;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CharacterDefinition {
    pub id: CharacterId,
    pub name: String,
    pub role: String,
    pub public_profile: String,
    pub personality: PersonalityProfile,
    pub goals: Vec<CharacterGoal>,
    pub knowledge: Vec<FactId>,
    pub initial_beliefs: Vec<Belief>,
    pub claims: Vec<ClaimDefinition>,
    pub defenses: Vec<DefenseStrategy>,
    pub disclosure_graph: DisclosureGraph,
    pub resilience: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PersonalityProfile {
    pub traits: Vec<String>,
    pub speech_style: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CharacterGoal {
    pub description: String,
    pub priority: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Belief {
    pub proposition: BeliefProposition,
    pub confidence: u8,
    pub source: BeliefSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum BeliefSource {
    DirectKnowledge,
    Inference,
    Hearsay,
    Default,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CharacterRuntimeState {
    pub phase: InterrogationPhase,
    pub stress: u8,
    pub composure: u8,
    pub trust: i8,
    pub defense_budget: u8,
    pub active_strategy: Option<DefenseStrategyId>,
    pub revealed_disclosures: BTreeSet<DisclosureId>,
    pub exhausted_defenses: BTreeSet<DefenseStrategyId>,
    pub spoken_claims: Vec<SpokenClaim>,
    pub confronted_evidence: BTreeSet<EvidenceId>,
    pub last_transition_turn: Option<TurnId>,
}

impl CharacterRuntimeState {
    pub fn new(_resilience: u8) -> Self {
        Self {
            phase: InterrogationPhase::Calm,
            stress: 0,
            composure: 100,
            trust: 0,
            defense_budget: 100,
            active_strategy: None,
            revealed_disclosures: BTreeSet::new(),
            exhausted_defenses: BTreeSet::new(),
            spoken_claims: Vec::new(),
            confronted_evidence: BTreeSet::new(),
            last_transition_turn: None,
        }
    }

    pub fn apply_stress_delta(&mut self, delta: i32) {
        self.stress = (self.stress as i32 + delta).clamp(0, 100) as u8;
    }

    pub fn apply_composure_delta(&mut self, delta: i32) {
        self.composure = (self.composure as i32 + delta).clamp(0, 100) as u8;
    }

    pub fn apply_defense_budget_delta(&mut self, delta: i32) {
        self.defense_budget = (self.defense_budget as i32 + delta).clamp(0, 100) as u8;
    }

    pub fn apply_trust_delta(&mut self, delta: i32) {
        self.trust = (self.trust as i32 + delta).clamp(-100, 100) as i8;
    }

    pub fn set_phase(
        &mut self,
        new_phase: InterrogationPhase,
        turn_id: TurnId,
    ) -> Result<(), PhaseTransitionError> {
        if !self.phase.can_transition_to(new_phase) {
            return Err(PhaseTransitionError::IllegalTransition {
                from: self.phase,
                to: new_phase,
            });
        }
        self.phase = new_phase;
        self.last_transition_turn = Some(turn_id);
        Ok(())
    }

    pub fn exhaust_defense(&mut self, strategy_id: DefenseStrategyId) {
        self.exhausted_defenses.insert(strategy_id);
    }

    pub fn reveal_disclosure(&mut self, disclosure_id: DisclosureId) {
        self.revealed_disclosures.insert(disclosure_id);
    }

    pub fn add_evidence_confrontation(&mut self, evidence_id: EvidenceId) {
        self.confronted_evidence.insert(evidence_id);
    }
}

#[derive(Debug, Clone)]
pub enum PhaseTransitionError {
    IllegalTransition {
        from: InterrogationPhase,
        to: InterrogationPhase,
    },
}

impl std::fmt::Display for PhaseTransitionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PhaseTransitionError::IllegalTransition { from, to } => {
                write!(f, "Illegal phase transition: {from:?} -> {to:?}")
            }
        }
    }
}

impl std::error::Error for PhaseTransitionError {}
