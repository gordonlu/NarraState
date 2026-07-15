use crate::canonical_hash;
use narrastate_core::{CaseDefinition, CaseTemplate, CompiledCase, ValidationError, VariantId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompileIssue {
    pub code: String,
    pub path: String,
    pub message: String,
    pub related_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompileReport {
    pub errors: Vec<CompileIssue>,
}

impl std::fmt::Display for CompileReport {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "case compilation failed with {} error(s)",
            self.errors.len()
        )
    }
}

impl std::error::Error for CompileReport {}

pub fn compile(
    template: &CaseTemplate,
    variant_id: &VariantId,
) -> Result<CompiledCase, CompileReport> {
    let mut issues = preflight(template);
    let Some(variant) = template
        .solution_variants
        .iter()
        .find(|variant| &variant.id == variant_id)
    else {
        issues.push(issue(
            "COMPILE_VARIANT_NOT_FOUND",
            "solution_variants",
            format!("variant {variant_id} does not exist"),
            vec![variant_id.to_string()],
        ));
        return Err(CompileReport { errors: issues });
    };

    let mut facts = template.shared_facts.clone();
    let mut evidence = template.shared_evidence.clone();
    let mut characters = template.shared_characters.clone();

    apply_replacements(
        &mut facts,
        &variant.fact_replacements,
        |item| &item.id,
        "fact_replacements",
        &mut issues,
    );
    apply_replacements(
        &mut evidence,
        &variant.evidence_replacements,
        |item| &item.id,
        "evidence_replacements",
        &mut issues,
    );
    apply_replacements(
        &mut characters,
        &variant.character_replacements,
        |item| &item.id,
        "character_replacements",
        &mut issues,
    );
    append_unique(
        &mut facts,
        &variant.additions.facts,
        |item| &item.id,
        "additions.facts",
        &mut issues,
    );
    append_unique(
        &mut evidence,
        &variant.additions.evidence,
        |item| &item.id,
        "additions.evidence",
        &mut issues,
    );

    if !characters
        .iter()
        .any(|character| character.id == variant.responsible_character_id)
    {
        issues.push(issue(
            "COMPILE_RESPONSIBLE_CHARACTER_MISSING",
            "responsible_character_id",
            "responsible character is not present after replacements",
            vec![variant.responsible_character_id.to_string()],
        ));
    }

    for (index, character) in characters.iter().enumerate() {
        let has_confession = character.disclosure_graph.confession_node().is_some();
        let is_responsible = character.id == variant.responsible_character_id;
        if has_confession != is_responsible {
            issues.push(issue(
                "COMPILE_RESPONSIBILITY_GRAPH_MISMATCH",
                format!("characters[{index}].disclosure_graph"),
                if is_responsible {
                    "responsible character must have the main confession path"
                } else {
                    "non-responsible character must not have a main confession path"
                },
                vec![character.id.to_string()],
            ));
        }
    }

    facts.sort_by(|left, right| left.id.cmp(&right.id));
    evidence.sort_by(|left, right| left.id.cmp(&right.id));
    characters.sort_by(|left, right| left.id.cmp(&right.id));

    let definition = CaseDefinition {
        schema_version: template.schema_version.clone(),
        id: template.id.clone(),
        title: template.title.clone(),
        summary: template.summary.clone(),
        locale: template.locale.clone(),
        required_case_elements: variant
            .required_case_elements
            .clone()
            .unwrap_or_else(|| template.required_case_elements.clone()),
        entities: template.entities.clone(),
        facts,
        evidence,
        characters,
        initial_player_knowledge: template.initial_player_knowledge.clone(),
        ending: Some(variant.ending.clone()),
    };

    if let Err(errors) = definition.validate() {
        issues.extend(errors.into_iter().map(validation_issue));
    }
    if !issues.is_empty() {
        return Err(CompileReport { errors: issues });
    }

    let hash_input = (
        &definition,
        &variant.id,
        &template.version,
        &template.schema_version,
    );
    let compiled_content_hash = canonical_hash(&hash_input).map_err(|error| CompileReport {
        errors: vec![issue("COMPILE_HASH_FAILED", "$", error.to_string(), vec![])],
    })?;
    Ok(CompiledCase {
        case_id: template.id.clone(),
        case_version: template.version.clone(),
        schema_version: template.schema_version.clone(),
        variant_id: variant.id.clone(),
        responsible_character_id: variant.responsible_character_id.clone(),
        compiled_content_hash,
        definition,
    })
}

fn preflight(template: &CaseTemplate) -> Vec<CompileIssue> {
    let mut issues = Vec::new();
    unique_ids(
        template.solution_variants.iter().map(|item| &item.id),
        "solution_variants[].id",
        &mut issues,
    );
    if !template
        .solution_variants
        .iter()
        .any(|variant| variant.id == template.default_variant_id)
    {
        issues.push(issue(
            "COMPILE_DEFAULT_VARIANT_NOT_FOUND",
            "default_variant_id",
            "default variant does not exist",
            vec![template.default_variant_id.to_string()],
        ));
    }
    issues
}

fn unique_ids<'a, T: Ord + ToString + 'a>(
    ids: impl Iterator<Item = &'a T>,
    path: &str,
    issues: &mut Vec<CompileIssue>,
) {
    let mut seen = BTreeSet::new();
    for id in ids {
        if !seen.insert(id) {
            let id = id.to_string();
            issues.push(issue(
                "COMPILE_DUPLICATE_ID",
                path,
                format!("duplicate ID {id}"),
                vec![id],
            ));
        }
    }
}

fn apply_replacements<K, T, F>(
    target: &mut [T],
    replacements: &std::collections::BTreeMap<K, T>,
    id: F,
    path: &str,
    issues: &mut Vec<CompileIssue>,
) where
    K: Ord + Eq + ToString,
    F: Fn(&T) -> &K,
    T: Clone,
{
    for (key, replacement) in replacements {
        let key_text = key.to_string();
        if id(replacement) != key {
            issues.push(issue(
                "COMPILE_ID_KEY_MISMATCH",
                format!("{path}.{key_text}"),
                format!(
                    "replacement ID {} does not match map key {key_text}",
                    id(replacement).to_string()
                ),
                vec![key_text, id(replacement).to_string()],
            ));
            continue;
        }
        let Some(existing) = target.iter_mut().find(|item| id(item) == key) else {
            issues.push(issue(
                "COMPILE_UNKNOWN_OVERRIDE_TARGET",
                format!("{path}.{key_text}"),
                "replacement target does not exist in shared content",
                vec![key_text],
            ));
            continue;
        };
        *existing = replacement.clone();
    }
}

fn append_unique<K, T, F>(
    target: &mut Vec<T>,
    additions: &[T],
    id: F,
    path: &str,
    issues: &mut Vec<CompileIssue>,
) where
    K: Ord + ToString,
    F: Fn(&T) -> &K,
    T: Clone,
{
    let mut ids: BTreeSet<String> = target.iter().map(|item| id(item).to_string()).collect();
    for (index, addition) in additions.iter().enumerate() {
        let value = id(addition).to_string();
        if !ids.insert(value.clone()) {
            issues.push(issue(
                "COMPILE_DUPLICATE_ID",
                format!("{path}[{index}].id"),
                format!("ID {value} already exists"),
                vec![value],
            ));
        } else {
            target.push(addition.clone());
        }
    }
}

fn validation_issue(error: ValidationError) -> CompileIssue {
    let text = error.to_string();
    match error {
        ValidationError::DuplicateId { field, id } => {
            issue("COMPILE_DUPLICATE_ID", field, text, vec![id])
        }
        ValidationError::ReferenceNotFound {
            field, reference, ..
        } => issue("COMPILE_UNRESOLVED_REFERENCE", field, text, vec![reference]),
        ValidationError::RequiredElementNotCovered { element } => issue(
            "COMPILE_REQUIRED_ELEMENTS_UNMAPPABLE",
            "required_case_elements",
            text,
            vec![element],
        ),
        ValidationError::DisclosureCycle { field, .. }
        | ValidationError::Semantic { field, .. } => {
            issue("COMPILE_SEMANTIC_VALIDATION_FAILED", field, text, vec![])
        }
        ValidationError::NoCulprit | ValidationError::CulpritUnreachable { .. } => issue(
            "COMPILE_RESPONSIBILITY_GRAPH_MISMATCH",
            "characters[].disclosure_graph",
            text,
            vec![],
        ),
    }
}

fn issue(
    code: impl Into<String>,
    path: impl Into<String>,
    message: impl Into<String>,
    related_ids: Vec<String>,
) -> CompileIssue {
    CompileIssue {
        code: code.into(),
        path: path.into(),
        message: message.into(),
        related_ids,
    }
}
