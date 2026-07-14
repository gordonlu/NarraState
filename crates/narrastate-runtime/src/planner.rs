use narrastate_core::character::{CharacterDefinition, CharacterRuntimeState};
use narrastate_core::disclosure::{DialogueAct, DisclosureGraph, DisclosureNode};
use narrastate_core::evidence::EvidenceDefinition;
use narrastate_core::id::{ClaimId, DefenseStrategyId, DisclosureId, EvidenceId, FactId};
use narrastate_core::phase::InterrogationPhase;
use narrastate_core::strategy::DefenseStrategyKind;
use narrastate_core::transition::InterpretedAction;
use std::collections::BTreeMap;

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
        character_def: &CharacterDefinition,
        case_evidence: &BTreeMap<EvidenceId, EvidenceDefinition>,
    ) -> DialoguePlan {
        // Check for confession eligibility
        if let Some(confession_node) = character_def.disclosure_graph.confession_node() {
            if state.phase >= InterrogationPhase::ConfessionEligible
                && state.revealed_disclosures.contains(&confession_node.id)
            {
                return self.confession_plan(confession_node, character_def, state);
            }
        }

        // If a new disclosure was just unlocked, use its response_intent
        let newly_revealed = self.find_newly_revealed_major(state, &character_def.disclosure_graph);
        if let Some(ref did) = newly_revealed {
            if let Some(node) = character_def
                .disclosure_graph
                .nodes
                .iter()
                .find(|n| &n.id == did)
            {
                let allowed_claims = self.allowed_claims_for_phase(state, character_def);
                let allowed_facts = self.allowed_facts(state, character_def);
                return DialoguePlan {
                    act: node.response_intent,
                    strategy: state.active_strategy.clone(),
                    allowed_claims,
                    allowed_facts,
                    newly_revealed: Some(did.clone()),
                    forbidden_facts: vec![],
                };
            }
        }

        // If player presented evidence or challenged a claim
        if !action.evidence_usage.is_empty()
            || action.intent == narrastate_core::PlayerIntent::Challenge
            || action.intent == narrastate_core::PlayerIntent::Accuse
        {
            return self.defensive_plan(action, state, character_def, case_evidence);
        }

        // Default: answer
        self.answer_plan(state, character_def)
    }

    fn confession_plan(
        &self,
        _confession: &DisclosureNode,
        character_def: &CharacterDefinition,
        _state: &CharacterRuntimeState,
    ) -> DialoguePlan {
        DialoguePlan {
            act: DialogueAct::FullAdmission,
            strategy: None,
            allowed_claims: character_def.claims.iter().map(|c| c.id.clone()).collect(),
            allowed_facts: character_def.knowledge.clone(),
            newly_revealed: None,
            forbidden_facts: vec![],
        }
    }

    fn defensive_plan(
        &self,
        action: &InterpretedAction,
        state: &CharacterRuntimeState,
        character_def: &CharacterDefinition,
        _case_evidence: &BTreeMap<EvidenceId, EvidenceDefinition>,
    ) -> DialoguePlan {
        // Find an available defense strategy
        let strategy = character_def.defenses.iter().find(|d| {
            d.usable_phases.contains(&state.phase)
                && !state.exhausted_defenses.contains(&d.id)
                && (d.applicable_claims.is_empty()
                    || action
                        .referenced_claims
                        .iter()
                        .any(|c| d.applicable_claims.contains(c)))
        });

        if let Some(strat) = strategy {
            let allowed_claims = self.allowed_claims_for_phase(state, character_def);
            let allowed_facts = self.allowed_facts(state, character_def);
            let act = match strat.kind {
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
                strategy: Some(strat.id.clone()),
                allowed_claims,
                allowed_facts,
                newly_revealed: None,
                forbidden_facts: vec![],
            };
        }

        // No valid defense — partial admission if possible
        self.answer_plan(state, character_def)
    }

    fn answer_plan(
        &self,
        state: &CharacterRuntimeState,
        character_def: &CharacterDefinition,
    ) -> DialoguePlan {
        DialoguePlan {
            act: DialogueAct::Answer,
            strategy: state.active_strategy.clone(),
            allowed_claims: self.allowed_claims_for_phase(state, character_def),
            allowed_facts: self.allowed_facts(state, character_def),
            newly_revealed: None,
            forbidden_facts: vec![],
        }
    }

    fn allowed_claims_for_phase(
        &self,
        state: &CharacterRuntimeState,
        character_def: &CharacterDefinition,
    ) -> Vec<ClaimId> {
        character_def
            .claims
            .iter()
            .filter(|c| {
                c.available_from <= state.phase
                    && !c
                        .invalidated_by
                        .iter()
                        .any(|eid| state.confronted_evidence.contains(eid))
            })
            .map(|c| c.id.clone())
            .collect()
    }

    fn allowed_facts(
        &self,
        state: &CharacterRuntimeState,
        character_def: &CharacterDefinition,
    ) -> Vec<FactId> {
        character_def
            .knowledge
            .iter()
            .filter(|fid| {
                state.revealed_disclosures.iter().any(|did| {
                    character_def
                        .disclosure_graph
                        .nodes
                        .iter()
                        .any(|n| &n.id == did && n.reveals.contains(fid))
                })
            })
            .cloned()
            .collect()
    }

    fn find_newly_revealed_major(
        &self,
        _state: &CharacterRuntimeState,
        _graph: &DisclosureGraph,
    ) -> Option<DisclosureId> {
        // In a real pipeline, this is determined by the evaluator.
        // For the planner, we check what's been revealed against the character def.
        // This method is a placeholder — the actual newly revealed comes from the TransitionResult.
        None
    }
}
