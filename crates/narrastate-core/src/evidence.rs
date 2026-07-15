use crate::id::{ClaimId, EvidenceId, PropositionRef};
use crate::phase::InterrogationPhase;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EvidenceUse {
    pub evidence_id: EvidenceId,
    pub usage: EvidenceUsageKind,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum EvidenceUsageKind {
    DirectReference,
    ImplicitReference,
    SupportingContext,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EvidenceDefinition {
    pub id: EvidenceId,
    pub title: String,
    pub description: String,
    pub supports: Vec<PropositionRef>,
    pub contradicts: Vec<ClaimId>,
    pub elements: BTreeSet<CaseElement>,
    pub reliability: f32,
    pub directness: f32,
    pub exclusivity: f32,
    pub discoverable_by: Vec<DiscoveryRule>,
}

#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub enum CaseElement {
    Identity,
    Access,
    Opportunity,
    Means,
    Action,
    Motive,
    Intent,
    Causation,
    Concealment,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum DiscoveryRule {
    StartingEvidence,
    AutomaticAtPhase(InterrogationPhase),
    AfterEvidencePresented(EvidenceId),
}
