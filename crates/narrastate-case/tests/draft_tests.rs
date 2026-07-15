use narrastate_case::normalize_draft;
use narrastate_core::{
    ConfessionPolicy, Difficulty, DraftCaseTemplate, GeneratedCaseDraft, GenerationRequest,
    NarrativeTone, RealismLevel,
};

fn request() -> GenerationRequest {
    GenerationRequest {
        theme: "失踪".into(),
        setting: "港区".into(),
        tone: NarrativeTone::Realistic,
        target_duration_minutes: 45,
        difficulty: Difficulty::Medium,
        character_count: 4,
        variant_count: 3,
        realism: RealismLevel::Grounded,
        confession_policy: ConfessionPolicy::PartialThenFull,
        content_constraints: vec![],
        language: "zh-CN".into(),
    }
}

#[test]
fn incomplete_draft_is_not_silently_promoted_to_formal_template() {
    let draft = GeneratedCaseDraft {
        generation_request: request(),
        schema_version: "0.2".into(),
        case: DraftCaseTemplate::default(),
    };
    let issues = normalize_draft(&draft).expect_err("incomplete draft must fail");
    assert!(issues.iter().any(|issue| {
        issue.code == "DRAFT_REQUIRED_FIELD_MISSING" && issue.path == "$.case.id"
    }));
    assert!(issues.iter().any(|issue| {
        issue.code == "DRAFT_VARIANT_COUNT_MISMATCH" && issue.path == "$.case.solution_variants"
    }));
}
