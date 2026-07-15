use narrastate_core::{
    CaseTemplate, DraftSolutionVariant, GeneratedCaseDraft, GenerationIssue, SolutionVariant,
};
use std::collections::BTreeSet;

pub fn normalize_draft(draft: &GeneratedCaseDraft) -> Result<CaseTemplate, Vec<GenerationIssue>> {
    let mut issues = draft
        .generation_request
        .validate(narrastate_core::GenerationLimits::default());
    if draft.schema_version != "0.2" {
        issues.push(issue(
            "DRAFT_SCHEMA_VERSION_UNSUPPORTED",
            "$.schema_version",
            format!("expected 0.2, got {}", draft.schema_version),
        ));
    }
    let case = &draft.case;
    required(&mut issues, "$.case.id", case.id.as_ref());
    required(&mut issues, "$.case.version", case.version.as_ref());
    required(&mut issues, "$.case.title", case.title.as_ref());
    required(&mut issues, "$.case.summary", case.summary.as_ref());
    required(&mut issues, "$.case.locale", case.locale.as_ref());
    required(
        &mut issues,
        "$.case.required_case_elements",
        case.required_case_elements.as_ref(),
    );
    required(&mut issues, "$.case.entities", case.entities.as_ref());
    required(
        &mut issues,
        "$.case.shared_facts",
        case.shared_facts.as_ref(),
    );
    required(
        &mut issues,
        "$.case.shared_evidence",
        case.shared_evidence.as_ref(),
    );
    required(
        &mut issues,
        "$.case.shared_characters",
        case.shared_characters.as_ref(),
    );
    required(
        &mut issues,
        "$.case.initial_player_knowledge",
        case.initial_player_knowledge.as_ref(),
    );
    required(
        &mut issues,
        "$.case.default_variant_id",
        case.default_variant_id.as_ref(),
    );
    if case.solution_variants.len() != draft.generation_request.variant_count as usize {
        issues.push(issue(
            "DRAFT_VARIANT_COUNT_MISMATCH",
            "$.case.solution_variants",
            format!(
                "requested {}, received {}",
                draft.generation_request.variant_count,
                case.solution_variants.len()
            ),
        ));
    }

    let mut ids = BTreeSet::new();
    for (index, variant) in case.solution_variants.iter().enumerate() {
        validate_variant(&mut issues, index, variant);
        if let Some(id) = &variant.id {
            if !ids.insert(id.clone()) {
                issues.push(issue(
                    "DRAFT_DUPLICATE_VARIANT_ID",
                    format!("$.case.solution_variants[{index}].id"),
                    format!("duplicate variant id {id}"),
                ));
            }
        }
    }
    if !issues.is_empty() {
        return Err(issues);
    }

    Ok(CaseTemplate {
        schema_version: draft.schema_version.clone(),
        id: case.id.clone().expect("checked"),
        version: case.version.clone().expect("checked"),
        title: case.title.clone().expect("checked"),
        summary: case.summary.clone().expect("checked"),
        locale: case.locale.clone().expect("checked"),
        required_case_elements: case.required_case_elements.clone().expect("checked"),
        entities: case.entities.clone().expect("checked"),
        shared_facts: case.shared_facts.clone().expect("checked"),
        shared_evidence: case.shared_evidence.clone().expect("checked"),
        shared_characters: case.shared_characters.clone().expect("checked"),
        initial_player_knowledge: case.initial_player_knowledge.clone().expect("checked"),
        solution_variants: case.solution_variants.iter().map(to_variant).collect(),
        default_variant_id: case.default_variant_id.clone().expect("checked"),
    })
}

fn validate_variant(
    issues: &mut Vec<GenerationIssue>,
    index: usize,
    variant: &DraftSolutionVariant,
) {
    let root = format!("$.case.solution_variants[{index}]");
    required(issues, &format!("{root}.id"), variant.id.as_ref());
    required(issues, &format!("{root}.title"), variant.title.as_ref());
    required(
        issues,
        &format!("{root}.description"),
        variant.description.as_ref(),
    );
    required(issues, &format!("{root}.weight"), variant.weight.as_ref());
    required(issues, &format!("{root}.enabled"), variant.enabled.as_ref());
    required(
        issues,
        &format!("{root}.responsible_character_id"),
        variant.responsible_character_id.as_ref(),
    );
    required(issues, &format!("{root}.ending"), variant.ending.as_ref());
}

fn to_variant(draft: &DraftSolutionVariant) -> SolutionVariant {
    SolutionVariant {
        id: draft.id.clone().expect("checked"),
        title: draft.title.clone().expect("checked"),
        description: draft.description.clone().expect("checked"),
        weight: draft.weight.expect("checked"),
        enabled: draft.enabled.expect("checked"),
        responsible_character_id: draft.responsible_character_id.clone().expect("checked"),
        fact_replacements: draft.fact_replacements.clone(),
        evidence_replacements: draft.evidence_replacements.clone(),
        character_replacements: draft.character_replacements.clone(),
        additions: draft.additions.clone(),
        required_case_elements: draft.required_case_elements.clone(),
        ending: draft.ending.clone().expect("checked"),
    }
}

fn required<T>(issues: &mut Vec<GenerationIssue>, path: &str, value: Option<&T>) {
    if value.is_none() {
        issues.push(issue(
            "DRAFT_REQUIRED_FIELD_MISSING",
            path,
            "required draft field is missing",
        ));
    }
}

fn issue(code: &str, path: impl Into<String>, message: impl Into<String>) -> GenerationIssue {
    GenerationIssue {
        code: code.into(),
        path: path.into(),
        message: message.into(),
    }
}
