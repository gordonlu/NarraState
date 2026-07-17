use narrastate_core::{
    CharacterId, CharacterRuntimeState, ClaimId, CompiledCase, DisclosureId, DiscoveryRule,
    EvidenceId, EvidenceUsageKind, EvidenceUse, InterpretedAction, InterrogationPhase,
    PlayerIntent, PlayerTone, TransitionTuning, TurnId,
};
use narrastate_runtime::{covered_elements, TransitionEngine};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SimulationLimits {
    pub max_states: usize,
    pub max_turns: u32,
    pub max_branching: usize,
    pub timeout_ms: u64,
}

impl Default for SimulationLimits {
    fn default() -> Self {
        Self {
            max_states: 50_000,
            max_turns: 40,
            max_branching: 30,
            timeout_ms: 2_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SimulationAction {
    SelectCharacter(CharacterId),
    PresentEvidence {
        character: CharacterId,
        evidence: EvidenceId,
        claim: Option<ClaimId>,
    },
    ChallengeClaim {
        character: CharacterId,
        claim: ClaimId,
        evidence: EvidenceId,
    },
    AskDecisiveFollowUp(CharacterId),
    SubmitAccusation {
        character: CharacterId,
        evidence: BTreeSet<EvidenceId>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationStep {
    pub turn: u32,
    pub action: SimulationAction,
    pub phase_after: InterrogationPhase,
    pub newly_revealed_disclosures: Vec<DisclosureId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SimulationFailure {
    NoPathToRequiredEvidence,
    DisclosureNodeUnreachable,
    EndingRequirementsUnsatisfiable,
    FalseSuspectCanConfess,
    StateLimit,
    TurnLimit,
    Timeout,
}

impl SimulationFailure {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NoPathToRequiredEvidence => "NO_PATH_TO_REQUIRED_EVIDENCE",
            Self::DisclosureNodeUnreachable => "DISCLOSURE_NODE_UNREACHABLE",
            Self::EndingRequirementsUnsatisfiable => "ENDING_REQUIREMENTS_UNSATISFIABLE",
            Self::FalseSuspectCanConfess => "FALSE_SUSPECT_CAN_CONFESS",
            Self::StateLimit => "SIMULATION_STATE_LIMIT",
            Self::TurnLimit => "SIMULATION_TURN_LIMIT",
            Self::Timeout => "SIMULATION_TIMEOUT",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationResult {
    pub success: bool,
    pub visited_states: usize,
    pub turns: u32,
    pub acquired_evidence_ids: Vec<EvidenceId>,
    pub reached_disclosure_nodes: Vec<DisclosureId>,
    pub failure_reason: Option<SimulationFailure>,
    pub trace: Vec<SimulationStep>,
}

#[derive(Clone)]
struct SearchState {
    runtime: CharacterRuntimeState,
    discovered: BTreeSet<EvidenceId>,
    turns: u32,
    trace: Vec<SimulationStep>,
}

pub fn simulate_case(case: &CompiledCase, limits: SimulationLimits) -> SimulationResult {
    let started = Instant::now();
    if let Some(character) = case.definition.characters.iter().find(|character| {
        character.id != case.responsible_character_id
            && character.disclosure_graph.confession_node().is_some()
    }) {
        return failed(
            SimulationFailure::FalseSuspectCanConfess,
            0,
            0,
            vec![],
            vec![],
            vec![SimulationStep {
                turn: 0,
                action: SimulationAction::SelectCharacter(character.id.clone()),
                phase_after: InterrogationPhase::Calm,
                newly_revealed_disclosures: vec![],
            }],
        );
    }
    let Some(character) = case
        .definition
        .characters
        .iter()
        .find(|character| character.id == case.responsible_character_id)
    else {
        return failed(
            SimulationFailure::EndingRequirementsUnsatisfiable,
            0,
            0,
            vec![],
            vec![],
            vec![],
        );
    };
    let evidence: BTreeMap<_, _> = case
        .definition
        .evidence
        .iter()
        .cloned()
        .map(|item| (item.id.clone(), item))
        .collect();
    let mut discovered = BTreeSet::new();
    discover_available(
        &evidence,
        &BTreeSet::new(),
        InterrogationPhase::Calm,
        &mut discovered,
    );
    let initial = SearchState {
        runtime: CharacterRuntimeState::new(character.resilience),
        discovered,
        turns: 0,
        trace: vec![SimulationStep {
            turn: 0,
            action: SimulationAction::SelectCharacter(character.id.clone()),
            phase_after: InterrogationPhase::Calm,
            newly_revealed_disclosures: vec![],
        }],
    };
    let engine = TransitionEngine::new(TransitionTuning::default());
    let mut queue = VecDeque::from([initial]);
    let mut seen = BTreeSet::new();
    let mut visited = 0_usize;
    let mut saw_evidence_solution = false;
    let mut best: Option<SearchState> = None;
    let mut hit_turn_limit = false;

    while let Some(state) = queue.pop_front() {
        if started.elapsed() > Duration::from_millis(limits.timeout_ms) {
            return from_best(SimulationFailure::Timeout, visited, best);
        }
        if visited >= limits.max_states {
            return from_best(SimulationFailure::StateLimit, visited, best);
        }
        let key = state_key(&state);
        if !seen.insert(key) {
            continue;
        }
        visited += 1;
        if best.as_ref().is_none_or(|current| {
            state.runtime.revealed_disclosures.len() > current.runtime.revealed_disclosures.len()
        }) {
            best = Some(state.clone());
        }

        let coverage = covered_elements(&state.discovered, &evidence);
        let evidence_complete = case.definition.required_case_elements.is_subset(&coverage);
        saw_evidence_solution |= evidence_complete;
        let confession_complete = character
            .disclosure_graph
            .confession_node()
            .is_none_or(|node| state.runtime.revealed_disclosures.contains(&node.id));
        if evidence_complete && confession_complete {
            let mut trace = state.trace.clone();
            trace.push(SimulationStep {
                turn: state.turns.saturating_add(1),
                action: SimulationAction::SubmitAccusation {
                    character: character.id.clone(),
                    evidence: state.discovered.clone(),
                },
                phase_after: state.runtime.phase,
                newly_revealed_disclosures: vec![],
            });
            return SimulationResult {
                success: true,
                visited_states: visited,
                turns: state.turns.saturating_add(1),
                acquired_evidence_ids: state.discovered.into_iter().collect(),
                reached_disclosure_nodes: state.runtime.revealed_disclosures.into_iter().collect(),
                failure_reason: None,
                trace,
            };
        }
        if state.turns >= limits.max_turns {
            hit_turn_limit = true;
            continue;
        }

        let mut actions: Vec<_> = state
            .discovered
            .iter()
            .filter(|id| !state.runtime.confronted_evidence.contains(*id))
            .filter_map(|id| {
                evidence
                    .get(id)
                    .map(|item| SimulationAction::PresentEvidence {
                        character: character.id.clone(),
                        evidence: id.clone(),
                        claim: item.contradicts.first().cloned(),
                    })
            })
            .collect();
        actions.sort_by_key(action_key);
        actions.truncate(limits.max_branching);
        for action in actions {
            let SimulationAction::PresentEvidence {
                evidence: evidence_id,
                claim,
                ..
            } = &action
            else {
                continue;
            };
            let mut next = state.clone();
            let interpreted = InterpretedAction {
                intent: PlayerIntent::PresentEvidence,
                topics: vec![],
                referenced_entities: vec![],
                referenced_claims: claim.iter().cloned().collect(),
                evidence_usage: vec![EvidenceUse {
                    evidence_id: evidence_id.clone(),
                    usage: EvidenceUsageKind::DirectReference,
                }],
                asserted_propositions: vec![],
                tone: PlayerTone::Neutral,
                confidence: 1.0,
            };
            let transition = engine.process_with_requirements(
                &interpreted,
                &mut next.runtime,
                character,
                &evidence,
                &case.definition.required_case_elements,
                TurnId::new(),
            );
            next.turns = next.turns.saturating_add(1);
            discover_available(
                &evidence,
                &next.runtime.confronted_evidence,
                next.runtime.phase,
                &mut next.discovered,
            );
            next.trace.push(SimulationStep {
                turn: next.turns,
                action,
                phase_after: next.runtime.phase,
                newly_revealed_disclosures: transition.diff.newly_revealed_disclosures,
            });
            queue.push_back(next);
        }
    }

    let reason = if hit_turn_limit {
        SimulationFailure::TurnLimit
    } else if !saw_evidence_solution {
        SimulationFailure::NoPathToRequiredEvidence
    } else {
        SimulationFailure::DisclosureNodeUnreachable
    };
    from_best(reason, visited, best)
}

fn discover_available(
    evidence: &BTreeMap<EvidenceId, narrastate_core::EvidenceDefinition>,
    presented: &BTreeSet<EvidenceId>,
    phase: InterrogationPhase,
    discovered: &mut BTreeSet<EvidenceId>,
) {
    loop {
        let before = discovered.len();
        for item in evidence.values() {
            if item.discoverable_by.iter().any(|rule| match rule {
                DiscoveryRule::StartingEvidence => true,
                DiscoveryRule::AutomaticAtPhase { phase: required } => phase >= *required,
                DiscoveryRule::AfterEvidencePresented {
                    evidence_id: required,
                } => presented.contains(required),
            }) {
                discovered.insert(item.id.clone());
            }
        }
        if discovered.len() == before {
            break;
        }
    }
}

fn state_key(state: &SearchState) -> String {
    format!(
        "{:?}|{}|{}|{}|{:?}|{:?}|{:?}",
        state.runtime.phase,
        state.runtime.stress,
        state.runtime.composure,
        state.runtime.defense_budget,
        state.runtime.confronted_evidence,
        state.runtime.revealed_disclosures,
        state.discovered
    )
}

fn action_key(action: &SimulationAction) -> String {
    match action {
        SimulationAction::PresentEvidence { evidence, .. }
        | SimulationAction::ChallengeClaim { evidence, .. } => evidence.to_string(),
        _ => format!("{action:?}"),
    }
}

fn from_best(
    reason: SimulationFailure,
    visited: usize,
    best: Option<SearchState>,
) -> SimulationResult {
    let Some(best) = best else {
        return failed(reason, visited, 0, vec![], vec![], vec![]);
    };
    failed(
        reason,
        visited,
        best.turns,
        best.discovered.into_iter().collect(),
        best.runtime.revealed_disclosures.into_iter().collect(),
        best.trace,
    )
}

fn failed(
    reason: SimulationFailure,
    visited_states: usize,
    turns: u32,
    acquired_evidence_ids: Vec<EvidenceId>,
    reached_disclosure_nodes: Vec<DisclosureId>,
    trace: Vec<SimulationStep>,
) -> SimulationResult {
    SimulationResult {
        success: false,
        visited_states,
        turns,
        acquired_evidence_ids,
        reached_disclosure_nodes,
        failure_reason: Some(reason),
        trace,
    }
}
