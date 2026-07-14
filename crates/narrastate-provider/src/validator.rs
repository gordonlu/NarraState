use narrastate_core::id::{ClaimId, FactId};
use narrastate_runtime::DialoguePlan;

use crate::renderer::RendererOutput;

pub struct OutputValidator;

#[derive(Debug)]
pub enum ValidationError {
    InvalidUtterance,
    DisallowedClaim { claim: ClaimId },
    DisallowedFact { fact: FactId },
    ForbiddenFactReferenced { fact: FactId },
    FullAdmissionNotAllowed,
    EmptyUtterance,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidUtterance => write!(f, "Utterance failed validation"),
            Self::DisallowedClaim { claim } => write!(f, "Claim {claim} is not in the allowed set"),
            Self::DisallowedFact { fact } => write!(f, "Fact {fact} is not in the allowed set"),
            Self::ForbiddenFactReferenced { fact } => write!(f, "Fact {fact} is forbidden"),
            Self::FullAdmissionNotAllowed => write!(f, "Full admission not allowed by plan"),
            Self::EmptyUtterance => write!(f, "Utterance is empty"),
        }
    }
}

impl OutputValidator {
    pub fn new() -> Self {
        Self
    }

    pub fn validate(
        &self,
        output: &RendererOutput,
        plan: &DialoguePlan,
    ) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();

        if output.utterance.trim().is_empty() {
            errors.push(ValidationError::EmptyUtterance);
        }

        if output.utterance.len() > 500 {
            errors.push(ValidationError::InvalidUtterance);
        }

        for cid in &output.expressed_claim_ids {
            if !plan.allowed_claims.contains(cid) {
                errors.push(ValidationError::DisallowedClaim { claim: cid.clone() });
            }
        }

        for fid in &output.acknowledged_fact_ids {
            if plan.forbidden_facts.contains(fid) {
                errors.push(ValidationError::ForbiddenFactReferenced { fact: fid.clone() });
            } else if !plan.allowed_facts.contains(fid) {
                errors.push(ValidationError::DisallowedFact { fact: fid.clone() });
            }
        }

        let is_full_admission = output.utterance.contains("认罪")
            || output.utterance.contains("是我干的")
            || output.utterance.contains("I confess")
            || output.utterance.contains("I did it");

        if is_full_admission && !matches!(plan.act, narrastate_core::DialogueAct::FullAdmission) {
            errors.push(ValidationError::FullAdmissionNotAllowed);
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    pub fn template_fallback(&self, plan: &DialoguePlan) -> RendererOutput {
        use narrastate_core::DialogueAct::*;
        let utterance = match plan.act {
            Answer => "I am answering your question.".into(),
            Deny => "That's not true. I deny it.".into(),
            Evade => "I don't recall that clearly.".into(),
            Reframe => "Let me explain the situation differently.".into(),
            ChallengeEvidence => "That evidence doesn't prove anything.".into(),
            ShiftBlame => "Someone else must have done it.".into(),
            PartialAdmission => {
                if plan.newly_revealed.is_some() {
                    "Alright, I admit that much.".into()
                } else {
                    "I admit to some involvement, but not everything.".into()
                }
            }
            FullAdmission => "I confess. I did it.".into(),
            AskForClarification => "What exactly are you asking?".into(),
            Silence => "...".into(),
        };

        RendererOutput {
            utterance,
            expressed_claim_ids: plan.allowed_claims.clone(),
            acknowledged_fact_ids: plan.allowed_facts.clone(),
            tone: "neutral".into(),
        }
    }
}

impl Default for OutputValidator {
    fn default() -> Self {
        Self::new()
    }
}
