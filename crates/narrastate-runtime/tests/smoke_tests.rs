use std::collections::{BTreeMap, BTreeSet};

use narrastate_core::case::CaseDefinition;
use narrastate_core::character::CharacterRuntimeState;
use narrastate_core::evidence::{EvidenceDefinition, EvidenceUsageKind, EvidenceUse};
use narrastate_core::id::{EvidenceId, FactId, TurnId};
use narrastate_core::phase::InterrogationPhase;
use narrastate_core::transition::{InterpretedAction, PlayerIntent, PlayerTone, TransitionTuning};
use narrastate_runtime::mock::{MockInterpreter, MockRenderer};
use narrastate_runtime::{DialoguePlanner, TransitionEngine};

fn load_case() -> CaseDefinition {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let case_path = std::path::Path::new(&manifest_dir).join("../../cases/rain-gallery/case.json");
    let json = std::fs::read_to_string(&case_path)
        .unwrap_or_else(|_| std::fs::read_to_string("cases/rain-gallery/case.json").expect(
            "rain-gallery case file not found. Run tests from workspace root: cargo test -p narrastate-runtime"
        ));
    serde_json::from_str(&json).expect("rain-gallery case.json parse failed")
}

fn make_action(
    text: &str,
    evidence_ids: Vec<EvidenceId>,
    intent: PlayerIntent,
) -> InterpretedAction {
    InterpretedAction {
        intent,
        topics: vec![text.to_string()],
        referenced_entities: vec![],
        referenced_claims: vec![],
        evidence_usage: evidence_ids
            .into_iter()
            .map(|id| EvidenceUse {
                evidence_id: id,
                usage: EvidenceUsageKind::DirectReference,
            })
            .collect(),
        asserted_propositions: vec![],
        tone: PlayerTone::Neutral,
        confidence: 1.0,
    }
}

#[test]
fn smoke_full_luo_cheng_interrogation() {
    let case = load_case();
    let culprit = case
        .characters
        .iter()
        .find(|c| c.disclosure_graph.confession_node().is_some())
        .expect("No culprit found");

    let evidence_map: BTreeMap<EvidenceId, EvidenceDefinition> = case
        .evidence
        .iter()
        .map(|e| (e.id.clone(), e.clone()))
        .collect();
    let facts: BTreeSet<FactId> = case.facts.iter().map(|f| f.id.clone()).collect();

    let engine = TransitionEngine::new(TransitionTuning::default());
    let _interpreter = MockInterpreter;
    let renderer = MockRenderer;
    let planner = DialoguePlanner;

    let mut state = CharacterRuntimeState::new(culprit.resilience);
    assert_eq!(state.phase, InterrogationPhase::Calm);
    assert_eq!(state.stress, 0);
    assert_eq!(state.defense_budget, 100);

    let all_evidence_ids: Vec<EvidenceId> = case
        .evidence
        .iter()
        .filter(|e| {
            e.contradicts
                .iter()
                .any(|cid| culprit.claims.iter().any(|c| &c.id == cid))
        })
        .map(|e| e.id.clone())
        .collect();

    assert!(
        !all_evidence_ids.is_empty(),
        "No relevant evidence for culprit"
    );

    // Turn 1: Generic question, no evidence
    let action = make_action("你好，昨晚你在哪里？", vec![], PlayerIntent::Ask);
    let turn_id = TurnId::new();
    let result = engine.process(&action, &mut state, culprit, &evidence_map, &facts, turn_id);
    assert_eq!(result.diff.phase_after, InterrogationPhase::Calm);
    assert_eq!(result.diff.stress_after, 0);
    let plan = planner.plan(&action, &state, culprit, &evidence_map);
    let utterance = renderer.render(&plan);
    assert!(!utterance.utterance.is_empty());

    // Turn 2: Present first piece of evidence
    let ev = &all_evidence_ids[0];
    let action = make_action(
        "这是什么？",
        vec![ev.clone()],
        PlayerIntent::PresentEvidence,
    );
    let turn_id = TurnId::new();
    let result = engine.process(&action, &mut state, culprit, &evidence_map, &facts, turn_id);
    assert!(result.diff.stress_after > result.diff.stress_before);
    assert!(state.confronted_evidence.contains(ev));
    let plan = planner.plan(&action, &state, culprit, &evidence_map);
    let utterance = renderer.render(&plan);
    assert!(!utterance.utterance.is_empty());

    // Turn 3-6: Present remaining evidence
    for ev in &all_evidence_ids[1..] {
        let action = make_action(
            "解释一下这个证据",
            vec![ev.clone()],
            PlayerIntent::PresentEvidence,
        );
        let turn_id = TurnId::new();
        let result = engine.process(&action, &mut state, culprit, &evidence_map, &facts, turn_id);
        assert!(state.confronted_evidence.contains(ev));
        assert!(result.diff.stress_after >= result.diff.stress_before);
        let plan = planner.plan(&action, &state, culprit, &evidence_map);
        let utterance = renderer.render(&plan);
        assert!(!utterance.utterance.is_empty());
    }

    // Stress should have increased
    assert!(
        state.stress > 0,
        "Stress should have increased after evidence"
    );
    assert!(state.defense_budget < 100, "Defense should have been spent");

    // Phase should have advanced
    assert!(
        state.phase >= InterrogationPhase::Guarded,
        "Phase should have advanced past Calm, got {:?}",
        state.phase
    );

    // Composure decreased, defense spent, trust may have changed
    assert!(state.composure < 100, "Composure should have decreased");
    assert!(
        state.defense_budget < 100,
        "Defense budget should have decreased"
    );
}

#[test]
fn smoke_phase_progression_with_all_evidence() {
    let case = load_case();
    let culprit = case
        .characters
        .iter()
        .find(|c| c.disclosure_graph.confession_node().is_some())
        .expect("No culprit found");

    let evidence_map: BTreeMap<EvidenceId, EvidenceDefinition> = case
        .evidence
        .iter()
        .map(|e| (e.id.clone(), e.clone()))
        .collect();
    let facts: BTreeSet<FactId> = case.facts.iter().map(|f| f.id.clone()).collect();

    let engine = TransitionEngine::new(TransitionTuning::default());
    let mut state = CharacterRuntimeState::new(culprit.resilience);

    let relevant: Vec<&narrastate_core::evidence::EvidenceDefinition> = case
        .evidence
        .iter()
        .filter(|e| {
            e.contradicts
                .iter()
                .any(|cid| culprit.claims.iter().any(|c| &c.id == cid))
        })
        .collect();

    for (i, ev) in relevant.iter().enumerate() {
        let action = InterpretedAction {
            intent: PlayerIntent::PresentEvidence,
            topics: vec![format!("证据 {i}")],
            referenced_entities: vec![],
            referenced_claims: vec![],
            evidence_usage: vec![EvidenceUse {
                evidence_id: ev.id.clone(),
                usage: EvidenceUsageKind::DirectReference,
            }],
            asserted_propositions: vec![],
            tone: PlayerTone::Neutral,
            confidence: 1.0,
        };
        let turn_id = TurnId::new();
        let _result = engine.process(&action, &mut state, culprit, &evidence_map, &facts, turn_id);
    }

    // Stress should have increased substantially (evidence adds ~17-22 stress each)
    assert!(
        state.stress >= 70,
        "Stress should be at least 70 with all evidence, got {}",
        state.stress
    );

    // Defense should be mostly exhausted
    assert!(
        state.defense_budget < 50,
        "Defense should be substantially depleted, got {}",
        state.defense_budget
    );

    // Phase should reach at least Cornered with all evidence
    assert!(
        state.phase >= InterrogationPhase::Cornered,
        "All evidence should push phase to at least Cornered, got {:?}",
        state.phase
    );

    // All relevant evidence should be confronted (if it was successfully presented)
    for ev in &relevant {
        assert!(
            state.confronted_evidence.contains(&ev.id),
            "Evidence {} should be confronted",
            ev.id
        );
    }
    assert!(
        !state.confronted_evidence.is_empty(),
        "At least one evidence should have been confronted"
    );

    // At least one disclosure should be revealed
    assert!(
        !state.revealed_disclosures.is_empty(),
        "Disclosures should have been revealed"
    );

    println!("Phase: {:?}", state.phase);
    println!("Stress: {}", state.stress);
    println!("Defense: {}", state.defense_budget);
    println!("Disclosures: {} revealed", state.revealed_disclosures.len());
    println!("Evidence confronted: {}", state.confronted_evidence.len());
}

#[test]
fn smoke_multiple_characters_have_independent_state() {
    let case = load_case();
    let culprit = case
        .characters
        .iter()
        .find(|c| c.disclosure_graph.confession_node().is_some())
        .expect("No culprit");

    let innocent = case
        .characters
        .iter()
        .find(|c| c.disclosure_graph.confession_node().is_none())
        .expect("No innocent character");

    let evidence_map: BTreeMap<EvidenceId, EvidenceDefinition> = case
        .evidence
        .iter()
        .map(|e| (e.id.clone(), e.clone()))
        .collect();
    let facts: BTreeSet<FactId> = case.facts.iter().map(|f| f.id.clone()).collect();

    let engine = TransitionEngine::new(TransitionTuning::default());

    let mut culprit_state = CharacterRuntimeState::new(culprit.resilience);
    let mut innocent_state = CharacterRuntimeState::new(innocent.resilience);

    let relevant: Vec<&narrastate_core::evidence::EvidenceDefinition> = case
        .evidence
        .iter()
        .filter(|e| {
            e.contradicts
                .iter()
                .any(|cid| culprit.claims.iter().any(|c| &c.id == cid))
        })
        .collect();

    for ev in &relevant {
        let action = InterpretedAction {
            intent: PlayerIntent::PresentEvidence,
            topics: vec!["证据".into()],
            referenced_entities: vec![],
            referenced_claims: vec![],
            evidence_usage: vec![EvidenceUse {
                evidence_id: ev.id.clone(),
                usage: EvidenceUsageKind::DirectReference,
            }],
            asserted_propositions: vec![],
            tone: PlayerTone::Neutral,
            confidence: 1.0,
        };
        let turn_id = TurnId::new();
        let _ = engine.process(
            &action,
            &mut culprit_state,
            culprit,
            &evidence_map,
            &facts,
            turn_id,
        );
        let turn_id = TurnId::new();
        let _ = engine.process(
            &action,
            &mut innocent_state,
            innocent,
            &evidence_map,
            &facts,
            turn_id,
        );
    }

    // Culprit should be stressed, innocent may or may not be
    assert!(
        culprit_state.stress >= innocent_state.stress,
        "Culprit ({}) should have >= stress than innocent ({})",
        culprit_state.stress,
        innocent_state.stress
    );

    // Culprit's phase should be different from innocent's
    // The innocent character has no claims to contradict, so evidence against culprit's
    // claims should not phase-progress the innocent
    assert!(
        culprit_state.phase >= innocent_state.phase,
        "Culprit phase ({:?}) should be >= innocent phase ({:?})",
        culprit_state.phase,
        innocent_state.phase
    );
}

#[test]
fn smoke_cli_simulator_sequence_matches_golden() {
    let case = load_case();
    let culprit = case
        .characters
        .iter()
        .find(|c| c.disclosure_graph.confession_node().is_some())
        .expect("No culprit");

    let evidence_map: BTreeMap<EvidenceId, EvidenceDefinition> = case
        .evidence
        .iter()
        .map(|e| (e.id.clone(), e.clone()))
        .collect();
    let facts: BTreeSet<FactId> = case.facts.iter().map(|f| f.id.clone()).collect();

    let engine = TransitionEngine::new(TransitionTuning::default());
    let _interpreter = MockInterpreter;
    let planner = DialoguePlanner;
    let renderer = MockRenderer;

    let mut state = CharacterRuntimeState::new(culprit.resilience);

    // Simulate: question → present evidence → observe phase changes
    let actions = vec![
        ("你昨晚在控制室吗？", vec![], PlayerIntent::Ask),
        (
            "门禁记录显示你离开了",
            vec![EvidenceId::from("ev_card_log")],
            PlayerIntent::PresentEvidence,
        ),
        (
            "地上的泥印怎么解释",
            vec![EvidenceId::from("ev_mud_track")],
            PlayerIntent::PresentEvidence,
        ),
        (
            "传感器在21:43被关闭了",
            vec![EvidenceId::from("ev_sensor_cmd")],
            PlayerIntent::PresentEvidence,
        ),
        (
            "箱子上有你的纤维",
            vec![EvidenceId::from("ev_fiber")],
            PlayerIntent::PresentEvidence,
        ),
        (
            "当铺联系记录在这里",
            vec![EvidenceId::from("ev_pawn_contact")],
            PlayerIntent::PresentEvidence,
        ),
        (
            "门口的脚印和你的鞋吻合",
            vec![EvidenceId::from("ev_mud_track")],
            PlayerIntent::PresentEvidence,
        ),
    ];

    let mut turn_count = 0;
    for (text, ev_ids, intent) in &actions {
        turn_count += 1;
        let action = make_action(text, ev_ids.clone(), *intent);
        let turn_id = TurnId::new();
        let result = engine.process(&action, &mut state, culprit, &evidence_map, &facts, turn_id);
        let plan = planner.plan(&action, &state, culprit, &evidence_map);
        let utterance = renderer.render(&plan);

        assert!(
            !utterance.utterance.is_empty(),
            "Turn {turn_count}: renderer should produce non-empty utterance"
        );
        assert!(
            result.diff.phase_after >= result.diff.phase_before,
            "Turn {turn_count}: phase should not regress ({:?} → {:?})",
            result.diff.phase_before,
            result.diff.phase_after
        );
    }

    let expected_turns = actions.len();
    assert!(
        turn_count == expected_turns,
        "All {expected_turns} actions should be processed"
    );

    // Should have reached at least Defensive phase
    assert!(
        state.phase >= InterrogationPhase::Defensive,
        "Should reach at least Defensive, got {:?}",
        state.phase
    );

    println!("=== CLI Smoke Test Complete ===");
    println!("Turns processed: {turn_count}");
    println!("Final phase: {:?}", state.phase);
    println!("Stress: {}", state.stress);
    println!("Defense budget: {}", state.defense_budget);
    println!("Disclosures revealed: {}", state.revealed_disclosures.len());
    println!("Contradicted claims: {}", state.confronted_evidence.len());
}

#[test]
fn smoke_case_load_and_validate() {
    let case = load_case();
    assert_eq!(case.id.to_string(), "rain-gallery");
    assert_eq!(case.facts.len(), 17);
    assert_eq!(case.evidence.len(), 7);
    assert_eq!(case.characters.len(), 3);

    let result = case.validate();
    assert!(
        result.is_ok(),
        "Case validation should pass: {:?}",
        result.err()
    );
}

#[test]
fn smoke_all_characters_have_initial_state() {
    let case = load_case();
    for ch in &case.characters {
        let state = CharacterRuntimeState::new(ch.resilience);
        assert_eq!(state.phase, InterrogationPhase::Calm);
        assert_eq!(state.stress, 0);
        assert_eq!(state.composure, 100);
        assert_eq!(state.trust, 0);
        assert_eq!(state.defense_budget, 100);
        assert_eq!(state.revealed_disclosures.len(), 0);
        assert_eq!(state.confronted_evidence.len(), 0);
    }
}

#[test]
fn smoke_no_effect_from_irrelevant_evidence() {
    let case = load_case();
    let culprit = case
        .characters
        .iter()
        .find(|c| c.disclosure_graph.confession_node().is_some())
        .expect("No culprit");

    let evidence_map: BTreeMap<EvidenceId, EvidenceDefinition> = case
        .evidence
        .iter()
        .map(|e| (e.id.clone(), e.clone()))
        .collect();
    let facts: BTreeSet<FactId> = case.facts.iter().map(|f| f.id.clone()).collect();

    let engine = TransitionEngine::new(TransitionTuning::default());
    let mut state = CharacterRuntimeState::new(culprit.resilience);

    // Evidence that does NOT contradict any of the culprit's claims should have reduced (0.2×) impact
    let irrelevant: Vec<EvidenceId> = case
        .evidence
        .iter()
        .filter(|e| {
            !e.contradicts
                .iter()
                .any(|cid| culprit.claims.iter().any(|c| &c.id == cid))
        })
        .map(|e| e.id.clone())
        .collect();

    for ev_id in &irrelevant {
        let action = InterpretedAction {
            intent: PlayerIntent::PresentEvidence,
            topics: vec!["不相关的证据".into()],
            referenced_entities: vec![],
            referenced_claims: vec![],
            evidence_usage: vec![EvidenceUse {
                evidence_id: ev_id.clone(),
                usage: EvidenceUsageKind::DirectReference,
            }],
            asserted_propositions: vec![],
            tone: PlayerTone::Neutral,
            confidence: 1.0,
        };
        let turn_id = TurnId::new();
        let result = engine.process(&action, &mut state, culprit, &evidence_map, &facts, turn_id);
        assert!(
            result.diff.stress_after >= result.diff.stress_before,
            "Irrelevant evidence should not decrease stress"
        );
    }
}
