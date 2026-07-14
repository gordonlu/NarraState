use crate::fact::Proposition;
use crate::id::{CharacterId, ClaimId, EvidenceId, TurnId};
use crate::phase::InterrogationPhase;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ClaimDefinition {
    pub id: ClaimId,
    pub owner: CharacterId,
    pub proposition: Proposition,
    pub kind: ClaimKind,
    pub available_from: InterrogationPhase,
    pub invalidated_by: Vec<EvidenceId>,
    pub fallback_claim: Option<ClaimId>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum ClaimKind {
    Truth,
    Lie,
    HalfTruth,
    Opinion,
    Deflection,
}

/// A claim as actually spoken during a session.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SpokenClaim {
    pub claim_id: ClaimId,
    pub turn_id: TurnId,
    pub utterance: String,
    pub invalidated: bool,
}
