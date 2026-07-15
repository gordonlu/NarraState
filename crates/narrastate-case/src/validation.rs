use crate::{canonical_hash, compile, simulate_case, SimulationLimits, SimulationResult};
use narrastate_core::{
    CaseTemplate, CompiledCase, EvidenceDefinition, Fact, SolutionVariant, VariantId,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const EXPECTED_SCHEMA_VERSION: &str = "0.2";
pub const MIN_CHARACTERS: usize = 2;
pub const MAX_CHARACTERS: usize = 6;
pub const MAX_EVIDENCE: usize = 30;
pub const MAX_ENABLED_VARIANTS: usize = 5;
pub const MAX_TEXT_CHARS: usize = 4_000;
pub const MIN_DIVERGENCE_DIMENSIONS: usize = 2;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ValidationSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidationIssue {
    pub code: String,
    pub severity: ValidationSeverity,
    pub path: String,
    pub message: String,
    pub related_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariantValidationReport {
    pub variant_id: VariantId,
    pub valid: bool,
    pub issues: Vec<ValidationIssue>,
    pub simulation: Option<SimulationResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseValidationReport {
    pub valid: bool,
    pub errors: Vec<ValidationIssue>,
    pub warnings: Vec<ValidationIssue>,
    pub variant_reports: Vec<VariantValidationReport>,
}

pub fn validate_template(template: &CaseTemplate) -> CaseValidationReport {
    let mut errors = validate_limits(template);
    let enabled: Vec<_> = template
        .solution_variants
        .iter()
        .filter(|variant| variant.enabled)
        .collect();
    if enabled.is_empty() {
        errors.push(error(
            "SCHEMA_NO_ENABLED_VARIANTS",
            "solution_variants",
            "at least one solution variant must be enabled",
            vec![],
        ));
    }

    let mut compiled = BTreeMap::new();
    let mut variant_reports = Vec::new();
    for variant in enabled {
        match compile(template, &variant.id) {
            Ok(value) => {
                let simulation = simulate_case(&value, SimulationLimits::default());
                let mut issues = Vec::new();
                if let Some(reason) = &simulation.failure_reason {
                    issues.push(error(
                        reason.code(),
                        format!("solution_variants[{}]", variant.id),
                        "deterministic simulation found no legal completion path",
                        vec![variant.id.to_string()],
                    ));
                }
                errors.extend(issues.clone());
                compiled.insert(variant.id.clone(), value);
                variant_reports.push(VariantValidationReport {
                    variant_id: variant.id.clone(),
                    valid: simulation.success,
                    issues,
                    simulation: Some(simulation),
                });
            }
            Err(report) => {
                let issues: Vec<_> = report
                    .errors
                    .into_iter()
                    .map(|issue| ValidationIssue {
                        code: issue.code,
                        severity: ValidationSeverity::Error,
                        path: format!("solution_variants[{}].{}", variant.id, issue.path),
                        message: issue.message,
                        related_ids: issue.related_ids,
                    })
                    .collect();
                errors.extend(issues.clone());
                variant_reports.push(VariantValidationReport {
                    variant_id: variant.id.clone(),
                    valid: false,
                    issues,
                    simulation: None,
                });
            }
        }
    }
    errors.extend(validate_divergence(template, &compiled));
    let valid = errors.is_empty();
    CaseValidationReport {
        valid,
        errors,
        warnings: vec![],
        variant_reports,
    }
}

fn validate_limits(template: &CaseTemplate) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    if template.schema_version != EXPECTED_SCHEMA_VERSION {
        issues.push(error(
            "SCHEMA_VERSION_UNSUPPORTED",
            "schema_version",
            format!("expected schema version {EXPECTED_SCHEMA_VERSION}"),
            vec![template.schema_version.clone()],
        ));
    }
    if !(MIN_CHARACTERS..=MAX_CHARACTERS).contains(&template.shared_characters.len()) {
        issues.push(error(
            "SCHEMA_CHARACTER_COUNT_OUT_OF_RANGE",
            "shared_characters",
            format!("character count must be in {MIN_CHARACTERS}..={MAX_CHARACTERS}"),
            vec![],
        ));
    }
    if template.shared_evidence.len() > MAX_EVIDENCE {
        issues.push(error(
            "SCHEMA_EVIDENCE_COUNT_EXCEEDED",
            "shared_evidence",
            format!("evidence count must not exceed {MAX_EVIDENCE}"),
            vec![],
        ));
    }
    let enabled_count = template
        .solution_variants
        .iter()
        .filter(|variant| variant.enabled)
        .count();
    if enabled_count > MAX_ENABLED_VARIANTS {
        issues.push(error(
            "SCHEMA_VARIANT_COUNT_EXCEEDED",
            "solution_variants",
            format!("enabled variant count must not exceed {MAX_ENABLED_VARIANTS}"),
            vec![],
        ));
    }
    for (path, value) in [
        ("title", template.title.as_str()),
        ("summary", template.summary.as_str()),
    ] {
        if value.chars().count() > MAX_TEXT_CHARS {
            issues.push(error(
                "SCHEMA_TEXT_TOO_LONG",
                path,
                format!("text must not exceed {MAX_TEXT_CHARS} Unicode scalar values"),
                vec![],
            ));
        }
    }
    issues
}

fn validate_divergence(
    template: &CaseTemplate,
    compiled: &BTreeMap<VariantId, CompiledCase>,
) -> Vec<ValidationIssue> {
    let enabled: Vec<&SolutionVariant> = template
        .solution_variants
        .iter()
        .filter(|variant| variant.enabled && compiled.contains_key(&variant.id))
        .collect();
    let mut issues = Vec::new();
    for left_index in 0..enabled.len() {
        for right_index in (left_index + 1)..enabled.len() {
            let left = enabled[left_index];
            let right = enabled[right_index];
            let dimensions = divergence_dimensions(
                compiled.get(&left.id).expect("filtered above"),
                compiled.get(&right.id).expect("filtered above"),
            );
            if dimensions.len() < MIN_DIVERGENCE_DIMENSIONS {
                issues.push(error(
                    "VARIANT_INSUFFICIENT_DIVERGENCE",
                    "solution_variants",
                    format!(
                        "variants {} and {} differ in only {} semantic dimension(s); required {}",
                        left.id,
                        right.id,
                        dimensions.len(),
                        MIN_DIVERGENCE_DIMENSIONS
                    ),
                    vec![left.id.to_string(), right.id.to_string()],
                ));
            }
        }
    }
    issues
}

fn divergence_dimensions(left: &CompiledCase, right: &CompiledCase) -> BTreeSet<&'static str> {
    let mut changed = BTreeSet::new();
    if left.responsible_character_id != right.responsible_character_id {
        changed.insert("responsible_character");
    }
    compare_hash(
        &mut changed,
        "motive_facts",
        &tagged(&left.definition.facts, "motive"),
        &tagged(&right.definition.facts, "motive"),
    );
    compare_hash(
        &mut changed,
        "action_facts",
        &tagged(&left.definition.facts, "action"),
        &tagged(&right.definition.facts, "action"),
    );
    compare_hash(
        &mut changed,
        "decisive_evidence",
        &decisive_evidence(left),
        &decisive_evidence(right),
    );
    compare_hash(
        &mut changed,
        "contradicted_claims",
        &contradicted_claims(left),
        &contradicted_claims(right),
    );
    let left_graph = left
        .definition
        .characters
        .iter()
        .find(|character| character.id == left.responsible_character_id)
        .map(|character| &character.disclosure_graph);
    let right_graph = right
        .definition
        .characters
        .iter()
        .find(|character| character.id == right.responsible_character_id)
        .map(|character| &character.disclosure_graph);
    compare_hash(&mut changed, "disclosure_graph", &left_graph, &right_graph);
    changed
}

fn compare_hash<T: Serialize>(
    changed: &mut BTreeSet<&'static str>,
    name: &'static str,
    left: &T,
    right: &T,
) {
    if canonical_hash(left).ok() != canonical_hash(right).ok() {
        changed.insert(name);
    }
}

fn tagged<'a>(facts: &'a [Fact], tag: &str) -> Vec<&'a Fact> {
    facts
        .iter()
        .filter(|fact| fact.tags.contains(tag))
        .collect()
}

fn decisive_evidence(case: &CompiledCase) -> Vec<&EvidenceDefinition> {
    case.definition
        .evidence
        .iter()
        .filter(|evidence| {
            evidence
                .elements
                .iter()
                .any(|element| case.definition.required_case_elements.contains(element))
        })
        .collect()
}

fn contradicted_claims(case: &CompiledCase) -> BTreeSet<String> {
    case.definition
        .evidence
        .iter()
        .flat_map(|evidence| evidence.contradicts.iter().map(ToString::to_string))
        .collect()
}

fn error(
    code: impl Into<String>,
    path: impl Into<String>,
    message: impl Into<String>,
    related_ids: Vec<String>,
) -> ValidationIssue {
    ValidationIssue {
        code: code.into(),
        severity: ValidationSeverity::Error,
        path: path.into(),
        message: message.into(),
        related_ids,
    }
}
