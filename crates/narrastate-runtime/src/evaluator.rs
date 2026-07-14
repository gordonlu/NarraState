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

            if novel && explicitly_mapped && action.confidence >= CONTRADICTION_CONFIDENCE {
                for claim in relevant_claims {
                    if !previously_invalidated.contains(claim) {
                        newly_contradicted.insert((*claim).clone());
                    }
                }
            }
        }

        total_impact = total_impact.clamp(0.0, 1.0);
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
    match current {
        Calm if stress >= 15 || cracks > 0 || matches!(intent, PlayerIntent::Accuse) => Guarded,
        Guarded if stress >= 30 || cracks >= 1 => Defensive,
        Defensive if stress >= 50 && defense <= 65 => Pressured,
        Pressured if stress >= 70 && cracks >= 2 => Cornered,
        Cornered if has_confession_path && elements_complete && action_disclosed => {
            ConfessionEligible
        }
        _ => current,
    }
}
