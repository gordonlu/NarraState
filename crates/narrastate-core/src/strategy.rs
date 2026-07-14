use crate::id::{ClaimId, DefenseStrategyId};
use crate::phase::InterrogationPhase;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DefenseStrategy {
    pub id: DefenseStrategyId,
    pub kind: DefenseStrategyKind,
    pub usable_phases: Vec<InterrogationPhase>,
    pub max_uses: u8,
    pub applicable_claims: Vec<ClaimId>,
    pub fallback_strategy: Option<DefenseStrategyId>,
    pub style_prompt: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum DefenseStrategyKind {
    Denial,
    MemoryGap,
    InnocentExplanation,
    EvidenceChallenge,
    MinimizeResponsibility,
    ShiftBlame,
    EmotionalAppeal,
    Silence,
}
