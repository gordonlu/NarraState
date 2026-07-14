use narrastate_core::character::{CharacterDefinition, CharacterRuntimeState};
use narrastate_core::evidence::EvidenceDefinition;
use narrastate_core::id::{ClaimId, DisclosureId, EvidenceId, FactId, TurnId};
use narrastate_core::phase::InterrogationPhase;
use narrastate_core::transition::{InterpretedAction, TransitionReason, TransitionTuning};
use std::collections::{BTreeMap, BTreeSet};

use crate::evaluator::EvidenceEvaluator;

#[derive(Debug, Clone)]
pub struct StateDiff {
    pub stress_before: u8,
    pub stress_after: u8,
    pub composure_before: u8,
    pub composure_after: u8,
    pub defense_budget_before: u8,
    pub defense_budget_after: u8,
    pub trust_before: i8,
    pub trust_after: i8,
    pub phase_before: InterrogationPhase,
    pub phase_after: InterrogationPhase,
    pub newly_revealed_disclosures: Vec<DisclosureId>,
    pub transition_reason: Option<TransitionReason>,
}

#[derive(Debug, Clone)]
pub struct TransitionResult {
    pub diff: StateDiff,
    pub contradictory_claims: Vec<ClaimId>,
    pub transition_reason: Option<TransitionReason>,
}

pub struct TransitionEngine {
    evaluator: EvidenceEvaluator,
}

impl TransitionEngine {
    pub fn new(tuning: TransitionTuning) -> Self {
        Self {
            evaluator: EvidenceEvaluator::new(tuning),
        }
    }

    pub fn process(
        &self,
        action: &InterpretedAction,
        state: &mut CharacterRuntimeState,
        character_def: &CharacterDefinition,
        case_evidence: &BTreeMap<EvidenceId, EvidenceDefinition>,
        case_facts: &BTreeSet<FactId>,
        turn_id: TurnId,
    ) -> TransitionResult {
        let phase_before = state.phase;
        let stress_before = state.stress;
        let composure_before = state.composure;
        let defense_budget_before = state.defense_budget;
        let trust_before = state.trust;

        let evaluation =
            self.evaluator
                .evaluate(action, state, character_def, case_evidence, case_facts);

        if let Some(impact) = &evaluation.impact {
            state.apply_stress_delta(impact.stress_delta);
            state.apply_composure_delta(-impact.composure_delta);
            state.apply_defense_budget_delta(-impact.defense_delta);
            state.apply_trust_delta(impact.trust_delta);

            for usage in &action.evidence_usage {
                state.add_evidence_confrontation(usage.evidence_id.clone());
            }
        }

        if evaluation.proposed_phase != state.phase {
            if let Err(e) = state.set_phase(evaluation.proposed_phase, turn_id) {
                tracing::warn!(
                    "Phase transition rejected: {e:?} (from {:?} to {:?})",
                    state.phase,
                    evaluation.proposed_phase
                );
            }
        }

        let mut newly_revealed = Vec::new();
        for did in &evaluation.unlockable_disclosures {
            if !state.revealed_disclosures.contains(did) {
                state.reveal_disclosure(did.clone());
                newly_revealed.push(did.clone());
            }
        }

        let transition_reason = if state.phase != phase_before {
            evaluation
                .transition_reason
                .or(Some(TransitionReason::DisclosurePrerequisitesMet))
        } else {
            evaluation.transition_reason
        };

        TransitionResult {
            diff: StateDiff {
                stress_before,
                stress_after: state.stress,
                composure_before,
                composure_after: state.composure,
                defense_budget_before,
                defense_budget_after: state.defense_budget,
                trust_before,
                trust_after: state.trust,
                phase_before,
                phase_after: state.phase,
                newly_revealed_disclosures: newly_revealed,
                transition_reason,
            },
            contradictory_claims: evaluation.newly_contradicted_claims,
            transition_reason,
        }
    }
}
