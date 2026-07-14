use narrastate_core::character::{CharacterDefinition, CharacterRuntimeState};
use narrastate_core::evidence::EvidenceDefinition;
use narrastate_core::id::{ClaimId, DisclosureId, EvidenceId, FactId};
use narrastate_core::phase::InterrogationPhase;
use narrastate_core::transition::{
    InterpretedAction, PlayerIntent, TransitionReason, TransitionTuning,
};
use std::collections::{BTreeMap, BTreeSet};

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
    pub unlockable_disclosures: Vec<DisclosureId>,
}

pub struct EvidenceEvaluator {
    tuning: TransitionTuning,
}

impl EvidenceEvaluator {
    pub fn new(tuning: TransitionTuning) -> Self {
        Self { tuning }
    }

    pub fn evaluate(
        &self,
        action: &InterpretedAction,
        state: &CharacterRuntimeState,
        character_def: &CharacterDefinition,
        case_evidence: &BTreeMap<EvidenceId, EvidenceDefinition>,
        _case_facts: &BTreeSet<FactId>,
    ) -> EvaluationResult {
        let confronted = &state.confronted_evidence;
        let mut max_impact = 0.0f32;
        let mut _total_stress_delta = 0i32;
        let mut _total_defense_delta = 0i32;
        let mut _total_composure_delta = 0i32;
        let mut impact_details: Option<EvidenceImpact> = None;
        let mut newly_contradicted: Vec<ClaimId> = Vec::new();
        let mut seen_evidence = BTreeSet::new();

        let trust_sign = match action.tone {
            narrastate_core::PlayerTone::Aggressive | narrastate_core::PlayerTone::Accusatory => -1,
            narrastate_core::PlayerTone::Friendly => 1,
            _ => 0,
        };
        let trust_abs = match action.tone {
            narrastate_core::PlayerTone::Aggressive => 5,
            narrastate_core::PlayerTone::Accusatory => 8,
            narrastate_core::PlayerTone::Friendly => 3,
            _ => 0,
        };
        let total_trust_delta = (trust_sign * trust_abs)
            .clamp(self.tuning.trust_range_min, self.tuning.trust_range_max);

        for usage in &action.evidence_usage {
            let ev_id = &usage.evidence_id;
            if !seen_evidence.insert(ev_id.clone()) {
                continue;
            }

            let Some(ev_def) = case_evidence.get(ev_id) else {
                continue;
            };

            let already_confronted = confronted.contains(ev_id);

            let proposition_match = if ev_def.supports.is_empty() { 0.5 } else { 1.0 };

            let base_strength = self.tuning.reliability_weight * ev_def.reliability
                + self.tuning.directness_weight * ev_def.directness
                + self.tuning.exclusivity_weight * ev_def.exclusivity
                + self.tuning.proposition_match_weight * proposition_match;

            let novelty_multiplier = if already_confronted {
                self.tuning.novelty_multiplier_repeat
            } else {
                self.tuning.novelty_multiplier_first
            };

            let is_relevant = ev_def
                .contradicts
                .iter()
                .any(|cid| character_def.claims.iter().any(|c| &c.id == cid));
            let relevance_multiplier = if is_relevant { 1.0 } else { 0.2 };

            let chain_bonus = if ev_def.contradicts.iter().any(|cid| {
                character_def.claims.iter().any(|c| {
                    &c.id == cid
                        && state
                            .spoken_claims
                            .iter()
                            .any(|sc| sc.claim_id == c.id && !sc.invalidated)
                })
            }) {
                self.tuning.chain_bonus
            } else {
                0.0
            };

            let interpretation_multiplier = action.confidence.clamp(
                self.tuning.min_interpretation_multiplier,
                self.tuning.max_interpretation_multiplier,
            );

            let impact = ((base_strength * novelty_multiplier + chain_bonus).clamp(0.0, 1.0))
                * interpretation_multiplier
                * relevance_multiplier;

            let stress_delta = (impact
                * (self.tuning.stress_per_impact_base
                    - character_def.resilience as f32 * self.tuning.stress_resilience_reduction))
                .round() as i32;
            let defense_delta = (impact * self.tuning.defense_per_impact).round() as i32;
            let composure_delta = (impact * self.tuning.composure_per_impact).round() as i32;

            _total_stress_delta += stress_delta;
            _total_defense_delta += defense_delta;
            _total_composure_delta += composure_delta;

            if impact > max_impact {
                max_impact = impact;
                impact_details = Some(EvidenceImpact {
                    base_strength,
                    novelty_multiplier,
                    chain_bonus,
                    final_impact: impact,
                    stress_delta,
                    defense_delta,
                    composure_delta,
                    trust_delta: total_trust_delta,
                });
            }

            for cid in &ev_def.contradicts {
                if character_def.claims.iter().any(|c| &c.id == cid)
                    && !newly_contradicted.contains(cid)
                {
                    newly_contradicted.push(cid.clone());
                }
            }
        }

        let reason = if !newly_contradicted.is_empty() {
            Some(TransitionReason::PriorClaimContradicted)
        } else if !action.evidence_usage.is_empty() {
            Some(TransitionReason::NewEvidencePresented)
        } else if action.intent == PlayerIntent::Challenge {
            Some(TransitionReason::DirectChallenge)
        } else {
            None
        };

        let projected_stress = (state.stress as i32 + _total_stress_delta).clamp(0, 100) as u8;
        let projected_defense =
            (state.defense_budget as i32 - _total_defense_delta).clamp(0, 100) as u8;

        let proposed_phase = self.determine_proposed_phase(
            projected_stress,
            projected_defense,
            state,
            character_def,
            &newly_contradicted,
        );

        let unlockable = self.find_unlockable_disclosures(state, character_def);

        EvaluationResult {
            impact: impact_details,
            newly_contradicted_claims: newly_contradicted,
            transition_reason: reason,
            proposed_phase,
            unlockable_disclosures: unlockable,
        }
    }

    fn determine_proposed_phase(
        &self,
        stress: u8,
        defense: u8,
        state: &CharacterRuntimeState,
        character_def: &CharacterDefinition,
        newly_contradicted: &[ClaimId],
    ) -> InterrogationPhase {
        use InterrogationPhase::*;

        let undermined_claims = character_def
            .claims
            .iter()
            .filter(|c| {
                c.invalidated_by
                    .iter()
                    .any(|eid| state.confronted_evidence.contains(eid))
            })
            .count();
        let total_cracks = undermined_claims + newly_contradicted.len();

        let mut candidate = state.phase;

        if (stress >= 15 || total_cracks > 0) && candidate < Guarded {
            candidate = Guarded;
        }
        if (stress >= 30 || total_cracks >= 1) && candidate < Defensive {
            candidate = Defensive;
        }
        if stress >= 50 && defense <= 65 && candidate < Pressured {
            candidate = Pressured;
        }
        if stress >= 70 && total_cracks >= 2 && candidate < Cornered {
            candidate = Cornered;
        }
        if stress >= 85 && total_cracks >= 2 && candidate < ConfessionEligible {
            candidate = ConfessionEligible;
        }

        if candidate > state.phase && state.phase.can_transition_to(candidate) {
            candidate
        } else {
            state.phase
        }
    }

    fn find_unlockable_disclosures(
        &self,
        state: &CharacterRuntimeState,
        character_def: &CharacterDefinition,
    ) -> Vec<DisclosureId> {
        let graph = &character_def.disclosure_graph;
        let mut unlockable = Vec::new();
        let mut major_unlocked_this_turn = false;

        for node in &graph.nodes {
            if state.revealed_disclosures.contains(&node.id) {
                continue;
            }

            if graph.major_disclosure_kinds().contains(&node.kind) && major_unlocked_this_turn {
                continue;
            }

            if graph.is_unlockable(&node.id, &state.revealed_disclosures, state.phase) {
                unlockable.push(node.id.clone());
                if graph.major_disclosure_kinds().contains(&node.kind) {
                    major_unlocked_this_turn = true;
                }
            }
        }

        unlockable
    }
}
