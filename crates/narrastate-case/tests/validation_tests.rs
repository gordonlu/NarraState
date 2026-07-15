use narrastate_case::{adapt_v01, compile, validate_template, LegacyAdapterError};
use narrastate_core::{CaseDefinition, DisclosureKind, Ending, InterrogationPhase, VariantId};

fn legacy_case() -> CaseDefinition {
    serde_json::from_str(include_str!("../../../cases/rain-gallery/case.json")).unwrap()
}

#[test]
fn v01_adapter_preserves_runtime_content_in_default_variant() {
    let legacy = legacy_case();
    let expected_fact_count = legacy.facts.len();
    let expected_evidence_count = legacy.evidence.len();
    let template = adapt_v01(legacy, "1.0.0", VariantId::from("classic")).unwrap();

    assert_eq!(template.schema_version, "0.2");
    assert_eq!(template.solution_variants.len(), 1);
    let compiled = compile(&template, &VariantId::from("classic")).unwrap();
    assert_eq!(compiled.definition.facts.len(), expected_fact_count);
    assert_eq!(compiled.definition.evidence.len(), expected_evidence_count);
}

#[test]
fn v01_adapter_rejects_missing_responsible_character() {
    let mut legacy = legacy_case();
    for character in &mut legacy.characters {
        character
            .disclosure_graph
            .nodes
            .retain(|node| node.kind != DisclosureKind::Confession);
    }
    let result = adapt_v01(legacy, "1.0.0", VariantId::from("classic"));
    assert_eq!(
        result.unwrap_err(),
        LegacyAdapterError::AmbiguousResponsibleCharacter { found: 0 }
    );
}

#[test]
fn v01_adapter_does_not_invent_missing_ending() {
    let mut legacy = legacy_case();
    legacy.ending = None;
    let result = adapt_v01(legacy, "1.0.0", VariantId::from("classic"));
    assert_eq!(result.unwrap_err(), LegacyAdapterError::MissingEnding);
}

#[test]
fn identical_semantic_variants_are_rejected() {
    let mut template = adapt_v01(legacy_case(), "1.0.0", VariantId::from("classic")).unwrap();
    let mut duplicate = template.solution_variants[0].clone();
    duplicate.id = VariantId::from("same-truth-new-title");
    duplicate.title = "另一个标题".into();
    duplicate.ending = Ending {
        epilogue: "另一段结局文案".into(),
    };
    template.solution_variants.push(duplicate);

    let report = validate_template(&template);
    assert!(!report.valid);
    assert!(report
        .errors
        .iter()
        .any(|issue| issue.code == "VARIANT_INSUFFICIENT_DIVERGENCE"));
}

#[test]
fn unsupported_schema_version_is_observable() {
    let mut template = adapt_v01(legacy_case(), "1.0.0", VariantId::from("classic")).unwrap();
    template.schema_version = "9.9".into();
    let report = validate_template(&template);
    assert!(report
        .errors
        .iter()
        .any(|issue| issue.code == "SCHEMA_VERSION_UNSUPPORTED" && issue.path == "schema_version"));
}

#[test]
fn text_limit_counts_unicode_scalars_and_does_not_truncate() {
    let mut template = adapt_v01(legacy_case(), "1.0.0", VariantId::from("classic")).unwrap();
    template.summary = "案".repeat(narrastate_case::MAX_TEXT_CHARS + 1);
    let original = template.summary.clone();
    let report = validate_template(&template);
    assert!(report
        .errors
        .iter()
        .any(|issue| issue.code == "SCHEMA_TEXT_TOO_LONG"));
    assert_eq!(template.summary, original);
}

#[test]
fn compiled_but_unplayable_disclosure_path_fails_validation() {
    let mut template = adapt_v01(legacy_case(), "1.0.0", VariantId::from("classic")).unwrap();
    let responsible = template.solution_variants[0]
        .responsible_character_id
        .clone();
    let character = template
        .shared_characters
        .iter_mut()
        .find(|character| character.id == responsible)
        .unwrap();
    for node in &mut character.disclosure_graph.nodes {
        node.min_phase = InterrogationPhase::ConfessionEligible;
    }

    let report = validate_template(&template);
    assert!(!report.valid);
    assert!(report.errors.iter().any(|issue| {
        matches!(
            issue.code.as_str(),
            "DISCLOSURE_NODE_UNREACHABLE" | "SIMULATION_TURN_LIMIT"
        )
    }));
    assert!(report.variant_reports[0].simulation.is_some());
}
