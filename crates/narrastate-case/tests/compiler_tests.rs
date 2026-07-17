use narrastate_case::{compile, freeze_case};
use narrastate_core::{
    CaseDefinition, CaseTemplate, CharacterId, DisclosureKind, Ending, FactId, Seed,
    SolutionVariant, VariantAdditions, VariantId,
};
use std::collections::BTreeMap;

fn template() -> CaseTemplate {
    let definition: CaseDefinition =
        serde_json::from_str(include_str!("../../../cases/rain-gallery/case.json")).unwrap();
    CaseTemplate {
        schema_version: "0.2".into(),
        id: definition.id,
        version: "1.0.0".into(),
        title: definition.title,
        summary: definition.summary,
        locale: definition.locale,
        required_case_elements: definition.required_case_elements,
        entities: definition.entities,
        shared_facts: definition.facts,
        shared_evidence: definition.evidence,
        shared_characters: definition.characters,
        initial_player_knowledge: definition.initial_player_knowledge,
        solution_variants: vec![SolutionVariant {
            id: VariantId::from("classic"),
            title: "经典真相".into(),
            description: "罗成是责任人".into(),
            weight: 1,
            enabled: true,
            responsible_character_id: CharacterId::from("luo-cheng"),
            fact_replacements: BTreeMap::new(),
            evidence_replacements: BTreeMap::new(),
            character_replacements: BTreeMap::new(),
            additions: VariantAdditions::default(),
            required_case_elements: None,
            ending: definition.ending.unwrap_or(Ending {
                epilogue: "案件结束".into(),
            }),
        }],
        default_variant_id: VariantId::from("classic"),
    }
}

fn has_code(
    result: &Result<narrastate_core::CompiledCase, narrastate_case::CompileReport>,
    code: &str,
) -> bool {
    result
        .as_ref()
        .unwrap_err()
        .errors
        .iter()
        .any(|issue| issue.code == code)
}

#[test]
fn compiles_legacy_definition_into_frozen_variant() {
    let compiled = compile(&template(), &VariantId::from("classic")).unwrap();
    assert_eq!(compiled.variant_id, VariantId::from("classic"));
    assert!(compiled
        .compiled_content_hash
        .as_ref()
        .starts_with("sha256:"));

    let first = freeze_case(compiled.clone(), Seed(42));
    let second = freeze_case(compiled, Seed(42));
    assert_eq!(first.instance_hash, second.instance_hash);
    assert_ne!(first.instance_id, second.instance_id);
}

#[test]
fn rejects_override_of_unknown_shared_id() {
    let mut template = template();
    let mut replacement = template.shared_facts[0].clone();
    replacement.id = FactId::from("unknown");
    template.solution_variants[0]
        .fact_replacements
        .insert(FactId::from("unknown"), replacement);

    let result = compile(&template, &VariantId::from("classic"));
    assert!(has_code(&result, "COMPILE_UNKNOWN_OVERRIDE_TARGET"));
}

#[test]
fn rejects_replacement_whose_id_differs_from_map_key() {
    let mut template = template();
    let replacement = template.shared_facts[0].clone();
    template.solution_variants[0]
        .fact_replacements
        .insert(FactId::from("different-key"), replacement);

    let result = compile(&template, &VariantId::from("classic"));
    assert!(has_code(&result, "COMPILE_ID_KEY_MISMATCH"));
}

#[test]
fn rejects_variant_addition_that_reuses_shared_id() {
    let mut template = template();
    template.solution_variants[0]
        .additions
        .facts
        .push(template.shared_facts[0].clone());

    let result = compile(&template, &VariantId::from("classic"));
    assert!(has_code(&result, "COMPILE_DUPLICATE_ID"));
}

#[test]
fn rejects_responsible_character_without_matching_confession_graph() {
    let mut template = template();
    template.solution_variants[0].responsible_character_id = CharacterId::from("shen-an");

    let result = compile(&template, &VariantId::from("classic"));
    assert!(has_code(&result, "COMPILE_RESPONSIBILITY_GRAPH_MISMATCH"));
}

#[test]
fn allows_evidence_only_variant_without_a_confession_node() {
    let mut template = template();
    let responsible = template.solution_variants[0]
        .responsible_character_id
        .clone();
    template
        .shared_characters
        .iter_mut()
        .find(|character| character.id == responsible)
        .unwrap()
        .disclosure_graph
        .nodes
        .retain(|node| node.kind != DisclosureKind::Confession);

    compile(&template, &VariantId::from("classic")).unwrap();
}

#[test]
fn rejects_missing_default_variant() {
    let mut template = template();
    template.default_variant_id = VariantId::from("missing");

    let result = compile(&template, &VariantId::from("classic"));
    assert!(has_code(&result, "COMPILE_DEFAULT_VARIANT_NOT_FOUND"));
}

#[test]
fn unresolved_reference_keeps_stable_code_and_field_path() {
    let mut template = template();
    template.shared_characters[0]
        .knowledge
        .push(FactId::from("missing-fact"));

    let issue = compile(&template, &VariantId::from("classic"))
        .unwrap_err()
        .errors
        .into_iter()
        .find(|issue| issue.code == "COMPILE_UNRESOLVED_REFERENCE")
        .expect("unresolved reference issue");
    assert!(issue.path.starts_with("characters["));
    assert_eq!(issue.related_ids, vec!["missing-fact"]);
}
