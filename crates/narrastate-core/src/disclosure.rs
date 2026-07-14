use crate::id::{DisclosureId, FactId};
use crate::phase::InterrogationPhase;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DisclosureGraph {
    pub nodes: Vec<DisclosureNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DisclosureNode {
    pub id: DisclosureId,
    pub kind: DisclosureKind,
    pub reveals: Vec<FactId>,
    pub prerequisites: Vec<DisclosurePrerequisite>,
    pub min_phase: InterrogationPhase,
    pub response_intent: DialogueAct,
}

#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub enum DisclosureKind {
    PeripheralSecret,
    Presence,
    Access,
    Means,
    PartialAction,
    FullAction,
    Intent,
    Confession,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum DisclosurePrerequisite {
    Disclosure { disclosure: DisclosureId },
    EvidencePresented { facts: Vec<FactId> },
    PhaseAtLeast { min_phase: InterrogationPhase },
    ClaimInvalidated { disclosure: DisclosureId },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum DialogueAct {
    Answer,
    Deny,
    Evade,
    Reframe,
    ChallengeEvidence,
    ShiftBlame,
    PartialAdmission,
    FullAdmission,
    AskForClarification,
    Silence,
}

impl DisclosureGraph {
    pub fn validate_acyclic(&self) -> Result<(), Vec<CycleError>> {
        let mut errors = Vec::new();
        let node_ids: BTreeSet<&DisclosureId> = self.nodes.iter().map(|n| &n.id).collect();
        for node in &self.nodes {
            for prereq in &node.prerequisites {
                if let DisclosurePrerequisite::Disclosure { disclosure: dep_id } = prereq {
                    if !node_ids.contains(dep_id) {
                        errors.push(CycleError::MissingPrerequisiteNode {
                            node: node.id.clone(),
                            missing: dep_id.clone(),
                        });
                    }
                }
                if let DisclosurePrerequisite::ClaimInvalidated { disclosure: dep_id } = prereq {
                    if !node_ids.contains(dep_id) {
                        errors.push(CycleError::MissingPrerequisiteNode {
                            node: node.id.clone(),
                            missing: dep_id.clone(),
                        });
                    }
                }
            }
        }

        if !errors.is_empty() {
            return Err(errors);
        }

        let visited = &mut BTreeSet::new();
        let stack = &mut BTreeSet::new();

        for node in &self.nodes {
            if !visited.contains(&node.id) {
                if let Err(cycle) = self.dfs_check(&node.id, visited, stack) {
                    errors.push(CycleError::CycleDetected(cycle));
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn dfs_check(
        &self,
        current: &DisclosureId,
        visited: &mut BTreeSet<DisclosureId>,
        stack: &mut BTreeSet<DisclosureId>,
    ) -> Result<(), Vec<DisclosureId>> {
        if stack.contains(current) {
            return Err(vec![current.clone()]);
        }
        if visited.contains(current) {
            return Ok(());
        }
        visited.insert(current.clone());
        stack.insert(current.clone());

        if let Some(node) = self.nodes.iter().find(|n| n.id == *current) {
            for prereq in &node.prerequisites {
                match prereq {
                    DisclosurePrerequisite::Disclosure { disclosure: dep_id }
                    | DisclosurePrerequisite::ClaimInvalidated { disclosure: dep_id } => {
                        self.dfs_check(dep_id, visited, stack)?;
                    }
                    _ => {}
                }
            }
        }

        stack.remove(current);
        Ok(())
    }

    pub fn confession_node(&self) -> Option<&DisclosureNode> {
        self.nodes
            .iter()
            .find(|n| n.kind == DisclosureKind::Confession)
    }

    pub fn validate_confession(&self) -> Result<(), ConfessionValidationError> {
        let confessions: Vec<&DisclosureNode> = self
            .nodes
            .iter()
            .filter(|n| n.kind == DisclosureKind::Confession)
            .collect();

        if confessions.len() > 1 {
            return Err(ConfessionValidationError::MultipleConfessionNodes);
        }

        if let Some(confession) = confessions.first() {
            let has_action_prerequisite = confession.prerequisites.iter().any(|p| {
                if let DisclosurePrerequisite::Disclosure { disclosure: dep_id } = p {
                    self.nodes.iter().any(|n| {
                        &n.id == dep_id
                            && matches!(
                                n.kind,
                                DisclosureKind::FullAction
                                    | DisclosureKind::PartialAction
                                    | DisclosureKind::Intent
                            )
                    })
                } else {
                    false
                }
            });

            if !has_action_prerequisite {
                return Err(ConfessionValidationError::MissingActionPrerequisite);
            }
        }

        Ok(())
    }

    pub fn is_unlockable(
        &self,
        node_id: &DisclosureId,
        revealed: &BTreeSet<DisclosureId>,
        phase: InterrogationPhase,
    ) -> bool {
        let Some(node) = self.nodes.iter().find(|n| n.id == *node_id) else {
            return false;
        };

        if revealed.contains(node_id) {
            return false;
        }

        if phase < node.min_phase {
            return false;
        }

        for prereq in &node.prerequisites {
            let met = match prereq {
                DisclosurePrerequisite::Disclosure { disclosure: dep_id } => {
                    revealed.contains(dep_id)
                }
                DisclosurePrerequisite::EvidencePresented { .. } => false, // checked at runtime
                DisclosurePrerequisite::PhaseAtLeast { min_phase } => phase >= *min_phase,
                DisclosurePrerequisite::ClaimInvalidated { .. } => false, // checked at runtime
            };
            if !met {
                return false;
            }
        }

        true
    }

    pub fn unlocked_count(
        &self,
        node_id: &DisclosureId,
        revealed: &BTreeSet<DisclosureId>,
    ) -> usize {
        let Some(node) = self.nodes.iter().find(|n| n.id == *node_id) else {
            return 0;
        };
        node.prerequisites
            .iter()
            .filter(|p| {
                if let DisclosurePrerequisite::Disclosure { disclosure: dep_id } = p {
                    revealed.contains(dep_id)
                } else {
                    false
                }
            })
            .count()
    }

    pub fn major_disclosure_kinds(&self) -> BTreeSet<DisclosureKind> {
        use DisclosureKind::*;
        [
            Presence,
            Access,
            Means,
            PartialAction,
            FullAction,
            Intent,
            Confession,
        ]
        .into_iter()
        .collect()
    }
}

#[derive(Debug, Clone)]
pub enum CycleError {
    CycleDetected(Vec<DisclosureId>),
    MissingPrerequisiteNode {
        node: DisclosureId,
        missing: DisclosureId,
    },
}

impl std::fmt::Display for CycleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CycleError::CycleDetected(ids) => {
                write!(
                    f,
                    "Cycle detected in disclosure graph: {}",
                    ids.iter()
                        .map(|id| id.to_string())
                        .collect::<Vec<_>>()
                        .join(" -> ")
                )
            }
            CycleError::MissingPrerequisiteNode { node, missing } => {
                write!(
                    f,
                    "Node {node} references missing prerequisite node {missing}"
                )
            }
        }
    }
}

impl std::error::Error for CycleError {}

#[derive(Debug, Clone)]
pub enum ConfessionValidationError {
    MultipleConfessionNodes,
    MissingActionPrerequisite,
}

impl std::fmt::Display for ConfessionValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfessionValidationError::MultipleConfessionNodes => {
                write!(f, "DisclosureGraph has more than one Confession node")
            }
            ConfessionValidationError::MissingActionPrerequisite => {
                write!(
                    f,
                    "Confession node must have at least one FullAction/PartialAction/Intent prerequisite disclosure"
                )
            }
        }
    }
}

impl std::error::Error for ConfessionValidationError {}
