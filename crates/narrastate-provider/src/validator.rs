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
    ModelIdentityLeak,
    ConfessionalToneNotAllowed,
    MissingRequiredFact { fact: FactId },
    InvalidTone,
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
            Self::ModelIdentityLeak => write!(f, "Utterance leaked model or system identity"),
            Self::ConfessionalToneNotAllowed => {
                write!(f, "Confessional tone is not allowed by the dialogue plan")
            }
            Self::MissingRequiredFact { fact } => {
                write!(f, "Newly revealed fact {fact} was not acknowledged")
            }
            Self::InvalidTone => write!(f, "Renderer returned an unsupported tone"),
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

        if contains_model_identity(&output.utterance) {
            errors.push(ValidationError::ModelIdentityLeak);
        }

        if !VALID_TONES.contains(&output.tone.as_str()) {
            errors.push(ValidationError::InvalidTone);
        }
        if output.tone == "confessional"
            && !matches!(plan.act, narrastate_core::DialogueAct::FullAdmission)
        {
            errors.push(ValidationError::ConfessionalToneNotAllowed);
        }

        let is_full_admission = contains_unnegated_admission(&output.utterance);

        if is_full_admission && !matches!(plan.act, narrastate_core::DialogueAct::FullAdmission) {
            errors.push(ValidationError::FullAdmissionNotAllowed);
        }

        if matches!(
            plan.act,
            narrastate_core::DialogueAct::PartialAdmission
                | narrastate_core::DialogueAct::FullAdmission
        ) {
            for fact in &plan.newly_revealed_facts {
                if !output.acknowledged_fact_ids.contains(fact) {
                    errors.push(ValidationError::MissingRequiredFact { fact: fact.clone() });
                }
            }
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
            Answer => "我会回答你的问题。".into(),
            Deny => "这不是真的，我否认这一点。".into(),
            Evade => "这件事我记得不太清楚。".into(),
            Reframe => "事情并不是你说的那样。".into(),
            ChallengeEvidence => "这份证据还不能证明你的结论。".into(),
            ShiftBlame => "你应该再查查其他人。".into(),
            PartialAdmission => {
                if plan.newly_revealed.is_some() {
                    "好吧，这一点我承认。".into()
                } else {
                    "我承认有所牵涉，但事情不是你想的那样。".into()
                }
            }
            FullAdmission => "我认罪，是我做的。".into(),
            AskForClarification => "你具体想问什么？".into(),
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

const VALID_TONES: &[&str] = &[
    "neutral",
    "defensive",
    "agitated",
    "controlled_defensive",
    "evasive",
    "resigned",
    "confessional",
    "angry",
    "calm",
];

fn contains_model_identity(text: &str) -> bool {
    let normalized = text.to_lowercase().replace(char::is_whitespace, "");
    [
        "作为ai",
        "我是ai",
        "语言模型",
        "系统提示",
        "systemprompt",
        "chatgpt",
        "openai",
        "asanai",
        "iamanai",
    ]
    .iter()
    .any(|phrase| normalized.contains(phrase))
}

fn contains_unnegated_admission(text: &str) -> bool {
    let normalized = text.to_lowercase();
    [
        "我认罪",
        "是我干的",
        "是我做的",
        "是我偷的",
        "我偷了",
        "我拿走了",
        "我拿走的",
        "我安排的",
        "我策划的",
        "i confess",
        "i did it",
        "i stole",
    ]
    .iter()
    .any(|phrase| contains_unnegated(&normalized, phrase))
}

fn contains_unnegated(text: &str, phrase: &str) -> bool {
    text.match_indices(phrase).any(|(index, _)| {
        let prefix = &text[..index];
        !["不", "没", "并非", "不是", "never", "didn't", "did not"]
            .iter()
            .any(|negation| prefix.trim_end().ends_with(negation))
    })
}

impl Default for OutputValidator {
    fn default() -> Self {
        Self::new()
    }
}
