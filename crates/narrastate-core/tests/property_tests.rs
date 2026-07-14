use narrastate_core::disclosure::{
    DisclosureGraph, DisclosureKind, DisclosureNode, DisclosurePrerequisite,
};
use narrastate_core::id::DisclosureId;
use narrastate_core::phase::InterrogationPhase;
use proptest::prelude::*;
use std::collections::BTreeSet;

proptest! {
    // ── Invariant 1: All numeric state values stay in legal range ────
    #[test]
    fn test_stress_stays_in_0_100(
        _initial_stress in 0u8..=100,
        deltas in prop::collection::vec(-200i32..=200, 1..=10),
    ) {
        let mut state = narrastate_core::CharacterRuntimeState::new(50);
        // Override initial stress (not directly mutable, so we apply deltas only)
        for d in deltas {
            state.apply_stress_delta(d);
        }
        assert!(
            state.stress <= 100,
            "stress {} exceeded 100",
            state.stress
        );
    }

    #[test]
    fn test_composure_stays_in_0_100(
        deltas in prop::collection::vec(-200i32..=200, 1..=10),
    ) {
        let mut state = narrastate_core::CharacterRuntimeState::new(50);
        for d in deltas {
            state.apply_composure_delta(d);
        }
        assert!(state.composure <= 100);
    }

    #[test]
    fn test_trust_stays_in_neg100_100(
        deltas in prop::collection::vec(-500i32..=500, 1..=10),
    ) {
        let mut state = narrastate_core::CharacterRuntimeState::new(50);
        for d in deltas {
            state.apply_trust_delta(d);
        }
        assert!(state.trust <= 100);
        assert!(state.trust >= -100);
    }

    #[test]
    fn test_defense_budget_stays_in_0_100(
        deltas in prop::collection::vec(-200i32..=200, 1..=10),
    ) {
        let mut state = narrastate_core::CharacterRuntimeState::new(50);
        for d in deltas {
            state.apply_defense_budget_delta(d);
        }
        assert!(state.defense_budget <= 100);
    }

    // ── Invariant 2: Phase transitions are legal ────────────────────
    #[test]
    fn test_phase_transition_never_illegal(
        from_idx in 0u8..7u8,
        to_idx in 0u8..7u8,
    ) {
        let phases = [
            InterrogationPhase::Calm,
            InterrogationPhase::Guarded,
            InterrogationPhase::Defensive,
            InterrogationPhase::Pressured,
            InterrogationPhase::Cornered,
            InterrogationPhase::ConfessionEligible,
            InterrogationPhase::Resolved,
        ];
        let from = phases[from_idx as usize];
        let to = phases[to_idx as usize];
        prop_assume!(from != to);

        let allowed = from.can_transition_to(to);
        // Verify set_phase returns Ok iff allowed
        let turn = narrastate_core::id::TurnId::new();
        let mut state = narrastate_core::CharacterRuntimeState::new(50);
        // Walk state forward until we reach `from`
        for p in &phases {
            if *p == from { break; }
            if state.phase.can_transition_to(*p) {
                let _ = state.set_phase(*p, turn.clone());
            }
        }
        // If state didn't reach `from`, skip
        if state.phase != from { return Ok(()); }

        let result = state.set_phase(to, turn.clone());
        if allowed {
            prop_assert!(result.is_ok(), "Legal transition {from:?} -> {to:?} should be OK");
        } else {
            prop_assert!(result.is_err(), "Illegal transition {from:?} -> {to:?} should be Err");
        }
    }

    // ── Invariant 3: Disclosure cannot unlock without prerequisites ──
    #[test]
    fn test_disclosure_requires_prerequisites(
        has_prerequisite in proptest::bool::ANY,
        is_revealed in proptest::bool::ANY,
    ) {
        let mut revealed = BTreeSet::new();
        if is_revealed {
            revealed.insert(DisclosureId::from("prereq"));
        }

        let graph = DisclosureGraph {
            nodes: vec![
                DisclosureNode {
                    id: DisclosureId::from("prereq"),
                    kind: DisclosureKind::Presence,
                    reveals: vec![],
                    prerequisites: vec![],
                    min_phase: InterrogationPhase::Calm,
                    response_intent: narrastate_core::DialogueAct::Answer,
                },
                DisclosureNode {
                    id: DisclosureId::from("target"),
                    kind: DisclosureKind::Access,
                    reveals: vec![],
                    prerequisites: if has_prerequisite {
                        vec![DisclosurePrerequisite::Disclosure { disclosure: DisclosureId::from("prereq") }]
                    } else {
                        vec![]
                    },
                    min_phase: InterrogationPhase::Guarded,
                    response_intent: narrastate_core::DialogueAct::Answer,
                },
            ],
        };

        let is_unlockable = graph.is_unlockable(
            &DisclosureId::from("target"),
            &revealed,
            InterrogationPhase::Guarded,
        );

        if has_prerequisite && !is_revealed {
            prop_assert!(!is_unlockable, "Target should not be unlockable when prerequisite exists but is not revealed");
        }
    }
}
