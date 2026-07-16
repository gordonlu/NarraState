use narrastate_core::{
    ConfessionPolicy, Difficulty, GenerationLimits, GenerationRequest, GenerationStatus,
    NarrativeTone, RealismLevel,
};

fn request() -> GenerationRequest {
    GenerationRequest {
        theme: "港口货物失踪".into(),
        setting: "现代港区".into(),
        tone: NarrativeTone::Realistic,
        target_duration_minutes: 45,
        difficulty: Difficulty::Medium,
        character_count: 4,
        variant_count: 3,
        realism: RealismLevel::Grounded,
        confession_policy: ConfessionPolicy::PartialThenFull,
        content_constraints: vec!["不得依赖超自然因素".into()],
        language: "zh-CN".into(),
    }
}

#[test]
fn generation_request_enforces_central_limits_with_paths() {
    let mut request = request();
    request.character_count = 5;
    request.variant_count = 0;
    let issues = request.validate(GenerationLimits::default());
    assert!(issues.iter().any(|issue| {
        issue.code == "GENERATION_VALUE_OUT_OF_RANGE" && issue.path == "$.character_count"
    }));
    assert!(issues.iter().any(|issue| {
        issue.code == "GENERATION_VALUE_OUT_OF_RANGE" && issue.path == "$.variant_count"
    }));
}

#[test]
fn generation_text_limit_counts_unicode_scalars() {
    let mut request = request();
    request.theme = "案".repeat(4_001);
    let issues = request.validate(GenerationLimits::default());
    assert!(issues
        .iter()
        .any(|issue| issue.code == "GENERATION_TEXT_TOO_LONG" && issue.path == "$.theme"));
}

#[test]
fn generation_setting_is_optional_but_still_length_limited() {
    let mut request = request();
    request.setting.clear();
    assert!(!request
        .validate(GenerationLimits::default())
        .iter()
        .any(|issue| issue.path == "$.setting"));

    request.setting = "地".repeat(4_001);
    assert!(request
        .validate(GenerationLimits::default())
        .iter()
        .any(|issue| { issue.code == "GENERATION_TEXT_TOO_LONG" && issue.path == "$.setting" }));
}

#[test]
fn generation_request_rejects_unplayable_duration_scope_combinations() {
    let mut request = request();
    request.target_duration_minutes = 10;
    request.difficulty = Difficulty::Hard;
    request.character_count = 5;
    request.variant_count = 5;

    let issues = request.validate(GenerationLimits::default());
    for path in ["$.difficulty", "$.character_count", "$.variant_count"] {
        assert!(issues
            .iter()
            .any(|issue| { issue.code == "GENERATION_INCOHERENT_SCOPE" && issue.path == path }));
    }
}

#[test]
fn generation_state_machine_rejects_skips_and_terminal_restarts() {
    assert!(!GenerationStatus::Pending.can_transition_to(GenerationStatus::Completed));
    assert!(!GenerationStatus::Completed.can_transition_to(GenerationStatus::Drafting));
    assert!(GenerationStatus::Pending.can_transition_to(GenerationStatus::Drafting));
    assert!(GenerationStatus::Simulating.can_transition_to(GenerationStatus::Completed));
}
