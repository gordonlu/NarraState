use narrastate_core::character::{CharacterDefinition, CharacterRuntimeState};
use narrastate_core::disclosure::{DialogueAct, DisclosureKind};
use narrastate_core::evidence::EvidenceDefinition;
use narrastate_core::id::{ClaimId, DefenseStrategyId, DisclosureId, EvidenceId, FactId};
use narrastate_core::strategy::DefenseStrategyKind;
use narrastate_core::transition::{InterpretedAction, PlayerIntent};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone)]
pub struct DialoguePlan {
    pub act: DialogueAct,
    pub strategy: Option<DefenseStrategyId>,
    pub allowed_claims: Vec<ClaimId>,
    pub allowed_facts: Vec<FactId>,
    pub newly_revealed: Option<DisclosureId>,
    pub forbidden_facts: Vec<FactId>,
}

pub struct DialoguePlanner;

impl DialoguePlanner {
    pub fn plan(
        &self,
        action: &InterpretedAction,
        state: &CharacterRuntimeState,
        character: &CharacterDefinition,
        evidence: &BTreeMap<EvidenceId, EvidenceDefinition>,
    ) -> DialoguePlan {
        let mut state = state.clone();
        self.plan_with_context(
            action,
            &mut state,
            character,
            evidence,
            &BTreeSet::new(),
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn plan_with_context(
        &self,
        action: &InterpretedAction,
        state: &mut CharacterRuntimeState,
        character: &CharacterDefinition,
        _evidence: &BTreeMap<EvidenceId, EvidenceDefinition>,
        player_known_facts: &BTreeSet<FactId>,
        newly_revealed: Option<&DisclosureId>,
    ) -> DialoguePlan {
        let allowed_claims = self.allowed_claims(state, character);
        let allowed_facts = self.allowed_facts(state, character, player_known_facts);
        let forbidden_facts = character
            .knowledge
            .iter()
            .filter(|id| !allowed_facts.contains(id))
            .cloned()
            .collect();

        if action.intent == PlayerIntent::Unknown || action.confidence < 0.5 {
            return DialoguePlan {
                act: DialogueAct::AskForClarification,
                strategy: None,
                allowed_claims,
                allowed_facts,
                newly_revealed: None,
                forbidden_facts,
            };
        }

        if let Some(id) = newly_revealed {
            if let Some(node) = character
                .disclosure_graph
                .nodes
                .iter()
                .find(|node| &node.id == id)
            {
                return DialoguePlan {
                    act: if node.kind == DisclosureKind::Confession {
                        DialogueAct::FullAdmission
                    } else {
                        node.response_intent
                    },
                    strategy: None,
                    allowed_claims,
                    allowed_facts,
                    newly_revealed: Some(id.clone()),
                    forbidden_facts,
                };
            }
        }

        if !action.evidence_usage.is_empty()
            || matches!(
                action.intent,
                PlayerIntent::Challenge | PlayerIntent::Accuse
            )
        {
            if let Some(strategy) = character.defenses.iter().find(|strategy| {
                strategy.usable_phases.contains(&state.phase)
                    && !state.exhausted_defenses.contains(&strategy.id)
                    && (strategy.applicable_claims.is_empty()
                        || action
                            .referenced_claims
                            .iter()
                            .any(|claim| strategy.applicable_claims.contains(claim)))
            }) {
                state.use_defense(&strategy.id, strategy.max_uses);
                let act = match strategy.kind {
                    DefenseStrategyKind::Denial => DialogueAct::Deny,
                    DefenseStrategyKind::MemoryGap => DialogueAct::Evade,
                    DefenseStrategyKind::InnocentExplanation => DialogueAct::Answer,
                    DefenseStrategyKind::EvidenceChallenge => DialogueAct::ChallengeEvidence,
                    DefenseStrategyKind::MinimizeResponsibility => DialogueAct::Reframe,
                    DefenseStrategyKind::ShiftBlame => DialogueAct::ShiftBlame,
                    DefenseStrategyKind::EmotionalAppeal => DialogueAct::AskForClarification,
                    DefenseStrategyKind::Silence => DialogueAct::Silence,
                };
                return DialoguePlan {
                    act,
                    strategy: Some(strategy.id.clone()),
                    allowed_claims,
                    allowed_facts,
                    newly_revealed: None,
                    forbidden_facts,
                };
            }
        }

        DialoguePlan {
            act: DialogueAct::Answer,
            strategy: state.active_strategy.clone(),
            allowed_claims,
            allowed_facts,
            newly_revealed: None,
            forbidden_facts,
        }
    }

    fn allowed_claims(
        &self,
        state: &CharacterRuntimeState,
        character: &CharacterDefinition,
    ) -> Vec<ClaimId> {
        character
            .claims
            .iter()
            .filter(|claim| {
                claim.available_from <= state.phase
                    && !claim
                        .invalidated_by
                        .iter()
                        .any(|id| state.confronted_evidence.contains(id))
            })
            .map(|claim| claim.id.clone())
            .collect()
    }

    fn allowed_facts(
        &self,
        state: &CharacterRuntimeState,
        character: &CharacterDefinition,
        player_known_facts: &BTreeSet<FactId>,
    ) -> Vec<FactId> {
        character
            .knowledge
            .iter()
            .filter(|fact| {
                player_known_facts.contains(*fact)
                    || state.revealed_disclosures.iter().any(|disclosure| {
                        character
                            .disclosure_graph
                            .nodes
                            .iter()
                            .any(|node| &node.id == disclosure && node.reveals.contains(*fact))
                    })
            })
            .cloned()
            .collect()
    }
}
