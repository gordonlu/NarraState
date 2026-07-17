use narrastate_case::{adapt_v01, compile, simulate_case, SimulationFailure, SimulationLimits};
use narrastate_core::{CaseDefinition, VariantId};

fn compiled_case() -> narrastate_core::CompiledCase {
    let legacy: CaseDefinition =
        serde_json::from_str(include_str!("../../../cases/rain-gallery/case.json")).unwrap();
    let template = adapt_v01(legacy, "1.0.0", VariantId::from("classic")).unwrap();
    compile(&template, &VariantId::from("classic")).unwrap()
}

#[test]
fn valid_case_has_deterministic_legal_path_to_ending() {
    let first = simulate_case(&compiled_case(), SimulationLimits::default());
    let second = simulate_case(&compiled_case(), SimulationLimits::default());
    assert!(first.success, "failure: {:?}", first.failure_reason);
    assert_eq!(first.turns, second.turns);
    assert_eq!(first.acquired_evidence_ids, second.acquired_evidence_ids);
    assert_eq!(
        first.reached_disclosure_nodes,
        second.reached_disclosure_nodes
    );
    assert!(first.trace.iter().any(|step| matches!(
        step.action,
        narrastate_case::SimulationAction::SubmitAccusation { .. }
    )));
}

#[test]
fn hidden_required_evidence_cannot_be_used_by_simulator() {
    let mut case = compiled_case();
    for evidence in &mut case.definition.evidence {
        if evidence
            .elements
            .iter()
            .any(|element| case.definition.required_case_elements.contains(element))
        {
            evidence.discoverable_by.clear();
        }
    }
    let result = simulate_case(&case, SimulationLimits::default());
    assert!(!result.success);
    assert_eq!(
        result.failure_reason,
        Some(SimulationFailure::NoPathToRequiredEvidence)
    );
}

#[test]
fn false_suspect_confession_is_rejected_before_search() {
    let mut case = compiled_case();
    let confession_graph = case
        .definition
        .characters
        .iter()
        .find(|character| character.id == case.responsible_character_id)
        .unwrap()
        .disclosure_graph
        .clone();
    case.definition
        .characters
        .iter_mut()
        .find(|character| character.id != case.responsible_character_id)
        .unwrap()
        .disclosure_graph = confession_graph;

    let result = simulate_case(&case, SimulationLimits::default());
    assert_eq!(
        result.failure_reason,
        Some(SimulationFailure::FalseSuspectCanConfess)
    );
}

#[test]
fn state_limit_has_stable_failure_code() {
    let result = simulate_case(
        &compiled_case(),
        SimulationLimits {
            max_states: 0,
            ..SimulationLimits::default()
        },
    );
    assert_eq!(result.failure_reason, Some(SimulationFailure::StateLimit));
    assert_eq!(
        result.failure_reason.unwrap().code(),
        "SIMULATION_STATE_LIMIT"
    );
}

#[test]
fn turn_limit_is_not_silently_treated_as_unreachable() {
    let result = simulate_case(
        &compiled_case(),
        SimulationLimits {
            max_turns: 0,
            ..SimulationLimits::default()
        },
    );
    assert_eq!(result.failure_reason, Some(SimulationFailure::TurnLimit));
}

#[test]
fn evidence_complete_action_disclosure_can_finish_through_decisive_follow_ups() {
    let mut case = compiled_case();
    for evidence in &mut case.definition.evidence {
        evidence.reliability = 0.0;
        evidence.directness = 0.0;
        evidence.exclusivity = 0.0;
    }
    let responsible = case
        .definition
        .characters
        .iter_mut()
        .find(|character| character.id == case.responsible_character_id)
        .unwrap();
    for node in &mut responsible.disclosure_graph.nodes {
        if node.kind != narrastate_core::DisclosureKind::Confession {
            node.min_phase = narrastate_core::InterrogationPhase::Calm;
            for prerequisite in &mut node.prerequisites {
                if let narrastate_core::DisclosurePrerequisite::PhaseAtLeast { min_phase } =
                    prerequisite
                {
                    *min_phase = narrastate_core::InterrogationPhase::Calm;
                }
            }
        }
    }

    let result = simulate_case(&case, SimulationLimits::default());

    assert!(result.success, "failure: {:?}", result.failure_reason);
    assert!(result.trace.iter().any(|step| matches!(
        step.action,
        narrastate_case::SimulationAction::AskDecisiveFollowUp(_)
    )));
}
