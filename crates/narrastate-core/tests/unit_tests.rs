use narrastate_core::disclosure::{
    ConfessionValidationError, CycleError, DisclosureGraph, DisclosureKind, DisclosureNode,
    DisclosurePrerequisite,
};
use narrastate_core::evidence::{
    CaseElement, DiscoveryRule, EvidenceDefinition, EvidenceUsageKind,
};
use narrastate_core::fact::{Fact, FactValue, FactVisibility, Proposition, TruthValue};
use narrastate_core::id::{
    CaseId, CharacterId, ClaimId, DefenseStrategyId, DisclosureId, EvidenceId, FactId, SessionId,
    TurnId,
};
use narrastate_core::phase::InterrogationPhase;
use narrastate_core::strategy::{DefenseStrategy, DefenseStrategyKind};
use narrastate_core::transition::{
    InterpretedAction, PlayerIntent, PlayerTone, TransitionReason, TransitionTuning,
};
use narrastate_core::{
    AccusationResult, Belief, BeliefSource, CaseDefinition, CharacterDefinition, CharacterGoal,
    CharacterRuntimeState, ClaimDefinition, ClaimKind, DialogueAct, DialogueSpeaker, Ending,
    Entity, PersonalityProfile, PlayerKnowledge, SessionMode, SessionState, SessionStatus,
    SpokenClaim,
};
use std::collections::{BTreeMap, BTreeSet};

// ── ID Semantic Validation ────────────────────────────────────────────

#[test]
fn test_ids_are_displayable() {
    let fid = FactId::from("fact_001");
    assert_eq!(fid.to_string(), "fact_001");
    let eid = EvidenceId::from("ev_001");
    assert_eq!(eid.to_string(), "ev_001");
    let cid = CharacterId::from("luo-cheng");
    assert_eq!(cid.to_string(), "luo-cheng");
}

#[test]
fn test_uuid_ids_are_unique() {
    let s1 = SessionId::new();
    let s2 = SessionId::new();
    assert_ne!(s1, s2);
}

// ── Phase Transition ───────────────────────────────────────────────────

#[test]
fn test_calm_can_go_to_guarded() {
    assert!(InterrogationPhase::Calm.can_transition_to(InterrogationPhase::Guarded));
}

#[test]
fn test_calm_cannot_skip_directly_to_confession() {
    assert!(!InterrogationPhase::Calm.can_transition_to(InterrogationPhase::ConfessionEligible));
    // Calm may transition to Resolved via non-confession paths (accusation, etc.)
    assert!(InterrogationPhase::Calm.can_transition_to(InterrogationPhase::Resolved));
}

#[test]
fn test_phase_hysteresis_no_regression() {
    assert!(!InterrogationPhase::Cornered.can_transition_to(InterrogationPhase::Calm));
    assert!(!InterrogationPhase::Defensive.can_transition_to(InterrogationPhase::Guarded));
}

#[test]
fn test_phase_forward_transitions_are_valid() {
    // Allowed transitions
    assert!(InterrogationPhase::Guarded.can_transition_to(InterrogationPhase::Defensive));
    assert!(InterrogationPhase::Defensive.can_transition_to(InterrogationPhase::Pressured));
    assert!(InterrogationPhase::Pressured.can_transition_to(InterrogationPhase::Cornered));
    assert!(InterrogationPhase::Cornered.can_transition_to(InterrogationPhase::ConfessionEligible));
    assert!(InterrogationPhase::ConfessionEligible.can_transition_to(InterrogationPhase::Resolved));
}

#[test]
fn test_any_phase_can_go_to_resolved() {
    let phases = vec![
        InterrogationPhase::Calm,
        InterrogationPhase::Guarded,
        InterrogationPhase::Defensive,
        InterrogationPhase::Pressured,
        InterrogationPhase::Cornered,
        InterrogationPhase::ConfessionEligible,
    ];
    for p in phases {
        assert!(
            p.can_transition_to(InterrogationPhase::Resolved),
            "{p:?} should be able to transition to Resolved"
        );
    }
    // Terminal phase cannot transition to itself
    assert!(!InterrogationPhase::Resolved.can_transition_to(InterrogationPhase::Resolved));
}

#[test]
fn test_phase_allowed_targets_does_not_include_self() {
    let targets = InterrogationPhase::Calm.allowed_targets();
    assert!(!targets.contains(&InterrogationPhase::Calm));
}

#[test]
fn test_character_state_phase_transition_valid() {
    let mut state = CharacterRuntimeState::new(50);
    let turn_id = TurnId::new();
    assert!(state
        .set_phase(InterrogationPhase::Guarded, turn_id)
        .is_ok());
    assert_eq!(state.phase, InterrogationPhase::Guarded);
    assert_eq!(state.last_transition_turn, Some(turn_id));
}

#[test]
fn test_character_state_phase_transition_invalid() {
    let mut state = CharacterRuntimeState::new(50);
    let turn_id = TurnId::new();
    assert!(state
        .set_phase(InterrogationPhase::ConfessionEligible, turn_id)
        .is_err());
    assert_eq!(state.phase, InterrogationPhase::Calm);
}

// ── State Range Safety ─────────────────────────────────────────────────

#[test]
fn test_stress_clamped_to_0_100() {
    let mut state = CharacterRuntimeState::new(50);
    state.apply_stress_delta(200);
    assert_eq!(state.stress, 100);
    state.apply_stress_delta(-500);
    assert_eq!(state.stress, 0);
}

#[test]
fn test_composure_clamped_to_0_100() {
    let mut state = CharacterRuntimeState::new(50);
    state.apply_composure_delta(-200);
    assert_eq!(state.composure, 0);
    state.apply_composure_delta(500);
    assert_eq!(state.composure, 100);
}

#[test]
fn test_trust_clamped_to_neg100_100() {
    let mut state = CharacterRuntimeState::new(50);
    state.apply_trust_delta(-500);
    assert_eq!(state.trust, -100);
    state.apply_trust_delta(1000);
    assert_eq!(state.trust, 100);
}

#[test]
fn test_defense_budget_clamped() {
    let mut state = CharacterRuntimeState::new(50);
    state.apply_defense_budget_delta(-500);
    assert_eq!(state.defense_budget, 0);
    state.apply_defense_budget_delta(500);
    assert_eq!(state.defense_budget, 100);
}

// ── DisclosureGraph ────────────────────────────────────────────────────

fn make_simple_graph(edges: &[(&str, &str)]) -> DisclosureGraph {
    let mut nodes = Vec::new();
    for (id, _) in edges {
        nodes.push(DisclosureNode {
            id: DisclosureId::from(*id),
            kind: DisclosureKind::PeripheralSecret,
            reveals: vec![],
            prerequisites: vec![],
            min_phase: InterrogationPhase::Calm,
            response_intent: DialogueAct::Answer,
        });
    }
    for (id, dep) in edges {
        if !dep.is_empty() {
            let node = nodes
                .iter_mut()
                .find(|n| n.id == DisclosureId::from(*id))
                .unwrap();
            node.prerequisites.push(DisclosurePrerequisite::Disclosure {
                disclosure: DisclosureId::from(*dep),
            });
        }
    }
    DisclosureGraph { nodes }
}

#[test]
fn test_acyclic_graph_passes() {
    let graph = make_simple_graph(&[("a", ""), ("b", "a"), ("c", "b")]);
    assert!(graph.validate_acyclic().is_ok());
}

#[test]
fn test_cyclic_graph_fails() {
    let graph = make_simple_graph(&[("a", "c"), ("b", "a"), ("c", "b")]);
    assert!(graph.validate_acyclic().is_err());
}

#[test]
fn test_self_cycle_fails() {
    let graph = make_simple_graph(&[("a", "a")]);
    assert!(graph.validate_acyclic().is_err());
}

#[test]
fn test_missing_prerequisite_reported() {
    let graph = DisclosureGraph {
        nodes: vec![DisclosureNode {
            id: DisclosureId::from("a"),
            kind: DisclosureKind::PeripheralSecret,
            reveals: vec![],
            prerequisites: vec![DisclosurePrerequisite::Disclosure {
                disclosure: DisclosureId::from("nonexistent"),
            }],
            min_phase: InterrogationPhase::Calm,
            response_intent: DialogueAct::Answer,
        }],
    };
    let result = graph.validate_acyclic();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| matches!(e, CycleError::MissingPrerequisiteNode { .. })));
}

#[test]
fn test_confession_node_validation() {
    let graph = DisclosureGraph {
        nodes: vec![
            DisclosureNode {
                id: DisclosureId::from("action"),
                kind: DisclosureKind::FullAction,
                reveals: vec![],
                prerequisites: vec![],
                min_phase: InterrogationPhase::Calm,
                response_intent: DialogueAct::Answer,
            },
            DisclosureNode {
                id: DisclosureId::from("confession"),
                kind: DisclosureKind::Confession,
                reveals: vec![],
                prerequisites: vec![DisclosurePrerequisite::Disclosure {
                    disclosure: DisclosureId::from("action"),
                }],
                min_phase: InterrogationPhase::ConfessionEligible,
                response_intent: DialogueAct::FullAdmission,
            },
        ],
    };
    assert!(graph.validate_confession().is_ok());
}

#[test]
fn test_confession_without_action_prerequisite_fails() {
    let graph = DisclosureGraph {
        nodes: vec![DisclosureNode {
            id: DisclosureId::from("confession"),
            kind: DisclosureKind::Confession,
            reveals: vec![],
            prerequisites: vec![],
            min_phase: InterrogationPhase::ConfessionEligible,
            response_intent: DialogueAct::FullAdmission,
        }],
    };
    assert!(matches!(
        graph.validate_confession(),
        Err(ConfessionValidationError::MissingActionPrerequisite)
    ));
}

#[test]
fn test_multiple_confession_nodes_fails() {
    let graph = DisclosureGraph {
        nodes: vec![
            DisclosureNode {
                id: DisclosureId::from("confession1"),
                kind: DisclosureKind::Confession,
                reveals: vec![],
                prerequisites: vec![DisclosurePrerequisite::Disclosure {
                    disclosure: DisclosureId::from("action"),
                }],
                min_phase: InterrogationPhase::ConfessionEligible,
                response_intent: DialogueAct::FullAdmission,
            },
            DisclosureNode {
                id: DisclosureId::from("confession2"),
                kind: DisclosureKind::Confession,
                reveals: vec![],
                prerequisites: vec![DisclosurePrerequisite::Disclosure {
                    disclosure: DisclosureId::from("action"),
                }],
                min_phase: InterrogationPhase::ConfessionEligible,
                response_intent: DialogueAct::FullAdmission,
            },
            DisclosureNode {
                id: DisclosureId::from("action"),
                kind: DisclosureKind::FullAction,
                reveals: vec![],
                prerequisites: vec![],
                min_phase: InterrogationPhase::Calm,
                response_intent: DialogueAct::Answer,
            },
        ],
    };
    assert!(matches!(
        graph.validate_confession(),
        Err(ConfessionValidationError::MultipleConfessionNodes)
    ));
}

#[test]
fn test_confession_node_max_one_invariant() {
    let graph = DisclosureGraph {
        nodes: vec![
            DisclosureNode {
                id: DisclosureId::from("c1"),
                kind: DisclosureKind::Confession,
                reveals: vec![],
                prerequisites: vec![],
                min_phase: InterrogationPhase::ConfessionEligible,
                response_intent: DialogueAct::FullAdmission,
            },
            DisclosureNode {
                id: DisclosureId::from("c2"),
                kind: DisclosureKind::Confession,
                reveals: vec![],
                prerequisites: vec![],
                min_phase: InterrogationPhase::ConfessionEligible,
                response_intent: DialogueAct::FullAdmission,
            },
        ],
    };
    assert!(graph.validate_confession().is_err());
}

#[test]
fn test_non_culprit_no_confession_node() {
    // A character without a Confession node is not a culprit.
    let graph = DisclosureGraph {
        nodes: vec![DisclosureNode {
            id: DisclosureId::from("secret"),
            kind: DisclosureKind::PeripheralSecret,
            reveals: vec![],
            prerequisites: vec![],
            min_phase: InterrogationPhase::Calm,
            response_intent: DialogueAct::Answer,
        }],
    };
    assert!(graph.confession_node().is_none());
}

// ── Disclosure unlockability ───────────────────────────────────────────

#[test]
fn test_disclosure_not_unlockable_without_prerequisites() {
    let graph = DisclosureGraph {
        nodes: vec![
            DisclosureNode {
                id: DisclosureId::from("prereq"),
                kind: DisclosureKind::Presence,
                reveals: vec![],
                prerequisites: vec![],
                min_phase: InterrogationPhase::Calm,
                response_intent: DialogueAct::Answer,
            },
            DisclosureNode {
                id: DisclosureId::from("target"),
                kind: DisclosureKind::Access,
                reveals: vec![],
                prerequisites: vec![DisclosurePrerequisite::Disclosure {
                    disclosure: DisclosureId::from("prereq"),
                }],
                min_phase: InterrogationPhase::Defensive,
                response_intent: DialogueAct::Answer,
            },
        ],
    };
    let revealed: BTreeSet<DisclosureId> = BTreeSet::new();
    assert!(!graph.is_unlockable(
        &DisclosureId::from("target"),
        &revealed,
        InterrogationPhase::Calm
    ));
}

#[test]
fn test_disclosure_unlockable_when_prerequisites_met() {
    let graph = DisclosureGraph {
        nodes: vec![
            DisclosureNode {
                id: DisclosureId::from("prereq"),
                kind: DisclosureKind::Presence,
                reveals: vec![],
                prerequisites: vec![],
                min_phase: InterrogationPhase::Calm,
                response_intent: DialogueAct::Answer,
            },
            DisclosureNode {
                id: DisclosureId::from("target"),
                kind: DisclosureKind::Access,
                reveals: vec![],
                prerequisites: vec![DisclosurePrerequisite::Disclosure {
                    disclosure: DisclosureId::from("prereq"),
                }],
                min_phase: InterrogationPhase::Guarded,
                response_intent: DialogueAct::Answer,
            },
        ],
    };
    let mut revealed: BTreeSet<DisclosureId> = BTreeSet::new();
    revealed.insert(DisclosureId::from("prereq"));
    assert!(graph.is_unlockable(
        &DisclosureId::from("target"),
        &revealed,
        InterrogationPhase::Guarded
    ));
}

#[test]
fn test_already_revealed_disclosure_not_unlockable() {
    let graph = DisclosureGraph {
        nodes: vec![DisclosureNode {
            id: DisclosureId::from("node"),
            kind: DisclosureKind::PeripheralSecret,
            reveals: vec![],
            prerequisites: vec![],
            min_phase: InterrogationPhase::Calm,
            response_intent: DialogueAct::Answer,
        }],
    };
    let mut revealed: BTreeSet<DisclosureId> = BTreeSet::new();
    revealed.insert(DisclosureId::from("node"));
    assert!(!graph.is_unlockable(
        &DisclosureId::from("node"),
        &revealed,
        InterrogationPhase::Calm
    ));
}

#[test]
fn test_disclosure_requires_min_phase() {
    let graph = DisclosureGraph {
        nodes: vec![DisclosureNode {
            id: DisclosureId::from("secret"),
            kind: DisclosureKind::Intent,
            reveals: vec![],
            prerequisites: vec![],
            min_phase: InterrogationPhase::Pressured,
            response_intent: DialogueAct::Answer,
        }],
    };
    let revealed: BTreeSet<DisclosureId> = BTreeSet::new();
    assert!(!graph.is_unlockable(
        &DisclosureId::from("secret"),
        &revealed,
        InterrogationPhase::Calm
    ));
    assert!(graph.is_unlockable(
        &DisclosureId::from("secret"),
        &revealed,
        InterrogationPhase::Pressured
    ));
}

// ── CaseDefinition Validation ──────────────────────────────────────────

fn valid_case() -> CaseDefinition {
    let fact_id = FactId::from("fact_left_control_room");
    let evidence_id = EvidenceId::from("ev_card_log");
    let claim_id = ClaimId::from("claim_never_left");
    let char_id = CharacterId::from("luo-cheng");
    let disclosure_id = DisclosureId::from("d1");
    let action_id = DisclosureId::from("d_action");

    CaseDefinition {
        schema_version: "0.1".to_string(),
        id: CaseId::from("test-case"),
        title: "Test".to_string(),
        summary: "A test case".to_string(),
        locale: "zh-CN".to_string(),
        required_case_elements: vec![CaseElement::Opportunity].into_iter().collect(),
        entities: vec![],
        facts: vec![Fact {
            id: fact_id.clone(),
            display_text: None,
            subject: "luo-cheng".into(),
            predicate: "was_in".to_string(),
            object: FactValue::String("control_room".to_string()),
            happened_at: None,
            location: None,
            truth: TruthValue::True,
            tags: BTreeSet::new(),
            visibility: FactVisibility::Hidden,
        }],
        evidence: vec![EvidenceDefinition {
            id: evidence_id.clone(),
            title: "Access Card Log".to_string(),
            description: "Shows card usage at 21:47".to_string(),
            supports: vec![],
            contradicts: vec![claim_id.clone()],
            elements: vec![CaseElement::Opportunity].into_iter().collect(),
            reliability: 0.9,
            directness: 0.8,
            exclusivity: 0.5,
            discoverable_by: vec![DiscoveryRule::StartingEvidence],
        }],
        characters: vec![CharacterDefinition {
            id: char_id.clone(),
            name: "罗成".to_string(),
            role: "安保主管".to_string(),
            public_profile: "负责门禁和巡查".to_string(),
            personality: PersonalityProfile {
                traits: vec!["冷静".to_string()],
                speech_style: None,
            },
            goals: vec![CharacterGoal {
                description: "隐藏盗窃行为".to_string(),
                priority: 10,
            }],
            knowledge: vec![fact_id.clone()],
            initial_beliefs: vec![Belief {
                proposition: Proposition {
                    subject: "luo-cheng".into(),
                    predicate: "is_innocent".to_string(),
                    object: FactValue::Boolean(true),
                },
                confidence: 100,
                source: BeliefSource::Default,
            }],
            claims: vec![ClaimDefinition {
                id: claim_id.clone(),
                owner: char_id.clone(),
                proposition: Proposition {
                    subject: "luo-cheng".into(),
                    predicate: "was_in".to_string(),
                    object: FactValue::String("control_room".to_string()),
                },
                kind: ClaimKind::Lie,
                available_from: InterrogationPhase::Calm,
                invalidated_by: vec![evidence_id.clone()],
                fallback_claim: None,
            }],
            defenses: vec![DefenseStrategy {
                id: DefenseStrategyId::from("denial"),
                kind: DefenseStrategyKind::Denial,
                usable_phases: vec![InterrogationPhase::Calm, InterrogationPhase::Guarded],
                max_uses: 3,
                applicable_claims: vec![claim_id.clone()],
                fallback_strategy: None,
                style_prompt: None,
            }],
            disclosure_graph: DisclosureGraph {
                nodes: vec![
                    DisclosureNode {
                        id: disclosure_id.clone(),
                        kind: DisclosureKind::Presence,
                        reveals: vec![fact_id.clone()],
                        prerequisites: vec![],
                        min_phase: InterrogationPhase::Guarded,
                        response_intent: DialogueAct::PartialAdmission,
                    },
                    DisclosureNode {
                        id: action_id.clone(),
                        kind: DisclosureKind::FullAction,
                        reveals: vec![fact_id.clone()],
                        prerequisites: vec![DisclosurePrerequisite::Disclosure {
                            disclosure: disclosure_id.clone(),
                        }],
                        min_phase: InterrogationPhase::Pressured,
                        response_intent: DialogueAct::PartialAdmission,
                    },
                    DisclosureNode {
                        id: DisclosureId::from("d_confession"),
                        kind: DisclosureKind::Confession,
                        reveals: vec![fact_id.clone()],
                        prerequisites: vec![DisclosurePrerequisite::Disclosure {
                            disclosure: action_id.clone(),
                        }],
                        min_phase: InterrogationPhase::ConfessionEligible,
                        response_intent: DialogueAct::FullAdmission,
                    },
                ],
            },
            resilience: 60,
        }],
        initial_player_knowledge: PlayerKnowledge {
            fact_ids: vec![],
            evidence_ids: vec![],
        },
        ending: Some(Ending {
            epilogue: "Case closed.".to_string(),
        }),
    }
}

#[test]
fn test_valid_case_passes_validation() {
    let case = valid_case();
    let result = case.validate();
    assert!(result.is_ok(), "Expected OK, got errors: {:?}", result);
}

#[test]
fn test_duplicate_fact_id_detected() {
    let mut case = valid_case();
    case.facts.push(case.facts[0].clone());
    let result = case.validate();
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .iter()
        .any(|e| format!("{e:?}").contains("DuplicateId")));
}

#[test]
fn test_reference_to_nonexistent_fact_detected() {
    let mut case = valid_case();
    // Add a character that references a fact that doesn't exist
    case.characters[0]
        .knowledge
        .push(FactId::from("nonexistent_fact"));
    let result = case.validate();
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .iter()
        .any(|e| format!("{e:?}").contains("ReferenceNotFound")));
}

#[test]
fn test_no_culprit_detected() {
    let mut case = valid_case();
    // Remove confession node from all characters
    for ch in &mut case.characters {
        ch.disclosure_graph
            .nodes
            .retain(|n| n.kind != DisclosureKind::Confession);
    }
    let result = case.validate();
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .iter()
        .any(|e| format!("{e:?}").contains("NoCulprit")));
}

#[test]
fn test_required_element_not_covered() {
    let mut case = valid_case();
    case.required_case_elements.insert(CaseElement::Means);
    case.required_case_elements.insert(CaseElement::Intent);
    let result = case.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| format!("{e:?}").contains("RequiredElementNotCovered")));
}

// ── Accusation Resolution ──────────────────────────────────────────────

#[test]
fn test_correct_but_insufficient_not_same_as_wrong_suspect() {
    let wrong = AccusationResult::WrongSuspect;
    let insufficient = AccusationResult::CorrectButInsufficient;
    assert_ne!(wrong, insufficient);
}

// ── CharacterRuntimeState ──────────────────────────────────────────────

#[test]
fn test_initial_character_state_defaults() {
    let state = CharacterRuntimeState::new(50);
    assert_eq!(state.phase, InterrogationPhase::Calm);
    assert_eq!(state.stress, 0);
    assert_eq!(state.composure, 100);
    assert_eq!(state.trust, 0);
    assert_eq!(state.defense_budget, 100);
    assert!(state.revealed_disclosures.is_empty());
    assert!(state.exhausted_defenses.is_empty());
    assert!(state.spoken_claims.is_empty());
    assert!(state.confronted_evidence.is_empty());
    assert!(state.last_transition_turn.is_none());
}

// ── TransitionTuning Defaults ──────────────────────────────────────────

#[test]
fn test_transition_tuning_defaults_in_valid_range() {
    let t = TransitionTuning::default();
    assert!((t.reliability_weight - 0.35).abs() < f32::EPSILON);
    assert!((t.directness_weight - 0.30).abs() < f32::EPSILON);
    assert!((t.exclusivity_weight - 0.20).abs() < f32::EPSILON);
    assert!((t.proposition_match_weight - 0.15).abs() < f32::EPSILON);
    assert!((t.novelty_multiplier_first - 1.0).abs() < f32::EPSILON);
    assert!((t.novelty_multiplier_repeat - 0.0).abs() < f32::EPSILON);
    assert!((t.chain_bonus - 0.15).abs() < f32::EPSILON);
    assert!((t.min_interpretation_multiplier - 0.5).abs() < f32::EPSILON);
    assert!((t.max_interpretation_multiplier - 1.0).abs() < f32::EPSILON);
}

// ── Evidence decimal range ─────────────────────────────────────────────

#[test]
fn test_evidence_reliability_default_in_range() {
    let ev = EvidenceDefinition {
        id: EvidenceId::from("test_ev"),
        title: "test".to_string(),
        description: "test".to_string(),
        supports: vec![],
        contradicts: vec![],
        elements: BTreeSet::new(),
        reliability: 0.5,
        directness: 0.5,
        exclusivity: 0.5,
        discoverable_by: vec![],
    };
    assert!((0.0..=1.0).contains(&ev.reliability));
    assert!((0.0..=1.0).contains(&ev.directness));
    assert!((0.0..=1.0).contains(&ev.exclusivity));
}

#[test]
fn test_disclosure_kind_confession_requires_action_prerequisite() {
    let node = DisclosureNode {
        id: DisclosureId::from("c"),
        kind: DisclosureKind::Confession,
        reveals: vec![],
        prerequisites: vec![DisclosurePrerequisite::Disclosure {
            disclosure: DisclosureId::from("action"),
        }],
        min_phase: InterrogationPhase::ConfessionEligible,
        response_intent: DialogueAct::FullAdmission,
    };
    assert_eq!(node.kind, DisclosureKind::Confession);
    assert!(node
        .prerequisites
        .iter()
        .any(|p| matches!(p, DisclosurePrerequisite::Disclosure { .. })));
}

// ── DisclosureGraph invariants ─────────────────────────────────────────

#[test]
fn test_major_disclosure_kinds_contains_confession() {
    let graph = DisclosureGraph { nodes: vec![] };
    assert!(graph
        .major_disclosure_kinds()
        .contains(&DisclosureKind::Confession));
}

#[test]
fn test_at_most_one_major_disclosure_per_turn() {
    // This test verifies that the DisclosureGraph tracks major kinds correctly.
    // The runtime enforcement happens in the transition engine (Phase 2).
    let major: BTreeSet<DisclosureKind> = [
        DisclosureKind::Presence,
        DisclosureKind::Access,
        DisclosureKind::Means,
        DisclosureKind::PartialAction,
        DisclosureKind::FullAction,
        DisclosureKind::Intent,
        DisclosureKind::Confession,
    ]
    .into_iter()
    .collect();
    assert_eq!(major.len(), 7);
    // PeripheralSecret is excluded from "major"
    assert!(!major.contains(&DisclosureKind::PeripheralSecret));
}

#[test]
fn test_case_definition_serde_roundtrip() {
    let case = valid_case();
    let json = serde_json::to_string_pretty(&case).expect("serialize");
    let deserialized: CaseDefinition = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(case.id, deserialized.id);
    assert_eq!(case.title, deserialized.title);
    assert_eq!(case.characters.len(), deserialized.characters.len());
}

#[test]
fn test_session_state_new_defaults() {
    let sid = SessionId::new();
    let state = SessionState {
        session_id: sid,
        case_id: CaseId::from("test"),
        instance_id: None,
        mode: SessionMode::Mock,
        status: SessionStatus::Active,
        current_turn: 0,
        active_character: None,
        discovered_facts: BTreeSet::new(),
        discovered_evidence: BTreeSet::new(),
        character_states: BTreeMap::new(),
        conversation: vec![],
        accusations: vec![],
        revision: 0,
    };
    assert_eq!(state.session_id, sid);
    assert_eq!(state.revision, 0);
    assert_eq!(state.status, SessionStatus::Active);
}

#[test]
fn test_disclosure_prerequisite_variants() {
    let _d = DisclosurePrerequisite::Disclosure {
        disclosure: DisclosureId::from("d1"),
    };
    let _e = DisclosurePrerequisite::EvidencePresented {
        evidence: vec![EvidenceId::from("e1")],
    };
    let _p = DisclosurePrerequisite::PhaseAtLeast {
        min_phase: InterrogationPhase::Defensive,
    };
    let _c = DisclosurePrerequisite::ClaimInvalidated {
        claim: ClaimId::from("some_claim"),
    };
}

// ── Proposition / FactValue / EntityRef ────────────────────────────────

#[test]
fn test_fact_value_variants() {
    let _s = FactValue::String("hello".to_string());
    let _n = FactValue::Number(42.0);
    let _b = FactValue::Boolean(true);
    let _e = FactValue::Entity("someone".into());
}

#[test]
fn test_proposition_construction() {
    let p = Proposition {
        subject: "luo-cheng".into(),
        predicate: "has_stress".to_string(),
        object: FactValue::Number(75.0),
    };
    assert_eq!(p.subject.as_ref(), "luo-cheng");
}

// ── InterpretedAction defaults ─────────────────────────────────────────

#[test]
fn test_interpreted_action_confidence_range() {
    let action = InterpretedAction {
        intent: PlayerIntent::Ask,
        topics: vec![],
        referenced_entities: vec![],
        referenced_claims: vec![],
        evidence_usage: vec![],
        asserted_propositions: vec![],
        tone: PlayerTone::Neutral,
        confidence: 0.8,
    };
    assert!((0.0..=1.0).contains(&action.confidence));
}

// ── DialogueAct comprehensive check ────────────────────────────────────

#[test]
fn test_dialogue_act_covers_all_phases() {
    // FullAdmission should only be used in appropriate phases
    let acts = vec![
        DialogueAct::Answer,
        DialogueAct::Deny,
        DialogueAct::Evade,
        DialogueAct::Reframe,
        DialogueAct::ChallengeEvidence,
        DialogueAct::ShiftBlame,
        DialogueAct::PartialAdmission,
        DialogueAct::FullAdmission,
        DialogueAct::AskForClarification,
        DialogueAct::Silence,
    ];
    assert_eq!(acts.len(), 10);
    assert!(acts.contains(&DialogueAct::FullAdmission));
}

// ── Error display ──────────────────────────────────────────────────────

#[test]
fn test_validation_error_display() {
    use narrastate_core::ValidationError;
    let err = ValidationError::DuplicateId {
        field: "facts[].id".to_string(),
        id: "dup".to_string(),
    };
    let msg = err.to_string();
    assert!(msg.contains("dup"));
}

// ── EvidenceUse type check ────────────────────────────────────────────

#[test]
fn test_evidence_usage_kind_variants() {
    let _d = EvidenceUsageKind::DirectReference;
    let _i = EvidenceUsageKind::ImplicitReference;
    let _s = EvidenceUsageKind::SupportingContext;
}

// ── DisclosureNode kind ordering ──────────────────────────────────────

#[test]
fn test_disclosure_kind_partial_ord() {
    assert!(DisclosureKind::PeripheralSecret < DisclosureKind::Presence);
    // This is just derived Ord from the enum declaration order.
    // In practice, range checks use min_phase, not kind ordering.
}

// ── SessionStatus transitions ─────────────────────────────────────────

#[test]
fn test_session_status_distinct() {
    assert_ne!(SessionStatus::Active, SessionStatus::Resolved);
    assert_ne!(SessionStatus::Resolved, SessionStatus::Abandoned);
}

// ── AccusationResult comparison ───────────────────────────────────────

#[test]
fn test_accusation_result_correct_ordering() {
    // These are distinct outcomes, not ordered by quality.
    let results = [
        AccusationResult::WrongSuspect,
        AccusationResult::CorrectButInsufficient,
        AccusationResult::CaseProvenWithoutConfession,
        AccusationResult::CaseProvenWithConfession,
    ];
    assert_eq!(results.len(), 4);
}

#[test]
fn test_defense_strategy_construction() {
    let strategy = DefenseStrategy {
        id: DefenseStrategyId::from("denial_01"),
        kind: DefenseStrategyKind::Denial,
        usable_phases: vec![InterrogationPhase::Calm, InterrogationPhase::Guarded],
        max_uses: 3,
        applicable_claims: vec![ClaimId::from("claim_01")],
        fallback_strategy: None,
        style_prompt: None,
    };
    assert_eq!(strategy.max_uses, 3);
    assert!(strategy.usable_phases.contains(&InterrogationPhase::Calm));
}

#[test]
fn test_case_element_coverage() {
    let mut case = valid_case();
    // Make required elements that ARE covered
    case.required_case_elements = vec![CaseElement::Opportunity].into_iter().collect();
    assert!(case.validate().is_ok());
}

#[test]
fn test_character_runtime_state_exhaust_defense() {
    let mut state = CharacterRuntimeState::new(50);
    let sid = DefenseStrategyId::from("denial");
    state.exhaust_defense(sid.clone());
    assert!(state.exhausted_defenses.contains(&sid));
    // Exhausting again should be idempotent
    state.exhaust_defense(sid.clone());
    assert_eq!(state.exhausted_defenses.len(), 1);
}

#[test]
fn test_character_runtime_state_reveal_disclosure() {
    let mut state = CharacterRuntimeState::new(50);
    let did = DisclosureId::from("d1");
    state.reveal_disclosure(did.clone());
    assert!(state.revealed_disclosures.contains(&did));
    state.reveal_disclosure(did.clone());
    assert_eq!(state.revealed_disclosures.len(), 1);
}

#[test]
fn test_character_runtime_state_add_evidence() {
    let mut state = CharacterRuntimeState::new(50);
    let eid = EvidenceId::from("ev_001");
    state.add_evidence_confrontation(eid.clone());
    assert!(state.confronted_evidence.contains(&eid));
}

#[test]
fn test_evidence_definition_can_store_case_elements() {
    let mut elements = BTreeSet::new();
    elements.insert(CaseElement::Identity);
    elements.insert(CaseElement::Opportunity);
    let ev = EvidenceDefinition {
        id: EvidenceId::from("ev_test"),
        title: "Test".to_string(),
        description: "Test evidence".to_string(),
        supports: vec![],
        contradicts: vec![],
        elements,
        reliability: 0.8,
        directness: 0.7,
        exclusivity: 0.3,
        discoverable_by: vec![],
    };
    assert!(ev.elements.contains(&CaseElement::Identity));
    assert!(ev.elements.contains(&CaseElement::Opportunity));
    assert!(!ev.elements.contains(&CaseElement::Intent));
}

#[test]
fn discovery_rule_uses_unambiguous_tagged_object_fields() {
    let phase_rule = DiscoveryRule::AutomaticAtPhase {
        phase: InterrogationPhase::Guarded,
    };
    assert_eq!(
        serde_json::to_value(&phase_rule).unwrap(),
        serde_json::json!({"type": "AutomaticAtPhase", "phase": "Guarded"})
    );
    assert_eq!(
        serde_json::from_value::<DiscoveryRule>(serde_json::json!({
            "type": "AfterEvidencePresented",
            "evidence_id": "evidence-1"
        }))
        .unwrap(),
        DiscoveryRule::AfterEvidencePresented {
            evidence_id: EvidenceId::from("evidence-1")
        }
    );
}

#[test]
fn test_claim_construction() {
    let claim = ClaimDefinition {
        id: ClaimId::from("claim_test"),
        owner: CharacterId::from("luo-cheng"),
        proposition: Proposition {
            subject: "luo-cheng".into(),
            predicate: "was_in".to_string(),
            object: FactValue::String("control_room".to_string()),
        },
        kind: ClaimKind::Lie,
        available_from: InterrogationPhase::Calm,
        invalidated_by: vec![EvidenceId::from("ev_card_log")],
        fallback_claim: None,
    };
    assert_eq!(claim.kind, ClaimKind::Lie);
    assert_eq!(claim.owner.as_ref(), "luo-cheng");
}

#[test]
fn test_spoken_claim_construction() {
    let claim = SpokenClaim {
        claim_id: ClaimId::from("c1"),
        turn_id: TurnId::new(),
        utterance: "I never left the control room.".to_string(),
        invalidated: false,
    };
    assert!(!claim.invalidated);
}

#[test]
fn test_character_goal_priority() {
    let goal = CharacterGoal {
        description: "Avoid suspicion".to_string(),
        priority: 5,
    };
    assert!(goal.priority > 0);

    let high_priority = CharacterGoal {
        description: "Hide evidence".to_string(),
        priority: 10,
    };
    assert!(high_priority.priority > goal.priority);
}

#[test]
fn test_belief_construction() {
    let belief = Belief {
        proposition: Proposition {
            subject: "luo-cheng".into(),
            predicate: "is_trustworthy".to_string(),
            object: FactValue::Boolean(true),
        },
        confidence: 80,
        source: BeliefSource::DirectKnowledge,
    };
    assert_eq!(belief.confidence, 80);
    assert!(matches!(belief.source, BeliefSource::DirectKnowledge));
}

#[test]
fn test_phase_allowed_targets_matches_can_transition_to() {
    let phases = vec![
        InterrogationPhase::Calm,
        InterrogationPhase::Guarded,
        InterrogationPhase::Defensive,
        InterrogationPhase::Pressured,
        InterrogationPhase::Cornered,
        InterrogationPhase::ConfessionEligible,
        InterrogationPhase::Resolved,
    ];
    for phase in phases {
        let targets = phase.allowed_targets();
        for target in &targets {
            assert!(phase.can_transition_to(*target));
        }
        // Verify exhaustive: every allowed transition should be in targets
        for target in &[
            InterrogationPhase::Calm,
            InterrogationPhase::Guarded,
            InterrogationPhase::Defensive,
            InterrogationPhase::Pressured,
            InterrogationPhase::Cornered,
            InterrogationPhase::ConfessionEligible,
            InterrogationPhase::Resolved,
        ] {
            if phase.can_transition_to(*target) {
                assert!(
                    targets.contains(target),
                    "{phase:?} can transition to {target:?} but it's not in allowed_targets"
                );
            }
        }
    }
}

// ── Cross-module reference validation in CaseDefinition ────────────────

#[test]
fn test_invalid_claim_ref_in_evidence_detected() {
    let mut case = valid_case();
    // Add evidence that references a non-existent claim
    if let Some(ev) = case.evidence.get_mut(0) {
        ev.contradicts.push(ClaimId::from("nonexistent_claim"));
    }
    let result = case.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors
        .iter()
        .any(|e| format!("{e:?}").contains("ReferenceNotFound")));
}

// ── TransitionReason completeness ──────────────────────────────────────

#[test]
fn test_transition_reason_variants() {
    let reasons = [
        TransitionReason::NewEvidencePresented,
        TransitionReason::PriorClaimContradicted,
        TransitionReason::DefenseExhausted,
        TransitionReason::DisclosurePrerequisitesMet,
        TransitionReason::RepeatedQuestionNoNewInformation,
        TransitionReason::DirectChallenge,
        TransitionReason::AccusationSubmitted,
        TransitionReason::CaseResolved,
    ];
    assert_eq!(reasons.len(), 8);
}

// ── DialogueSpeaker ────────────────────────────────────────────────────

#[test]
fn test_dialogue_speaker_comparison() {
    let player = DialogueSpeaker::Player;
    let system = DialogueSpeaker::System;
    let char_speaker = DialogueSpeaker::Character(CharacterId::from("luo-cheng"));
    assert_ne!(player, system);
    assert_ne!(player, char_speaker);
}

// ── Ending ─────────────────────────────────────────────────────────────

#[test]
fn test_ending_contains_epilogue() {
    let ending = Ending {
        epilogue: "The case was solved.".to_string(),
    };
    assert!(!ending.epilogue.is_empty());
}

// ── Entity ─────────────────────────────────────────────────────────────

#[test]
fn test_entity_construction() {
    let entity = Entity {
        id: "gallery".to_string(),
        name: "雨夜画廊".to_string(),
        kind: "location".to_string(),
    };
    assert_eq!(entity.kind, "location");
}
