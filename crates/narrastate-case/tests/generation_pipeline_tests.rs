use narrastate_case::{draft_from_template, load_case_package, run_generation_pipeline};
use narrastate_core::{
    ConfessionPolicy, Difficulty, DraftCaseTemplate, GeneratedCaseDraft, GenerationLimits,
    GenerationRequest, GenerationStatus, NarrativeTone, RealismLevel,
};
use narrastate_runtime::mock::MockCaseGenerationProvider;
use narrastate_runtime::ports::ProviderError;

fn request() -> GenerationRequest {
    GenerationRequest {
        theme: "画廊失窃".into(),
        setting: "现代画廊".into(),
        tone: NarrativeTone::Realistic,
        target_duration_minutes: 45,
        difficulty: Difficulty::Medium,
        character_count: 3,
        variant_count: 3,
        realism: RealismLevel::Grounded,
        confession_policy: ConfessionPolicy::PartialThenFull,
        content_constraints: vec![],
        language: "zh-CN".into(),
    }
}

fn valid_draft() -> GeneratedCaseDraft {
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cases/rain-gallery-variants");
    let package = load_case_package(root).unwrap();
    draft_from_template(request(), &package.template)
}

#[tokio::test]
async fn mock_pipeline_completes_without_any_live_model() {
    let provider = MockCaseGenerationProvider::new(vec![Ok(valid_draft())]);
    let result = run_generation_pipeline(&provider, request(), GenerationLimits::default())
        .await
        .unwrap();
    assert_eq!(result.repairs, 0);
    assert!(result.validation.valid);
    assert_eq!(
        result.events.last().unwrap().to,
        GenerationStatus::Completed
    );
}

#[tokio::test]
async fn invalid_first_draft_can_be_repaired_but_original_is_preserved() {
    let invalid = GeneratedCaseDraft {
        generation_request: request(),
        schema_version: "0.2".into(),
        case: DraftCaseTemplate::default(),
    };
    let provider = MockCaseGenerationProvider::new(vec![Ok(invalid.clone()), Ok(valid_draft())]);
    let result = run_generation_pipeline(&provider, request(), GenerationLimits::default())
        .await
        .unwrap();
    assert_eq!(result.repairs, 1);
    assert_eq!(result.drafts.len(), 2);
    assert!(result.drafts[0].case.id.is_none());
}

#[tokio::test]
async fn repair_exhaustion_is_explicit_and_never_publishes() {
    let invalid = GeneratedCaseDraft {
        generation_request: request(),
        schema_version: "0.2".into(),
        case: DraftCaseTemplate::default(),
    };
    let provider = MockCaseGenerationProvider::new(vec![
        Ok(invalid.clone()),
        Ok(invalid.clone()),
        Ok(invalid),
    ]);
    let failure = run_generation_pipeline(&provider, request(), GenerationLimits::default())
        .await
        .unwrap_err();
    assert_eq!(failure.code, "GENERATION_REPAIR_EXHAUSTED");
    assert_eq!(failure.repairs, 2);
    assert_eq!(failure.events.last().unwrap().to, GenerationStatus::Failed);
}

#[tokio::test]
async fn provider_timeout_and_empty_response_queue_are_explicit() {
    let timeout = MockCaseGenerationProvider::new(vec![Err(ProviderError::Timeout)]);
    let failure = run_generation_pipeline(&timeout, request(), GenerationLimits::default())
        .await
        .unwrap_err();
    assert_eq!(failure.code, "GENERATION_PROVIDER_TIMEOUT");

    let truncated = MockCaseGenerationProvider::new(vec![Err(ProviderError::OutputTruncated)]);
    let failure = run_generation_pipeline(&truncated, request(), GenerationLimits::default())
        .await
        .unwrap_err();
    assert_eq!(failure.code, "GENERATION_PROVIDER_OUTPUT_TRUNCATED");

    let empty = MockCaseGenerationProvider::new(vec![]);
    let failure = run_generation_pipeline(&empty, request(), GenerationLimits::default())
        .await
        .unwrap_err();
    assert_eq!(failure.code, "GENERATION_PROVIDER_INVALID_RESPONSE");
}

#[tokio::test]
async fn duplicate_variant_ids_fail_with_stable_draft_issue() {
    let mut draft = valid_draft();
    draft.case.solution_variants[1].id = draft.case.solution_variants[0].id.clone();
    let provider = MockCaseGenerationProvider::new(vec![Ok(draft)]);
    let limits = GenerationLimits {
        max_repairs: 0,
        ..GenerationLimits::default()
    };
    let failure = run_generation_pipeline(&provider, request(), limits)
        .await
        .unwrap_err();
    assert!(failure
        .issues
        .iter()
        .any(|issue| issue.code == "DRAFT_DUPLICATE_VARIANT_ID"));
}
