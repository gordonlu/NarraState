use crate::{normalize_draft, validate_template, CaseValidationReport};
use narrastate_core::{
    CaseTemplate, DraftCaseTemplate, DraftSolutionVariant, GeneratedCaseDraft, GenerationIssue,
    GenerationJobId, GenerationLimits, GenerationRepairRequest, GenerationRequest,
    GenerationStatus, GenerationStatusEvent,
};
use narrastate_runtime::ports::{CaseGenerationProvider, ProviderError, TokenUsage};

#[derive(Debug, Clone)]
pub struct GenerationPipelineSuccess {
    pub job_id: GenerationJobId,
    pub template: CaseTemplate,
    pub drafts: Vec<GeneratedCaseDraft>,
    pub validation: CaseValidationReport,
    pub events: Vec<GenerationStatusEvent>,
    pub repairs: u32,
    pub usage: TokenUsage,
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("{code}: {message}")]
pub struct GenerationPipelineFailure {
    pub job_id: GenerationJobId,
    pub code: String,
    pub message: String,
    pub issues: Vec<GenerationIssue>,
    pub drafts: Vec<GeneratedCaseDraft>,
    pub events: Vec<GenerationStatusEvent>,
    pub repairs: u32,
    pub usage: TokenUsage,
}

pub async fn run_generation_pipeline(
    provider: &dyn CaseGenerationProvider,
    request: GenerationRequest,
    limits: GenerationLimits,
) -> Result<GenerationPipelineSuccess, GenerationPipelineFailure> {
    run_generation_pipeline_with_id(provider, GenerationJobId::new(), request, limits).await
}

pub async fn run_generation_pipeline_with_id(
    provider: &dyn CaseGenerationProvider,
    job_id: GenerationJobId,
    request: GenerationRequest,
    limits: GenerationLimits,
) -> Result<GenerationPipelineSuccess, GenerationPipelineFailure> {
    let mut run = PipelineRun::new(job_id);
    run.transition(GenerationStatus::Drafting, None);
    let request_issues = request.validate(limits);
    if !request_issues.is_empty() {
        return Err(run.fail(
            "GENERATION_REQUEST_INVALID",
            "generation request failed validation",
            request_issues,
        ));
    }

    let response = match provider.generate_draft(&request).await {
        Ok(response) => response,
        Err(error) => return Err(run.provider_failure(error)),
    };
    run.usage = run.usage.combine(response.usage);
    run.drafts.push(response.output);
    run.transition(GenerationStatus::Parsing, None);

    loop {
        let draft = run
            .drafts
            .last()
            .expect("provider returned a draft")
            .clone();
        let mut draft_issues = inspect_draft(&draft, &request, limits);
        run.transition(GenerationStatus::Normalizing, None);
        let normalized = match normalize_draft(&draft) {
            Ok(template) if draft_issues.is_empty() => Some(template),
            Ok(_) => None,
            Err(mut issues) => {
                draft_issues.append(&mut issues);
                None
            }
        };
        if let Some(template) = normalized {
            run.transition(GenerationStatus::Compiling, None);
            let validation = validate_template(&template);
            run.transition(GenerationStatus::Validating, None);
            run.transition(GenerationStatus::Simulating, None);
            if validation.valid {
                run.transition(GenerationStatus::Completed, None);
                return Ok(GenerationPipelineSuccess {
                    job_id,
                    template,
                    drafts: run.drafts,
                    validation,
                    events: run.events,
                    repairs: run.repairs,
                    usage: run.usage,
                });
            }
            draft_issues.extend(validation.errors.iter().map(|issue| GenerationIssue {
                code: issue.code.clone(),
                path: issue.path.clone(),
                message: issue.message.clone(),
            }));
        }

        if run.repairs >= limits.max_repairs {
            return Err(run.fail(
                "GENERATION_REPAIR_EXHAUSTED",
                "draft remained invalid after the configured repair limit",
                draft_issues,
            ));
        }
        run.transition(GenerationStatus::Repairing, None);
        let repair_issues = draft_issues.clone();
        let repair = GenerationRepairRequest {
            draft,
            issues: draft_issues,
        };
        let response = match provider.repair_draft(&repair).await {
            Ok(response) => response,
            Err(error) => return Err(run.provider_failure_with_issues(error, repair_issues)),
        };
        run.repairs = run.repairs.saturating_add(1);
        run.usage = run.usage.combine(response.usage);
        run.drafts.push(response.output);
        run.transition(GenerationStatus::Parsing, None);
    }
}

pub fn draft_from_template(
    generation_request: GenerationRequest,
    template: &CaseTemplate,
) -> GeneratedCaseDraft {
    GeneratedCaseDraft {
        generation_request,
        schema_version: template.schema_version.clone(),
        case: DraftCaseTemplate {
            id: Some(template.id.clone()),
            version: Some(template.version.clone()),
            title: Some(template.title.clone()),
            summary: Some(template.summary.clone()),
            locale: Some(template.locale.clone()),
            required_case_elements: Some(template.required_case_elements.clone()),
            entities: Some(template.entities.clone()),
            shared_facts: Some(template.shared_facts.clone()),
            shared_evidence: Some(template.shared_evidence.clone()),
            shared_characters: Some(template.shared_characters.clone()),
            initial_player_knowledge: Some(template.initial_player_knowledge.clone()),
            solution_variants: template
                .solution_variants
                .iter()
                .map(|variant| DraftSolutionVariant {
                    id: Some(variant.id.clone()),
                    title: Some(variant.title.clone()),
                    description: Some(variant.description.clone()),
                    weight: Some(variant.weight),
                    enabled: Some(variant.enabled),
                    responsible_character_id: Some(variant.responsible_character_id.clone()),
                    fact_replacements: variant.fact_replacements.clone(),
                    evidence_replacements: variant.evidence_replacements.clone(),
                    character_replacements: variant.character_replacements.clone(),
                    additions: variant.additions.clone(),
                    required_case_elements: variant.required_case_elements.clone(),
                    ending: Some(variant.ending.clone()),
                })
                .collect(),
            default_variant_id: Some(template.default_variant_id.clone()),
        },
    }
}

fn inspect_draft(
    draft: &GeneratedCaseDraft,
    original_request: &GenerationRequest,
    limits: GenerationLimits,
) -> Vec<GenerationIssue> {
    let mut issues = Vec::new();
    if &draft.generation_request != original_request {
        issues.push(GenerationIssue {
            code: "DRAFT_CHANGED_GENERATION_REQUEST".into(),
            path: "$.generation_request".into(),
            message: "provider may not alter the original generation request".into(),
        });
    }
    match serde_json::to_vec(draft) {
        Ok(bytes) if bytes.len() > limits.max_draft_bytes => issues.push(GenerationIssue {
            code: "DRAFT_SIZE_LIMIT_EXCEEDED".into(),
            path: "$".into(),
            message: format!(
                "draft is {} bytes; maximum is {}",
                bytes.len(),
                limits.max_draft_bytes
            ),
        }),
        Err(error) => issues.push(GenerationIssue {
            code: "DRAFT_SERIALIZATION_FAILED".into(),
            path: "$".into(),
            message: error.to_string(),
        }),
        Ok(_) => {}
    }
    issues
}

struct PipelineRun {
    job_id: GenerationJobId,
    status: GenerationStatus,
    events: Vec<GenerationStatusEvent>,
    drafts: Vec<GeneratedCaseDraft>,
    repairs: u32,
    usage: TokenUsage,
}

impl PipelineRun {
    fn new(job_id: GenerationJobId) -> Self {
        Self {
            job_id,
            status: GenerationStatus::Pending,
            events: vec![],
            drafts: vec![],
            repairs: 0,
            usage: TokenUsage::default(),
        }
    }

    fn transition(&mut self, to: GenerationStatus, error_code: Option<String>) {
        assert!(
            self.status.can_transition_to(to),
            "invalid generation transition {:?} -> {:?}",
            self.status,
            to
        );
        let sequence = self.events.len() as u32;
        self.events.push(GenerationStatusEvent {
            job_id: self.job_id,
            sequence,
            from: self.status,
            to,
            error_code,
        });
        self.status = to;
    }

    fn fail(
        mut self,
        code: &str,
        message: &str,
        issues: Vec<GenerationIssue>,
    ) -> GenerationPipelineFailure {
        self.transition(GenerationStatus::Failed, Some(code.into()));
        GenerationPipelineFailure {
            job_id: self.job_id,
            code: code.into(),
            message: message.into(),
            issues,
            drafts: self.drafts,
            events: self.events,
            repairs: self.repairs,
            usage: self.usage,
        }
    }

    fn provider_failure(self, error: ProviderError) -> GenerationPipelineFailure {
        self.provider_failure_with_issues(error, vec![])
    }

    fn provider_failure_with_issues(
        self,
        error: ProviderError,
        issues: Vec<GenerationIssue>,
    ) -> GenerationPipelineFailure {
        let code = match error {
            ProviderError::Timeout => "GENERATION_PROVIDER_TIMEOUT",
            ProviderError::OutputTruncated => "GENERATION_PROVIDER_OUTPUT_TRUNCATED",
            ProviderError::InvalidResponse(_) => "GENERATION_PROVIDER_INVALID_RESPONSE",
            ProviderError::Unauthorized => "GENERATION_PROVIDER_UNAUTHORIZED",
            _ => "GENERATION_PROVIDER_FAILED",
        };
        let message = error.to_string();
        self.fail(code, &message, issues)
    }
}
