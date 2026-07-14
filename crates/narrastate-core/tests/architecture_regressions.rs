use narrastate_core::{
    ClaimId, DialogueAct, DisclosureGraph, DisclosureId, DisclosureKind, DisclosureNode,
    DisclosurePrerequisite, EvidenceId, FactId, InterrogationPhase,
};
use std::collections::BTreeSet;

fn case() -> narrastate_core::CaseDefinition {
    serde_json::from_str(include_str!("../../../cases/rain-gallery/case.json")).unwrap()
}

#[test]
fn phase_progression_cannot_skip_intermediate_stages() {
    assert!(!InterrogationPhase::Calm.can_transition_to(InterrogationPhase::Defensive));
    assert!(!InterrogationPhase::Calm.can_transition_to(InterrogationPhase::Cornered));
    assert!(
        !InterrogationPhase::Defensive.can_transition_to(InterrogationPhase::ConfessionEligible)
    );
}

#[test]
fn evidence_and_claim_prerequisites_are_both_enforced() {
    let evidence = EvidenceId::from("e1");
    let claim = ClaimId::from("c1");
    let graph = DisclosureGraph {
        nodes: vec![DisclosureNode {
            id: DisclosureId::from("d1"),
            kind: DisclosureKind::Presence,
            reveals: vec![FactId::from("f1")],
            prerequisites: vec![
                DisclosurePrerequisite::EvidencePresented {
                    evidence: vec![evidence.clone()],
                },
                DisclosurePrerequisite::ClaimInvalidated {
                    claim: claim.clone(),
                },
            ],
            min_phase: InterrogationPhase::Guarded,
            response_intent: DialogueAct::PartialAdmission,
        }],
    };
    let revealed = BTreeSet::new();
    let mut presented = BTreeSet::new();
    let mut invalidated = BTreeSet::new();
    presented.insert(evidence);
    assert!(!graph.is_unlockable_with_context(
        &DisclosureId::from("d1"),
        &revealed,
        InterrogationPhase::Guarded,
        &presented,
        &invalidated,
    ));
    invalidated.insert(claim);
    assert!(graph.is_unlockable_with_context(
        &DisclosureId::from("d1"),
        &revealed,
        InterrogationPhase::Guarded,
        &presented,
        &invalidated,
    ));
}

#[test]
fn non_finite_evidence_strength_is_rejected_with_field_path() {
    let mut case = case();
    case.evidence[0].reliability = f32::NAN;
    let errors = case.validate().expect_err("NaN must fail validation");
    assert!(errors
        .iter()
        .any(|error| error.to_string().contains("evidence[0].reliability")));
}

#[test]
fn claim_owner_mismatch_is_rejected_with_field_path() {
    let mut case = case();
    case.characters[0].claims[0].owner = narrastate_core::CharacterId::from("shen-an");
    let errors = case.validate().expect_err("owner mismatch must fail");
    assert!(errors
        .iter()
        .any(|error| error.to_string().contains("characters[0].claims[0].owner")));
}

#[test]
fn hidden_fact_cannot_be_initial_player_knowledge() {
    let mut case = case();
    case.initial_player_knowledge
        .fact_ids
        .push(FactId::from("fact_painting_hidden"));
    let errors = case
        .validate()
        .expect_err("hidden initial knowledge must fail");
    assert!(errors.iter().any(|error| {
        error
            .to_string()
            .contains("initial_player_knowledge.fact_ids")
    }));
}
