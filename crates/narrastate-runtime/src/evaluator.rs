use narrastate_core::character::{CharacterDefinition, CharacterRuntimeState};
use narrastate_core::evidence::{CaseElement, EvidenceDefinition};
use narrastate_core::id::{ClaimId, EvidenceId};
use narrastate_core::phase::InterrogationPhase;
use narrastate_core::transition::{
    InterpretedAction, PlayerIntent, TransitionReason, TransitionTuning,
};
use std::collections::{BTreeMap, BTreeSet};

const CONTRADICTION_CONFIDENCE: f32 = 0.6;

#[derive(Debug, Clone)]
pub struct EvidenceImpact {
    pub base_strength: f32,
    pub novelty_multiplier: f32,
    pub chain_bonus: f32,
    pub final_impact: f32,
    pub stress_delta: i32,
    pub defense_delta: i32,
    pub composure_delta: i32,
    pub trust_delta: i32,
}

#[derive(Debug, Clone)]
pub struct EvaluationResult {
    pub impact: Option<EvidenceImpact>,
    pub newly_contradicted_claims: Vec<ClaimId>,
    pub transition_reason: Option<TransitionReason>,
    pub proposed_phase: InterrogationPhase,
}

pub struct EvidenceEvaluator {
    tuning: TransitionTuning,
}

impl EvidenceEvaluator {
    pub fn new(tuning: TransitionTuning) -> Self {
        Self { tuning }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn evaluate(
        &self,
        action: &InterpretedAction,
        state: &CharacterRuntimeState,
        character: &CharacterDefinition,
        evidence: &BTreeMap<EvidenceId, EvidenceDefinition>,
        required_elements: &BTreeSet<CaseElement>,
    ) -> EvaluationResult {
        let mut used = BTreeSet::new();
        let mut total_impact = 0.0f32;
        let mut total_base = 0.0f32;
        let mut total_chain = 0.0f32;
        let mut any_novel = false;
        let mut impactful_pieces = 0usize;
        let mut newly_contradicted = BTreeSet::new();

        let previously_invalidated = invalidated_claims(character, &state.confronted_evidence);
        for usage in &action.evidence_usage {
            if !used.insert(usage.evidence_id.clone()) {
                continue;
            }
            let Some(item) = evidence.get(&usage.evidence_id) else {
                continue;
            };
            let novel = !state.confronted_evidence.contains(&item.id);
            let novelty = if novel {
                self.tuning.novelty_multiplier_first
            } else {
                self.tuning.novelty_multiplier_repeat
            };
            let relevant_claims: Vec<_> = item
                .contradicts
                .iter()
                .filter(|claim| character.claims.iter().any(|defined| &defined.id == *claim))
                .collect();
            let explicitly_mapped = relevant_claims.iter().any(|claim| {
                action.referenced_claims.contains(claim)
                    || (action.intent == PlayerIntent::PresentEvidence
                        && action.confidence >= CONTRADICTION_CONFIDENCE)
            });
            let relevant = !relevant_claims.is_empty();
            let proposition_match = if explicitly_mapped { 1.0 } else { 0.0 };
            let base = self.tuning.reliability_weight * item.reliability
                + self.tuning.directness_weight * item.directness
                + self.tuning.exclusivity_weight * item.exclusivity
                + self.tuning.proposition_match_weight * proposition_match;
            let chain = if novel
                && explicitly_mapped
                && relevant_claims.iter().any(|claim| {
                    state
                        .spoken_claims
                        .iter()
                        .any(|spoken| &spoken.claim_id == *claim && !spoken.invalidated)
                }) {
                self.tuning.chain_bonus
            } else {
                0.0
            };
            let confidence = action.confidence.clamp(
                self.tuning.min_interpretation_multiplier,
                self.tuning.max_interpretation_multiplier,
            );
            let relevance = if relevant { 1.0 } else { 0.2 };
            let impact = ((base * novelty + chain).clamp(0.0, 1.0)) * confidence * relevance;
            total_impact += impact;
            total_base += base;
            total_chain += chain;
            any_novel |= novel;
            if impact > 0.0 {
                impactful_pieces = impactful_pieces.saturating_add(1);
            }

            if novel && explicitly_mapped && action.confidence >= CONTRADICTION_CONFIDENCE {
                for claim in relevant_claims {
                    if !previously_invalidated.contains(claim) {
                        newly_contradicted.insert((*claim).clone());
                    }
                }
            }
        }

        total_impact = clamp_turn_impact(
            total_impact,
            impactful_pieces,
            self.tuning.max_impact_per_turn,
            self.tuning.turn_impact_ceiling_step,
        );
        let trust_delta = match action.tone {
            narrastate_core::PlayerTone::Aggressive => -5,
            narrastate_core::PlayerTone::Accusatory => -8,
            narrastate_core::PlayerTone::Friendly => 3,
            _ => 0,
        }
        .clamp(self.tuning.trust_range_min, self.tuning.trust_range_max);
        let impact = (total_impact > 0.0).then(|| EvidenceImpact {
            base_strength: total_base.clamp(0.0, 1.0),
            novelty_multiplier: if any_novel {
                self.tuning.novelty_multiplier_first
            } else {
                self.tuning.novelty_multiplier_repeat
            },
            chain_bonus: total_chain.clamp(0.0, 1.0),
            final_impact: total_impact,
            stress_delta: (total_impact
                * (self.tuning.stress_per_impact_base
                    - character.resilience as f32 * self.tuning.stress_resilience_reduction))
                .round() as i32,
            defense_delta: (total_impact * self.tuning.defense_per_impact).round() as i32,
            composure_delta: (total_impact * self.tuning.composure_per_impact).round() as i32,
            trust_delta,
        });

        let projected_stress = (state.stress as i32
            + impact.as_ref().map_or(0, |value| value.stress_delta))
        .clamp(0, 100) as u8;
        let projected_defense = (state.defense_budget as i32
            - impact.as_ref().map_or(0, |value| value.defense_delta))
        .clamp(0, 100) as u8;
        let mut confronted = state.confronted_evidence.clone();
        confronted.extend(used);
        let invalidated = invalidated_claims(character, &confronted);
        let coverage = covered_elements(&confronted, evidence);
        let has_action_disclosure = state.revealed_disclosures.iter().any(|id| {
            character.disclosure_graph.nodes.iter().any(|node| {
                &node.id == id
                    && matches!(
                        node.kind,
                        narrastate_core::DisclosureKind::PartialAction
                            | narrastate_core::DisclosureKind::FullAction
                            | narrastate_core::DisclosureKind::Intent
                    )
            })
        });
        let proposed_phase = next_phase(
            state.phase,
            projected_stress,
            projected_defense,
            invalidated.len(),
            required_elements.is_subset(&coverage),
            has_action_disclosure,
            character.disclosure_graph.confession_node().is_some(),
            action.intent,
        );
        let transition_reason = if !newly_contradicted.is_empty() {
            Some(TransitionReason::PriorClaimContradicted)
        } else if impact.is_some() {
            Some(TransitionReason::NewEvidencePresented)
        } else if !action.evidence_usage.is_empty() {
            Some(TransitionReason::RepeatedQuestionNoNewInformation)
        } else if action.intent == PlayerIntent::Challenge {
            Some(TransitionReason::DirectChallenge)
        } else if action.intent == PlayerIntent::Accuse {
            Some(TransitionReason::AccusationSubmitted)
        } else {
            None
        };

        EvaluationResult {
            impact,
            newly_contradicted_claims: newly_contradicted.into_iter().collect(),
            transition_reason,
            proposed_phase,
        }
    }
}

pub fn invalidated_claims(
    character: &CharacterDefinition,
    confronted: &BTreeSet<EvidenceId>,
) -> BTreeSet<ClaimId> {
    character
        .claims
        .iter()
        .filter(|claim| {
            claim
                .invalidated_by
                .iter()
                .any(|id| confronted.contains(id))
        })
        .map(|claim| claim.id.clone())
        .collect()
}

pub fn covered_elements(
    confronted: &BTreeSet<EvidenceId>,
    evidence: &BTreeMap<EvidenceId, EvidenceDefinition>,
) -> BTreeSet<CaseElement> {
    confronted
        .iter()
        .filter_map(|id| evidence.get(id))
        .flat_map(|item| item.elements.iter().copied())
        .collect()
}

/// Bounds how much pressure one player turn can deliver.
///
/// A turn that presents a single clue stays capped at `1.0`. Every further
/// clue that actually lands raises the ceiling by `step`, up to `max`, so a
/// player who attaches their whole evidence list to one question still advances
/// the interrogation instead of hitting an invisible wall.
pub fn clamp_turn_impact(raw: f32, impactful_pieces: usize, max: f32, step: f32) -> f32 {
    if impactful_pieces <= 1 {
        return raw.clamp(0.0, 1.0);
    }
    let ceiling = (1.0 + step * (impactful_pieces.saturating_sub(1)) as f32).min(max);
    raw.clamp(0.0, ceiling)
}

#[allow(clippy::too_many_arguments)]
fn next_phase(
    current: InterrogationPhase,
    stress: u8,
    defense: u8,
    cracks: usize,
    elements_complete: bool,
    action_disclosed: bool,
    has_confession_path: bool,
    intent: PlayerIntent,
) -> InterrogationPhase {
    use InterrogationPhase::*;
    let decisive = elements_complete && action_disclosed && intent == PlayerIntent::Challenge;
    match current {
        Calm
            if stress >= CALM_TO_GUARDED_STRESS
                || cracks > 0
                || matches!(intent, PlayerIntent::Accuse) =>
        {
            Guarded
        }
        Guarded if stress >= GUARDED_TO_DEFENSIVE_STRESS || cracks >= 1 || decisive => Defensive,
        Defensive
            if (stress >= DEFENSIVE_TO_PRESSURED_STRESS
                && defense <= DEFENSIVE_TO_PRESSURED_DEFENSE)
                || cracks >= 2
                || decisive =>
        {
            Pressured
        }
        Pressured
            if (stress >= PRESSURED_TO_CORNERED_STRESS
                && defense <= PRESSURED_TO_CORNERED_DEFENSE)
                || (stress >= PRESSURED_TO_CORNERED_FALLBACK_STRESS && cracks >= 2)
                || decisive =>
        {
            Cornered
        }
        Cornered if has_confession_path && elements_complete && action_disclosed => {
            ConfessionEligible
        }
        _ => current,
    }
}

/// Stress needed to move out of Calm once the player starts pressing.
const CALM_TO_GUARDED_STRESS: u8 = 10;
/// Stress needed to reach Defensive.
const GUARDED_TO_DEFENSIVE_STRESS: u8 = 25;
/// Stress + remaining defence budget needed to reach Pressured.
const DEFENSIVE_TO_PRESSURED_STRESS: u8 = 40;
const DEFENSIVE_TO_PRESSURED_DEFENSE: u8 = 70;
/// Stress + remaining defence budget needed to reach Cornered.
const PRESSURED_TO_CORNERED_STRESS: u8 = 55;
const PRESSURED_TO_CORNERED_DEFENSE: u8 = 50;
/// Higher stress path into Cornered for resilient characters with a low budget.
const PRESSURED_TO_CORNERED_FALLBACK_STRESS: u8 = 70;

#[cfg(test)]
mod tests {
    use super::next_phase;
    use super::{
        CALM_TO_GUARDED_STRESS, DEFENSIVE_TO_PRESSURED_DEFENSE, DEFENSIVE_TO_PRESSURED_STRESS,
        PRESSURED_TO_CORNERED_DEFENSE, PRESSURED_TO_CORNERED_FALLBACK_STRESS,
        PRESSURED_TO_CORNERED_STRESS,
    };
    use narrastate_core::{InterrogationPhase, PlayerIntent};

    #[test]
    fn decisive_follow_up_advances_one_phase_after_evidence_and_action_are_complete() {
        let next = next_phase(
            InterrogationPhase::Defensive,
            0,
            100,
            0,
            true,
            true,
            true,
            PlayerIntent::Challenge,
        );

        assert_eq!(next, InterrogationPhase::Pressured);
    }

    #[test]
    fn decisive_follow_up_cannot_advance_without_a_disclosed_action() {
        let next = next_phase(
            InterrogationPhase::Defensive,
            0,
            100,
            0,
            true,
            false,
            true,
            PlayerIntent::Challenge,
        );

        assert_eq!(next, InterrogationPhase::Defensive);
    }

    #[test]
    fn decisive_follow_up_can_advance_from_guarded_without_skipping_defensive() {
        let next = next_phase(
            InterrogationPhase::Guarded,
            0,
            100,
            0,
            true,
            true,
            true,
            PlayerIntent::Challenge,
        );

        assert_eq!(next, InterrogationPhase::Defensive);
    }

    #[test]
    fn calm_opens_up_under_modest_pressure() {
        assert_eq!(
            next_phase(
                InterrogationPhase::Calm,
                CALM_TO_GUARDED_STRESS,
                100,
                0,
                false,
                false,
                true,
                PlayerIntent::Ask,
            ),
            InterrogationPhase::Guarded
        );
        assert_eq!(
            next_phase(
                InterrogationPhase::Calm,
                CALM_TO_GUARDED_STRESS - 1,
                100,
                0,
                false,
                false,
                true,
                PlayerIntent::Ask,
            ),
            InterrogationPhase::Calm
        );
    }

    #[test]
    fn pressured_phase_is_reachable_with_weakened_defence_only() {
        assert_eq!(
            next_phase(
                InterrogationPhase::Defensive,
                DEFENSIVE_TO_PRESSURED_STRESS,
                DEFENSIVE_TO_PRESSURED_DEFENSE,
                0,
                false,
                false,
                true,
                PlayerIntent::Ask,
            ),
            InterrogationPhase::Pressured
        );
        assert_eq!(
            next_phase(
                InterrogationPhase::Defensive,
                PRESSURED_TO_CORNERED_FALLBACK_STRESS,
                100,
                2,
                false,
                false,
                true,
                PlayerIntent::Ask,
            ),
            InterrogationPhase::Pressured
        );
    }

    #[test]
    fn cornered_phase_is_reachable_without_a_full_stress_cap() {
        assert_eq!(
            next_phase(
                InterrogationPhase::Pressured,
                PRESSURED_TO_CORNERED_STRESS,
                PRESSURED_TO_CORNERED_DEFENSE,
                0,
                false,
                false,
                true,
                PlayerIntent::Ask,
            ),
            InterrogationPhase::Cornered
        );
        assert_eq!(
            next_phase(
                InterrogationPhase::Pressured,
                PRESSURED_TO_CORNERED_STRESS,
                100,
                0,
                false,
                false,
                true,
                PlayerIntent::Ask,
            ),
            InterrogationPhase::Pressured
        );
    }

    #[test]
    fn confession_eligible_still_requires_the_full_evidence_and_action_gate() {
        assert_eq!(
            next_phase(
                InterrogationPhase::Cornered,
                100,
                0,
                2,
                true,
                false,
                true,
                PlayerIntent::Challenge,
            ),
            InterrogationPhase::Cornered
        );
        assert_eq!(
            next_phase(
                InterrogationPhase::Cornered,
                100,
                0,
                2,
                true,
                true,
                false,
                PlayerIntent::Challenge,
            ),
            InterrogationPhase::Cornered
        );
        assert_eq!(
            next_phase(
                InterrogationPhase::Cornered,
                100,
                0,
                2,
                true,
                true,
                true,
                PlayerIntent::Challenge,
            ),
            InterrogationPhase::ConfessionEligible
        );
    }

    #[test]
    fn turn_impact_ceiling_grows_with_the_number_of_clues_presented() {
        use super::clamp_turn_impact;

        assert_eq!(clamp_turn_impact(4.0, 1, 3.0, 0.5), 1.0);
        assert_eq!(clamp_turn_impact(4.0, 2, 3.0, 0.5), 1.5);
        assert_eq!(clamp_turn_impact(4.0, 3, 3.0, 0.5), 2.0);
        assert_eq!(clamp_turn_impact(9.0, 9, 3.0, 0.5), 3.0);
        assert_eq!(clamp_turn_impact(0.0, 5, 3.0, 0.5), 0.0);
    }
}
