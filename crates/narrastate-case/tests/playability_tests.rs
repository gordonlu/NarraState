//! Playability budgets for the shipped golden cases.
//!
//! These tests replay a whole player session through the same runtime path the
//! server uses (Mock interpreter -> TransitionEngine -> DialoguePlanner) and
//! assert that a realistic player can actually reach a confession inside a
//! generous turn budget, instead of stalling on an unwinnable gate.
//!
//! They also pin the guard rails that must stay true no matter how much the
//! difficulty is loosened: a confession is only ever produced by the
//! DisclosureGraph, never by a contradiction count.
use std::collections::{BTreeMap, BTreeSet};

use narrastate_case::{adapt_v01, compile};
use narrastate_core::{
    CaseDefinition, CharacterDefinition, CharacterRuntimeState, DisclosureKind, EvidenceId,
    InterrogationPhase, VariantId,
};
use narrastate_runtime::mock::MockInterpreter;
use narrastate_runtime::{covered_elements, DialoguePlanner, TransitionEngine};

const TURN_BUDGET: u32 = 12;

struct Session {
    character: CharacterDefinition,
    evidence: BTreeMap<EvidenceId, narrastate_core::EvidenceDefinition>,
    required: BTreeSet<narrastate_core::CaseElement>,
    state: CharacterRuntimeState,
    engine: TransitionEngine,
    planner: DialoguePlanner,
    interpreter: MockInterpreter,
    turns: u32,
    revealed_order: Vec<narrastate_core::DisclosureId>,
    most_unlocked_in_one_turn: usize,
    history: Vec<(String, Vec<EvidenceId>)>,
}

fn start() -> Session {
    let legacy: CaseDefinition =
        serde_json::from_str(include_str!("../../../cases/rain-gallery/case.json")).unwrap();
    let template = adapt_v01(legacy, "1.0.0", VariantId::from("classic")).unwrap();
    let compiled = compile(&template, &VariantId::from("classic")).unwrap();
    let character = compiled
        .definition
        .characters
        .iter()
        .find(|candidate| candidate.id == compiled.responsible_character_id)
        .expect("responsible character")
        .clone();
    let evidence: BTreeMap<_, _> = compiled
        .definition
        .evidence
        .iter()
        .cloned()
        .map(|item| (item.id.clone(), item))
        .collect();
    let required = compiled.definition.required_case_elements.clone();
    Session {
        state: CharacterRuntimeState::new(character.resilience),
        character,
        evidence,
        required,
        engine: TransitionEngine::new(narrastate_core::TransitionTuning::default()),
        planner: DialoguePlanner,
        interpreter: MockInterpreter,
        turns: 0,
        revealed_order: Vec::new(),
        most_unlocked_in_one_turn: 0,
        history: Vec::new(),
    }
}

impl Session {
    /// Plays one player turn exactly like the server does in Mock mode.
    fn turn(&mut self, text: &str, attached: &[EvidenceId]) -> Vec<narrastate_core::DisclosureId> {
        let action = self.interpreter.interpret(text, attached);
        let before = self.state.revealed_disclosures.clone();
        self.engine.process_with_requirements(
            &action,
            &mut self.state,
            &self.character,
            &self.evidence,
            &self.required,
            narrastate_core::TurnId::new(),
        );
        let newly: Vec<_> = self
            .state
            .revealed_disclosures
            .difference(&before)
            .cloned()
            .collect();
        self.revealed_order.extend(newly.iter().cloned());
        self.most_unlocked_in_one_turn = self
            .most_unlocked_in_one_turn
            .max(newly.len());
        let _ = self
            .planner
            .plan_with_context(
                &action,
                &mut self.state.clone(),
                &self.character,
                &self.evidence,
                &BTreeSet::new(),
                newly.first(),
            );
        self.turns = self.turns.saturating_add(1);
        self.history.push((text.to_string(), attached.to_vec()));
        newly
    }

    /// Repeats the last question until the turn budget runs out, which is what
    /// a stuck player does when they have nothing new left to show. Once the
    /// character is confession-eligible the player states the accusation out
    /// loud, which is the only thing that can open the confession node.
    fn press_until_confessed_or_budget(&mut self) {
        while self.turns < TURN_BUDGET && !self.confessed() {
            let (text, attached) = self
                .history
                .last()
                .cloned()
                .unwrap_or_else(|| ("请再说一遍。".to_string(), Vec::new()));
            if self.state.phase == InterrogationPhase::ConfessionEligible {
                // Without attachments the Mock interpreter reads this as an
                // accusation, which is what can open the confession node.
                self.turn("你隐瞒了关键事实，认罪吧。", &[]);
            } else {
                self.turn(&text, &attached);
            }
        }
    }

    fn confessed(&self) -> bool {
        self.character
            .disclosure_graph
            .confession_node()
            .is_some_and(|node| self.state.revealed_disclosures.contains(&node.id))
    }

    fn confession_reachable(&self) -> bool {
        self.character.disclosure_graph.confession_node().is_some()
    }

    fn elements_complete(&self) -> bool {
        self.required
            .is_subset(&covered_elements(&self.state.confronted_evidence, &self.evidence))
    }
}

fn evidence_ids() -> Vec<EvidenceId> {
    vec![
        EvidenceId::from("ev_card_log"),
        EvidenceId::from("ev_sensor_cmd"),
        EvidenceId::from("ev_mud_track"),
        EvidenceId::from("ev_fiber"),
        EvidenceId::from("ev_pawn_contact"),
    ]
}

#[test]
fn player_who_presents_one_clue_per_turn_reaches_confession() {
    let mut session = start();
    for id in evidence_ids() {
        session.turn("这条线索你怎么解释？", &[id]);
    }
    session.press_until_confessed_or_budget();

    assert!(
        session.confessed(),
        "one-clue-per-turn play must reach the confession node, phase={:?}",
        session.state.phase
    );
    assert!(
        session.turns <= 6,
        "one-clue-per-turn play should not need more than 6 turns, took {}",
        session.turns
    );
    assert!(session.elements_complete());
}

#[test]
fn player_who_dumps_every_clue_into_one_question_still_reaches_confession() {
    let mut session = start();
    let all = evidence_ids();
    session.turn("这些线索都指向你，认罪吧。", &all);
    session.press_until_confessed_or_budget();

    assert!(
        session.confessed(),
        "batching every clue into one turn must not stall the investigation, \
         phase={:?} revealed={:?}",
        session.state.phase,
        session.revealed_order
    );
    assert!(
        session.turns <= TURN_BUDGET,
        "batching all clues should still resolve inside the turn budget"
    );
    // The satisfied chain must catch up in fewer turns than it has nodes.
    assert!(
        session.most_unlocked_in_one_turn > 1,
        "the disclosure chain must be able to catch up in one turn, biggest single turn: {}",
        session.most_unlocked_in_one_turn
    );
}

#[test]
fn player_who_starts_by_listening_can_still_reach_confession() {
    let mut session = start();
    session.turn("请从头说说你当时做了什么。", &[]);
    session.turn("事发当晚你在哪里？", &[]);
    // A player who listened first usually attaches the clues in batches.
    let batches = [
        vec![EvidenceId::from("ev_card_log"), EvidenceId::from("ev_sensor_cmd")],
        vec![EvidenceId::from("ev_mud_track"), EvidenceId::from("ev_fiber")],
        vec![EvidenceId::from("ev_pawn_contact")],
    ];
    for batch in &batches {
        session.turn("这条线索和你的说法矛盾。", batch);
    }
    session.press_until_confessed_or_budget();

    assert!(
        session.confessed(),
        "listening first must not be a dead end, phase={:?}",
        session.state.phase
    );
    assert!(
        session.turns <= TURN_BUDGET,
        "listening first then batching clues should still resolve inside the budget, took {}",
        session.turns
    );
}

#[test]
fn confession_requires_the_full_disclosure_chain_not_a_contradiction_count() {
    // Never hand over the two clues that gate FullAction; the character still
    // ends up with every claim contradicted and maximum stress.
    let mut session = start();
    let allowed = [
        EvidenceId::from("ev_card_log"),
        EvidenceId::from("ev_sensor_cmd"),
        EvidenceId::from("ev_pawn_contact"),
    ];
    for id in &allowed {
        session.turn("这条线索说明你在场。", &[id.clone()]);
    }
    while session.turns < TURN_BUDGET {
        session.turn("你的说法全是假的，你认罪。", &allowed);
    }

    assert!(
        session.confession_reachable(),
        "fixture must have a confession path for this invariant to be meaningful"
    );
    assert!(
        !session.confessed(),
        "confession leaked out of the DisclosureGraph without the FullAction prerequisites"
    );
    assert!(
        session.state.stress > 0 || session.turns == 0,
        "the character should have been under real pressure: stress={}",
        session.state.stress
    );
    assert!(
        session.state.phase <= InterrogationPhase::ConfessionEligible,
        "phase must not run past the confession gate, got {:?}",
        session.state.phase
    );
    // The action-level disclosure must never have been reachable.
    assert!(
        !session
            .revealed_order
            .iter()
            .any(|id| session.character.disclosure_graph.nodes.iter().any(|node| {
                &node.id == id && node.kind == DisclosureKind::FullAction
            })),
        "FullAction was revealed without its evidence prerequisites"
    );
}

#[test]
fn bare_questions_never_force_a_confession() {
    let mut session = start();
    for _ in 0..TURN_BUDGET {
        session.turn("是你偷的吗？请解释。", &[]);
    }

    assert!(!session.confessed());
    assert!(
        session.state.phase <= InterrogationPhase::Guarded,
        "asking without evidence should not escalate past Guarded, got {:?}",
        session.state.phase
    );
}

#[test]
fn confession_node_is_reserved_for_a_decisive_turn() {
    // Completing the whole chain with routine evidence turns must stop at the
    // Intent disclosure; only an explicit accusation or challenge opens the
    // confession node.
    let mut session = start();
    for id in evidence_ids() {
        session.turn("这条线索你怎么解释？", &[id]);
    }

    assert!(
        session.state.phase == InterrogationPhase::ConfessionEligible,
        "the escalation should be complete, got {:?}",
        session.state.phase
    );
    assert!(
        !session.confessed(),
        "presenting the last clue must not auto-confess"
    );

    // The player states the case out loud.
    session.turn("证据都指向你，认罪吧。", &[]);
    assert!(
        session.confessed(),
        "an explicit decisive turn must open the confession node"
    );
    assert!(
        session.turns <= 7,
        "the whole run should stay inside a short session, took {}",
        session.turns
    );
}
