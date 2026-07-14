use narrastate_core::character::{CharacterDefinition, CharacterRuntimeState};
use narrastate_core::disclosure::{DisclosureKind, DisclosurePrerequisite};
use narrastate_core::evidence::{CaseElement, EvidenceDefinition};
use narrastate_core::id::{ClaimId, DisclosureId, EvidenceId, FactId, TurnId};
use narrastate_core::phase::InterrogationPhase;
use narrastate_core::transition::{
    InterpretedAction, PlayerIntent, TransitionReason, TransitionTuning,
};
use std::collections::{BTreeMap, BTreeSet};

use crate::evaluator::{covered_elements, invalidated_claims, EvidenceEvaluator};

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

    #[allow(clippy::too_many_arguments)]
    pub fn process(
        &self,
        action: &InterpretedAction,
        state: &mut CharacterRuntimeState,
        character: &CharacterDefinition,
        evidence: &BTreeMap<EvidenceId, EvidenceDefinition>,
        _facts: &BTreeSet<FactId>,
        turn_id: TurnId,
    ) -> TransitionResult {
        let required = evidence
            .values()
            .flat_map(|item| item.elements.iter().copied())
            .collect();
        self.process_with_requirements(action, state, character, evidence, &required, turn_id)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn process_with_requirements(
        &self,
        action: &InterpretedAction,
        state: &mut CharacterRuntimeState,
        character: &CharacterDefinition,
        evidence: &BTreeMap<EvidenceId, EvidenceDefinition>,
        required_elements: &BTreeSet<CaseElement>,
        turn_id: TurnId,
    ) -> TransitionResult {
        let phase_before = state.phase;
        let stress_before = state.stress;
        let composure_before = state.composure;
        let defense_budget_before = state.defense_budget;
        let trust_before = state.trust;
        let evaluation =
            self.evaluator
                .evaluate(action, state, character, evidence, required_elements);

        if let Some(impact) = &evaluation.impact {
            state.apply_stress_delta(impact.stress_delta);
            state.apply_composure_delta(-impact.composure_delta);
            state.apply_defense_budget_delta(-impact.defense_delta);
            state.apply_trust_delta(impact.trust_delta);
        }
        for usage in &action.evidence_usage {
            if evidence.contains_key(&usage.evidence_id) {
                state.add_evidence_confrontation(usage.evidence_id.clone());
            }
        }
        for spoken in &mut state.spoken_claims {
            if evaluation
                .newly_contradicted_claims
                .contains(&spoken.claim_id)
            {
                spoken.invalidated = true;
            }
        }
        if evaluation.proposed_phase != state.phase {
            state
                .set_phase(evaluation.proposed_phase, turn_id)
                .expect("evaluator only proposes an adjacent legal phase");
        }

        let invalidated = invalidated_claims(character, &state.confronted_evidence);
        let coverage = covered_elements(&state.confronted_evidence, evidence);
        let effective_turn = evaluation.impact.is_some()
            || !evaluation.newly_contradicted_claims.is_empty()
            || (state.phase == InterrogationPhase::ConfessionEligible
                && matches!(
                    action.intent,
                    PlayerIntent::Accuse | PlayerIntent::Challenge | PlayerIntent::PresentEvidence
                ));
        let newly_revealed = self.unlock_one(
            action,
            state,
            character,
            &invalidated,
            required_elements.is_subset(&coverage),
            effective_turn,
        );
        let transition_reason = if newly_revealed.is_some() {
            Some(TransitionReason::DisclosurePrerequisitesMet)
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
                newly_revealed_disclosures: newly_revealed.into_iter().collect(),
                transition_reason,
            },
            contradictory_claims: evaluation.newly_contradicted_claims,
            transition_reason,
        }
    }

    fn unlock_one(
        &self,
        action: &InterpretedAction,
        state: &mut CharacterRuntimeState,
        character: &CharacterDefinition,
        invalidated_claims: &BTreeSet<ClaimId>,
        elements_complete: bool,
        effective_turn: bool,
    ) -> Option<DisclosureId> {
        if !effective_turn {
            return None;
        }
        let graph = &character.disclosure_graph;
        let candidate = graph.nodes.iter().find(|node| {
            if node.kind == DisclosureKind::Confession
                && (!elements_complete
                    || state.phase != InterrogationPhase::ConfessionEligible
                    || !matches!(
                        action.intent,
                        PlayerIntent::Accuse
                            | PlayerIntent::Challenge
                            | PlayerIntent::PresentEvidence
                    ))
            {
                return false;
            }
            graph.is_unlockable_with_context(
                &node.id,
                &state.revealed_disclosures,
                state.phase,
                &state.confronted_evidence,
                invalidated_claims,
            ) && node
                .prerequisites
                .iter()
                .all(|prerequisite| match prerequisite {
                    DisclosurePrerequisite::EvidencePresented { evidence } => evidence
                        .iter()
                        .all(|id| state.confronted_evidence.contains(id)),
                    _ => true,
                })
        })?;
        let id = candidate.id.clone();
        state.reveal_disclosure(id.clone());
        Some(id)
    }
}
