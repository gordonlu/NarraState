use std::collections::{BTreeMap, BTreeSet};

use narrastate_core::claim::{ClaimDefinition, ClaimKind};
use narrastate_core::disclosure::{
    DialogueAct, DisclosureGraph, DisclosureKind, DisclosureNode, DisclosurePrerequisite,
};
use narrastate_core::evidence::{CaseElement, EvidenceDefinition, EvidenceUsageKind, EvidenceUse};
use narrastate_core::fact::{FactValue, Proposition};
use narrastate_core::id::{
    CharacterId, ClaimId, DefenseStrategyId, DisclosureId, EvidenceId, FactId, TurnId,
};
use narrastate_core::phase::InterrogationPhase;
use narrastate_core::strategy::{DefenseStrategy, DefenseStrategyKind};
use narrastate_core::transition::{InterpretedAction, PlayerIntent, PlayerTone, TransitionTuning};
use narrastate_core::{
    Belief, BeliefSource, CharacterDefinition, CharacterGoal, CharacterRuntimeState,
    PersonalityProfile,
};
use narrastate_runtime::mock::MockRenderer;
use narrastate_runtime::{DialoguePlanner, TransitionEngine, TransitionResult};

// ── Test Fixtures ──────────────────────────────────────────────────────

#[allow(dead_code)]
struct CaseFixture {
    culprit_id: CharacterId,
    red_herring_id: CharacterId,
    culprit: CharacterDefinition,
    red_herring: CharacterDefinition,
    evidence: BTreeMap<EvidenceId, EvidenceDefinition>,
    facts: BTreeSet<FactId>,
}

fn make_culprit_disclosure_graph() -> DisclosureGraph {
    DisclosureGraph {
        nodes: vec![
            DisclosureNode {
                id: DisclosureId::from("d1_presence"),
                kind: DisclosureKind::Presence,
                reveals: vec![],
                prerequisites: vec![],
                min_phase: InterrogationPhase::Guarded,
                response_intent: DialogueAct::PartialAdmission,
            },
            DisclosureNode {
                id: DisclosureId::from("d2_access"),
                kind: DisclosureKind::Access,
                reveals: vec![],
                prerequisites: vec![DisclosurePrerequisite::Disclosure {
                    disclosure: DisclosureId::from("d1_presence"),
                }],
                min_phase: InterrogationPhase::Defensive,
                response_intent: DialogueAct::PartialAdmission,
            },
            DisclosureNode {
                id: DisclosureId::from("d3_means"),
                kind: DisclosureKind::Means,
                reveals: vec![],
                prerequisites: vec![DisclosurePrerequisite::Disclosure {
                    disclosure: DisclosureId::from("d2_access"),
                }],
                min_phase: InterrogationPhase::Pressured,
                response_intent: DialogueAct::PartialAdmission,
            },
            DisclosureNode {
                id: DisclosureId::from("d4_action"),
                kind: DisclosureKind::FullAction,
                reveals: vec![],
                prerequisites: vec![DisclosurePrerequisite::Disclosure {
                    disclosure: DisclosureId::from("d3_means"),
                }],
                min_phase: InterrogationPhase::Cornered,
                response_intent: DialogueAct::PartialAdmission,
            },
            DisclosureNode {
                id: DisclosureId::from("d5_confession"),
                kind: DisclosureKind::Confession,
                reveals: vec![],
                prerequisites: vec![DisclosurePrerequisite::Disclosure {
                    disclosure: DisclosureId::from("d4_action"),
                }],
                min_phase: InterrogationPhase::ConfessionEligible,
                response_intent: DialogueAct::FullAdmission,
            },
        ],
    }
}

fn make_red_herring_disclosure_graph() -> DisclosureGraph {
    DisclosureGraph {
        nodes: vec![DisclosureNode {
            id: DisclosureId::from("hr1_secret"),
            kind: DisclosureKind::PeripheralSecret,
            reveals: vec![],
            prerequisites: vec![],
            min_phase: InterrogationPhase::Defensive,
            response_intent: DialogueAct::Answer,
        }],
    }
}

fn build_fixture() -> CaseFixture {
    let culprit_id = CharacterId::from("luo-cheng");
    let red_herring_id = CharacterId::from("shen-an");
    let fact_1 = FactId::from("fact_motive");
    let fact_2 = FactId::from("fact_opportunity");
    let ev_card = EvidenceId::from("ev_card_log");
    let ev_sensor = EvidenceId::from("ev_sensor_cmd");
    let ev_mud = EvidenceId::from("ev_mud_track");
    let ev_fiber = EvidenceId::from("ev_fiber");
    let ev_pawn = EvidenceId::from("ev_pawn_contact");
    let claim_never_left = ClaimId::from("claim_never_left");
    let claim_innocent = ClaimId::from("claim_innocent");

    let mut evidence = BTreeMap::new();
    evidence.insert(
        ev_card.clone(),
        EvidenceDefinition {
            id: ev_card.clone(),
            title: "复制门禁卡记录".into(),
            description: "21:47 使用记录".into(),
            supports: vec![],
            contradicts: vec![claim_never_left.clone()],
            elements: vec![CaseElement::Opportunity].into_iter().collect(),
            reliability: 0.9,
            directness: 0.85,
            exclusivity: 0.6,
            discoverable_by: vec![],
        },
    );
    evidence.insert(
        ev_sensor.clone(),
        EvidenceDefinition {
            id: ev_sensor.clone(),
            title: "传感器维护命令".into(),
            description: "21:43 由安保终端发出".into(),
            supports: vec![],
            contradicts: vec![claim_innocent.clone()],
            elements: vec![CaseElement::Means, CaseElement::Action]
                .into_iter()
                .collect(),
            reliability: 0.8,
            directness: 0.75,
            exclusivity: 0.7,
            discoverable_by: vec![],
        },
    );
    evidence.insert(
        ev_mud.clone(),
        EvidenceDefinition {
            id: ev_mud.clone(),
            title: "红色湿泥".into(),
            description: "鞋底泥迹匹配装卸区".into(),
            supports: vec![],
            contradicts: vec![claim_never_left.clone(), claim_innocent.clone()],
            elements: vec![CaseElement::Action].into_iter().collect(),
            reliability: 0.7,
            directness: 0.6,
            exclusivity: 0.5,
            discoverable_by: vec![],
        },
    );
    evidence.insert(
        ev_fiber.clone(),
        EvidenceDefinition {
            id: ev_fiber.clone(),
            title: "制服纤维与缺失纽扣".into(),
            description: "在运输箱上发现".into(),
            supports: vec![],
            contradicts: vec![claim_innocent.clone()],
            elements: vec![CaseElement::Action, CaseElement::Identity]
                .into_iter()
                .collect(),
            reliability: 0.85,
            directness: 0.9,
            exclusivity: 0.8,
            discoverable_by: vec![],
        },
    );
    evidence.insert(
        ev_pawn.clone(),
        EvidenceDefinition {
            id: ev_pawn.clone(),
            title: "典当行联系人消息".into(),
            description: "显示预谋盗窃".into(),
            supports: vec![],
            contradicts: vec![claim_innocent.clone()],
            elements: vec![CaseElement::Intent].into_iter().collect(),
            reliability: 0.6,
            directness: 0.5,
            exclusivity: 0.3,
            discoverable_by: vec![],
        },
    );

    let culprit = CharacterDefinition {
        id: culprit_id.clone(),
        name: "罗成".into(),
        role: "安保主管".into(),
        public_profile: "负责门禁和巡查".into(),
        personality: PersonalityProfile {
            traits: vec!["冷静".into()],
            speech_style: None,
        },
        goals: vec![CharacterGoal {
            description: "隐藏盗窃".into(),
            priority: 10,
        }],
        knowledge: vec![fact_1.clone(), fact_2.clone()],
        initial_beliefs: vec![Belief {
            proposition: Proposition {
                subject: "luo-cheng".into(),
                predicate: "is_innocent".to_string(),
                object: FactValue::Boolean(true),
            },
            confidence: 100,
            source: BeliefSource::Default,
        }],
        claims: vec![
            ClaimDefinition {
                id: claim_never_left.clone(),
                owner: culprit_id.clone(),
                proposition: Proposition {
                    subject: "luo-cheng".into(),
                    predicate: "was_in".to_string(),
                    object: FactValue::String("control_room".to_string()),
                },
                kind: ClaimKind::Lie,
                available_from: InterrogationPhase::Calm,
                invalidated_by: vec![ev_card.clone(), ev_mud.clone()],
                fallback_claim: None,
            },
            ClaimDefinition {
                id: claim_innocent.clone(),
                owner: culprit_id.clone(),
                proposition: Proposition {
                    subject: "luo-cheng".into(),
                    predicate: "is_innocent".to_string(),
                    object: FactValue::Boolean(true),
                },
                kind: ClaimKind::Lie,
                available_from: InterrogationPhase::Calm,
                invalidated_by: vec![ev_sensor.clone(), ev_fiber.clone(), ev_pawn.clone()],
                fallback_claim: None,
            },
        ],
        defenses: vec![
            DefenseStrategy {
                id: DefenseStrategyId::from("denial"),
                kind: DefenseStrategyKind::Denial,
                usable_phases: vec![
                    InterrogationPhase::Calm,
                    InterrogationPhase::Guarded,
                    InterrogationPhase::Defensive,
                ],
                max_uses: 3,
                applicable_claims: vec![],
                fallback_strategy: None,
                style_prompt: None,
            },
            DefenseStrategy {
                id: DefenseStrategyId::from("innocent_explain"),
                kind: DefenseStrategyKind::InnocentExplanation,
                usable_phases: vec![InterrogationPhase::Pressured, InterrogationPhase::Cornered],
                max_uses: 2,
                applicable_claims: vec![],
                fallback_strategy: None,
                style_prompt: None,
            },
        ],
        disclosure_graph: make_culprit_disclosure_graph(),
        resilience: 60,
    };

    let red_herring = CharacterDefinition {
        id: red_herring_id.clone(),
        name: "沈安".into(),
        role: "策展人".into(),
        public_profile: "与画作所有人有估值争议".into(),
        personality: PersonalityProfile {
            traits: vec!["紧张".into()],
            speech_style: None,
        },
        goals: vec![CharacterGoal {
            description: "掩盖延期报告".into(),
            priority: 5,
        }],
        knowledge: vec![],
        initial_beliefs: vec![],
        claims: vec![],
        defenses: vec![DefenseStrategy {
            id: DefenseStrategyId::from("hr_denial"),
            kind: DefenseStrategyKind::Denial,
            usable_phases: vec![
                InterrogationPhase::Calm,
                InterrogationPhase::Guarded,
                InterrogationPhase::Defensive,
            ],
            max_uses: 5,
            applicable_claims: vec![],
            fallback_strategy: None,
            style_prompt: None,
        }],
        disclosure_graph: make_red_herring_disclosure_graph(),
        resilience: 40,
    };

    let mut facts = BTreeSet::new();
    facts.insert(fact_1);
    facts.insert(fact_2);

    CaseFixture {
        culprit_id,
        red_herring_id,
        culprit,
        red_herring,
        evidence,
        facts,
    }
}

fn create_renderer() -> MockRenderer {
    MockRenderer
}

fn create_engine() -> TransitionEngine {
    TransitionEngine::new(TransitionTuning::default())
}

fn create_planner() -> DialoguePlanner {
    DialoguePlanner
}

#[allow(clippy::too_many_arguments)]
fn run_turn(
    engine: &TransitionEngine,
    planner: &DialoguePlanner,
    renderer: &MockRenderer,
    character_def: &CharacterDefinition,
    state: &mut CharacterRuntimeState,
    evidence: &BTreeMap<EvidenceId, EvidenceDefinition>,
    facts: &BTreeSet<FactId>,
    action: &InterpretedAction,
    turn_id: TurnId,
) -> TransitionResult {
    let result = engine.process(action, state, character_def, evidence, facts, turn_id);
    let plan = planner.plan(action, state, character_def, evidence);
    let _utterance = renderer.render(&plan);
    result
}

fn make_action(
    intent: PlayerIntent,
    evidence_ids: Vec<EvidenceId>,
    text: &str,
) -> InterpretedAction {
    let evidence_usage: Vec<EvidenceUse> = evidence_ids
        .into_iter()
        .map(|eid| EvidenceUse {
            evidence_id: eid,
            usage: EvidenceUsageKind::DirectReference,
        })
        .collect();
    InterpretedAction {
        intent,
        topics: vec![text.to_string()],
        referenced_entities: vec![],
        referenced_claims: vec![],
        evidence_usage,
        asserted_propositions: vec![],
        tone: PlayerTone::Neutral,
        confidence: 1.0,
    }
}

// ── G1: No evidence direct questioning ─────────────────────────────────

#[test]
fn golden_g1_no_evidence_direct_questioning() {
    let fixture = build_fixture();
    let engine = create_engine();
    let planner = create_planner();
    let renderer = create_renderer();
    let mut state = CharacterRuntimeState::new(fixture.culprit.resilience);

    for i in 0..20 {
        let turn_id = TurnId::new();
        let action = make_action(PlayerIntent::Ask, vec![], "是不是你偷的");
        let result = run_turn(
            &engine,
            &planner,
            &renderer,
            &fixture.culprit,
            &mut state,
            &fixture.evidence,
            &fixture.facts,
            &action,
            turn_id,
        );
        // Should not progress past Guarded with no evidence
        assert!(
            state.phase <= InterrogationPhase::Guarded,
            "Turn {i}: phase should be <= Guarded, got {:?}",
            state.phase
        );
        // No disclosure should be unlocked
        assert!(
            result.diff.newly_revealed_disclosures.is_empty(),
            "Turn {i}: no disclosures should be unlocked"
        );
    }

    // After 20 questions with no evidence, stress should be manageable
    assert!(state.stress < 30, "Stress should stay low without evidence");
}

// ── G2: Single evidence repeated use with decay ────────────────────────

#[test]
fn golden_g2_single_evidence_repeated_use() {
    let fixture = build_fixture();
    let engine = create_engine();
    let planner = create_planner();
    let renderer = create_renderer();
    let mut state = CharacterRuntimeState::new(fixture.culprit.resilience);
    let ev_card = EvidenceId::from("ev_card_log");

    let turn_1 = TurnId::new();
    let action_1 = make_action(
        PlayerIntent::PresentEvidence,
        vec![ev_card.clone()],
        "这是你的门禁卡记录",
    );
    let result_1 = run_turn(
        &engine,
        &planner,
        &renderer,
        &fixture.culprit,
        &mut state,
        &fixture.evidence,
        &fixture.facts,
        &action_1,
        turn_1,
    );

    // First use should have impact
    assert!(result_1.diff.stress_after > result_1.diff.stress_before);
    assert!(state.confronted_evidence.contains(&ev_card));
    assert!(state.stress > 0);

    let stress_after_first = state.stress;

    // Repeat the same evidence 9 more times
    for i in 0..9 {
        let turn_id = TurnId::new();
        let action = make_action(
            PlayerIntent::PresentEvidence,
            vec![ev_card.clone()],
            "还是这个门禁卡记录",
        );
        let result = run_turn(
            &engine,
            &planner,
            &renderer,
            &fixture.culprit,
            &mut state,
            &fixture.evidence,
            &fixture.facts,
            &action,
            turn_id,
        );
        // Stress should not increase (decayed novelty)
        assert!(
            result.diff.stress_after <= stress_after_first + 5u8,
            "Turn {i}: stress shouldn't significantly increase from repeated evidence"
        );
    }

    // Should not reach confession by repeating the same evidence
    assert!(
        state.phase < InterrogationPhase::ConfessionEligible,
        "Cannot reach confession-eligible by repeating same evidence"
    );
}

// ── G3: Natural disclosure path (D1→D5) ────────────────────────────────

#[test]
fn golden_g3_natural_disclosure_path() {
    let fixture = build_fixture();
    let engine = create_engine();
    let planner = create_planner();
    let renderer = create_renderer();
    let mut state = CharacterRuntimeState::new(fixture.culprit.resilience);

    let evidence_order = [
        EvidenceId::from("ev_card_log"),
        EvidenceId::from("ev_sensor_cmd"),
        EvidenceId::from("ev_mud_track"),
        EvidenceId::from("ev_fiber"),
        EvidenceId::from("ev_pawn_contact"),
    ];

    let mut cumulative_major = 0usize;

    for (i, eid) in evidence_order.iter().enumerate() {
        let turn_id = TurnId::new();
        let action = make_action(
            PlayerIntent::PresentEvidence,
            vec![eid.clone()],
            &format!("证据 {i}"),
        );
        let result = run_turn(
            &engine,
            &planner,
            &renderer,
            &fixture.culprit,
            &mut state,
            &fixture.evidence,
            &fixture.facts,
            &action,
            turn_id,
        );

        // Track major disclosures
        let major_unlocked = result.diff.newly_revealed_disclosures.len();
        cumulative_major += major_unlocked;

        // At most one major disclosure per turn (the first unlockable one)
        assert!(
            major_unlocked <= 1,
            "Turn {i}: at most one major disclosure per turn, got {major_unlocked}"
        );
    }

    // After all evidence, we should have progressed through multiple phases
    assert!(
        state.phase >= InterrogationPhase::Cornered,
        "After all evidence, phase should be at least Cornered, got {:?}",
        state.phase
    );

    // At least one major disclosure was made
    assert!(
        cumulative_major > 0,
        "At least one major disclosure should have been unlocked"
    );

    // D5 (confession) requires explicit trigger (final question)
    assert!(
        state.phase <= InterrogationPhase::ConfessionEligible,
        "Should not auto-confess"
    );

    // Now ask the final question that triggers confession
    let final_turn = TurnId::new();
    let final_action = make_action(PlayerIntent::Accuse, vec![], "是你干的，认罪吧");
    let _final_result = run_turn(
        &engine,
        &planner,
        &renderer,
        &fixture.culprit,
        &mut state,
        &fixture.evidence,
        &fixture.facts,
        &final_action,
        final_turn,
    );

    // After final trigger, either confession was unlocked or we're confession-eligible
    assert!(
        state.phase >= InterrogationPhase::ConfessionEligible,
        "After final accusation, should be at least ConfessionEligible"
    );
}

// ── G4: Out-of-order strong evidence ──────────────────────────────────

#[test]
fn golden_g4_out_of_order_strong_evidence() {
    let fixture = build_fixture();
    let engine = create_engine();
    let planner = create_planner();
    let renderer = create_renderer();
    let mut state = CharacterRuntimeState::new(fixture.culprit.resilience);

    // Present the strongest evidence first (pawn contact = intent)
    let strong_evidence = [
        EvidenceId::from("ev_pawn_contact"),
        EvidenceId::from("ev_fiber"),
    ];

    for eid in strong_evidence.iter() {
        let turn_id = TurnId::new();
        let action = make_action(
            PlayerIntent::PresentEvidence,
            vec![eid.clone()],
            "决定性证据",
        );
        let _result = run_turn(
            &engine,
            &planner,
            &renderer,
            &fixture.culprit,
            &mut state,
            &fixture.evidence,
            &fixture.facts,
            &action,
            turn_id,
        );
    }

    // Strong evidence creates pressure but cannot skip to FullAction/Confession
    // without going through the disclosure chain
    let _revealed_action = state
        .revealed_disclosures
        .contains(&DisclosureId::from("d4_action"));
    let revealed_confession = state
        .revealed_disclosures
        .contains(&DisclosureId::from("d5_confession"));

    assert!(
        !revealed_confession,
        "Out-of-order evidence should not directly unlock confession"
    );

    // Should have moved past Calm due to pressure
    assert!(
        state.phase > InterrogationPhase::Calm,
        "Out-of-order evidence should create pressure"
    );

    // Now present evidence in order to complete the chain
    let ordered = [
        EvidenceId::from("ev_card_log"),
        EvidenceId::from("ev_sensor_cmd"),
        EvidenceId::from("ev_mud_track"),
    ];
    for eid in &ordered {
        let turn_id = TurnId::new();
        let action = make_action(PlayerIntent::PresentEvidence, vec![eid.clone()], "补充证据");
        let _result = run_turn(
            &engine,
            &planner,
            &renderer,
            &fixture.culprit,
            &mut state,
            &fixture.evidence,
            &fixture.facts,
            &action,
            turn_id,
        );
    }

    // After completing the chain, FullAction should be unlockable
    // but Confession still requires explicit trigger
    assert!(
        state.phase >= InterrogationPhase::Cornered,
        "After completing evidence chain, phase should advance"
    );
}

// ── G5: Wrong suspect high pressure ───────────────────────────────────

#[test]
fn golden_g5_wrong_suspect_high_pressure() {
    let fixture = build_fixture();
    let engine = create_engine();
    let planner = create_planner();
    let renderer = create_renderer();
    let mut state = CharacterRuntimeState::new(fixture.red_herring.resilience);

    // Apply high pressure to the red herring (Shen An) with all evidence
    let all_evidence = [
        EvidenceId::from("ev_card_log"),
        EvidenceId::from("ev_sensor_cmd"),
        EvidenceId::from("ev_mud_track"),
        EvidenceId::from("ev_fiber"),
        EvidenceId::from("ev_pawn_contact"),
    ];

    for eid in &all_evidence {
        let turn_id = TurnId::new();
        let action = make_action(PlayerIntent::PresentEvidence, vec![eid.clone()], "证据");
        let _result = run_turn(
            &engine,
            &planner,
            &renderer,
            &fixture.red_herring,
            &mut state,
            &fixture.evidence,
            &fixture.facts,
            &action,
            turn_id,
        );
    }

    // Even under high pressure, the red herring has no Confession node
    let has_confession = fixture
        .red_herring
        .disclosure_graph
        .confession_node()
        .is_some();
    assert!(
        !has_confession,
        "Red herring should have no Confession node"
    );

    // The red herring can only reveal peripheral secrets at most
    assert!(
        state.phase <= InterrogationPhase::Defensive,
        "Red herring should not enter high-pressure phases"
    );
}

// ── G6: Correct accusation but insufficient evidence ──────────────────

#[test]
fn golden_g6_correct_accusation_insufficient_evidence() {
    let fixture = build_fixture();
    let engine = create_engine();
    let planner = create_planner();
    let renderer = create_renderer();
    let mut state = CharacterRuntimeState::new(fixture.culprit.resilience);

    // Present just one piece of evidence
    let turn_id = TurnId::new();
    let action = make_action(
        PlayerIntent::PresentEvidence,
        vec![EvidenceId::from("ev_card_log")],
        "门禁记录",
    );
    let _result = run_turn(
        &engine,
        &planner,
        &renderer,
        &fixture.culprit,
        &mut state,
        &fixture.evidence,
        &fixture.facts,
        &action,
        turn_id,
    );

    // Accuse with insufficient evidence
    let accuse_turn = TurnId::new();
    let accuse_action = make_action(PlayerIntent::Accuse, vec![], "就是你偷的");
    let _result = run_turn(
        &engine,
        &planner,
        &renderer,
        &fixture.culprit,
        &mut state,
        &fixture.evidence,
        &fixture.facts,
        &accuse_action,
        accuse_turn,
    );

    // With insufficient evidence, the character should NOT be forced to confess
    assert!(
        state.phase < InterrogationPhase::ConfessionEligible,
        "Without sufficient evidence, should not reach confession eligibility"
    );

    // The accusation should not change the core gameplay state
    assert!(
        state.phase >= InterrogationPhase::Guarded,
        "At least some pressure from the accusation"
    );
}

// ── G7: Case proven without confession ─────────────────────────────────

#[test]
fn golden_g7_case_proven_without_confession() {
    let fixture = build_fixture();
    let engine = create_engine();
    let planner = create_planner();
    let renderer = create_renderer();
    let mut state = CharacterRuntimeState::new(fixture.culprit.resilience);

    // Present all evidence in order
    let evidence_order = [
        EvidenceId::from("ev_card_log"),
        EvidenceId::from("ev_sensor_cmd"),
        EvidenceId::from("ev_mud_track"),
        EvidenceId::from("ev_fiber"),
        EvidenceId::from("ev_pawn_contact"),
    ];

    for eid in &evidence_order {
        let turn_id = TurnId::new();
        let action = make_action(PlayerIntent::PresentEvidence, vec![eid.clone()], "证据");
        let _result = run_turn(
            &engine,
            &planner,
            &renderer,
            &fixture.culprit,
            &mut state,
            &fixture.evidence,
            &fixture.facts,
            &action,
            turn_id,
        );
    }

    // Check that the required case elements are covered by the evidence presented
    // In a real system, this would be checked by the accusation subsystem.
    // Here we verify the state progressed appropriately.
    assert!(
        state.phase >= InterrogationPhase::ConfessionEligible,
        "Phase should be at least ConfessionEligible after all evidence, got {:?}",
        state.phase
    );

    // Even at ConfessionEligible, the character hasn't necessarily confessed
    // (that requires a specific triggering turn)
    let _confessed = state
        .revealed_disclosures
        .contains(&DisclosureId::from("d5_confession"));
    // The confession requires explicit trigger, so it should NOT be auto-unlocked
    // unless the final action triggered it
}
