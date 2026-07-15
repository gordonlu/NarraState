use narrastate_core::{
    CaseDefinition, CaseTemplate, CharacterId, Ending, SolutionVariant, VariantAdditions, VariantId,
};
use std::collections::BTreeMap;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LegacyAdapterError {
    #[error("legacy case must contain exactly one confession graph; found {found}")]
    AmbiguousResponsibleCharacter { found: usize },
    #[error("legacy case has no ending")]
    MissingEnding,
}

/// Converts a validated v0.1 runtime case into a v0.2 author template with one default variant.
pub fn adapt_v01(
    case: CaseDefinition,
    version: impl Into<String>,
    variant_id: VariantId,
) -> Result<CaseTemplate, LegacyAdapterError> {
    let responsible: Vec<CharacterId> = case
        .characters
        .iter()
        .filter(|character| character.disclosure_graph.confession_node().is_some())
        .map(|character| character.id.clone())
        .collect();
    if responsible.len() != 1 {
        return Err(LegacyAdapterError::AmbiguousResponsibleCharacter {
            found: responsible.len(),
        });
    }
    let ending: Ending = case.ending.ok_or(LegacyAdapterError::MissingEnding)?;
    Ok(CaseTemplate {
        schema_version: "0.2".into(),
        id: case.id,
        version: version.into(),
        title: case.title,
        summary: case.summary,
        locale: case.locale,
        required_case_elements: case.required_case_elements,
        entities: case.entities,
        shared_facts: case.facts,
        shared_evidence: case.evidence,
        shared_characters: case.characters,
        initial_player_knowledge: case.initial_player_knowledge,
        solution_variants: vec![SolutionVariant {
            id: variant_id.clone(),
            title: "Classic".into(),
            description: "Migrated from a fixed-truth v0.1 case".into(),
            weight: 1,
            enabled: true,
            responsible_character_id: responsible.into_iter().next().expect("checked length"),
            fact_replacements: BTreeMap::new(),
            evidence_replacements: BTreeMap::new(),
            character_replacements: BTreeMap::new(),
            additions: VariantAdditions::default(),
            required_case_elements: None,
            ending,
        }],
        default_variant_id: variant_id,
    })
}
