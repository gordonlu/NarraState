use crate::character::CharacterDefinition;
use crate::disclosure::DisclosurePrerequisite;
use crate::evidence::{CaseElement, EvidenceDefinition};
use crate::fact::Fact;
use crate::id::{CaseId, EvidenceId, FactId};
use crate::validation::ValidationError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CaseDefinition {
    pub schema_version: String,
    pub id: CaseId,
    pub title: String,
    pub summary: String,
    pub locale: String,
    pub required_case_elements: BTreeSet<CaseElement>,
    pub entities: Vec<Entity>,
    pub facts: Vec<Fact>,
    pub evidence: Vec<EvidenceDefinition>,
    pub characters: Vec<CharacterDefinition>,
    pub initial_player_knowledge: PlayerKnowledge,
    pub ending: Option<Ending>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Entity {
    pub id: String,
    pub name: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PlayerKnowledge {
    pub fact_ids: Vec<FactId>,
    pub evidence_ids: Vec<EvidenceId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Ending {
    pub epilogue: String,
}

impl CaseDefinition {
    pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();

        // Check for duplicate IDs across all namespaces
        errors.extend(self.check_duplicate_ids());

        // Check all references are valid
        errors.extend(self.check_references());

        // Check disclosure graphs
        for (ci, character) in self.characters.iter().enumerate() {
            let path = format!("characters[{ci}]");
            errors.extend(self.validate_character_disclosure_graph(character, &path));
        }

        // Check at least one reachable culprit
        errors.extend(self.check_culprit_reachability());

        // Check required case elements
        errors.extend(self.check_required_elements());

        // Check initial player knowledge
        errors.extend(self.check_initial_knowledge());

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn check_duplicate_ids(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();

        let mut fact_ids = BTreeSet::new();
        for fact in &self.facts {
            if !fact_ids.insert(fact.id.clone()) {
                errors.push(ValidationError::DuplicateId {
                    field: "facts[].id".to_string(),
                    id: fact.id.to_string(),
                });
            }
        }

        let mut evidence_ids = BTreeSet::new();
        for ev in &self.evidence {
            if !evidence_ids.insert(ev.id.clone()) {
                errors.push(ValidationError::DuplicateId {
                    field: "evidence[].id".to_string(),
                    id: ev.id.to_string(),
                });
            }
        }

        let mut char_ids = BTreeSet::new();
        for ch in &self.characters {
            if !char_ids.insert(ch.id.clone()) {
                errors.push(ValidationError::DuplicateId {
                    field: "characters[].id".to_string(),
                    id: ch.id.to_string(),
                });
            }
        }

        errors
    }

    fn check_references(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let fact_ids: BTreeSet<&FactId> = self.facts.iter().map(|f| &f.id).collect();
        let evidence_ids: BTreeSet<&EvidenceId> = self.evidence.iter().map(|e| &e.id).collect();

        for (ci, character) in self.characters.iter().enumerate() {
            for (ki, fid) in character.knowledge.iter().enumerate() {
                if !fact_ids.contains(fid) {
                    errors.push(ValidationError::ReferenceNotFound {
                        field: format!("characters[{ci}].knowledge[{ki}]"),
                        reference: fid.to_string(),
                        target_type: "Fact".to_string(),
                    });
                }
            }

            for (cli, claim) in character.claims.iter().enumerate() {
                for (ei, eid) in claim.invalidated_by.iter().enumerate() {
                    if !evidence_ids.contains(eid) {
                        errors.push(ValidationError::ReferenceNotFound {
                            field: format!("characters[{ci}].claims[{cli}].invalidated_by[{ei}]"),
                            reference: eid.to_string(),
                            target_type: "Evidence".to_string(),
                        });
                    }
                }
            }
        }

        for (ei, ev) in self.evidence.iter().enumerate() {
            for (ci, cid) in ev.contradicts.iter().enumerate() {
                let found = self
                    .characters
                    .iter()
                    .any(|ch| ch.claims.iter().any(|c| &c.id == cid));
                if !found {
                    errors.push(ValidationError::ReferenceNotFound {
                        field: format!("evidence[{ei}].contradicts[{ci}]"),
                        reference: cid.to_string(),
                        target_type: "Claim".to_string(),
                    });
                }
            }
        }

        errors
    }

    fn validate_character_disclosure_graph(
        &self,
        character: &CharacterDefinition,
        path: &str,
    ) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let graph = &character.disclosure_graph;

        if let Err(cycle_errors) = graph.validate_acyclic() {
            for ce in cycle_errors {
                errors.push(ValidationError::DisclosureCycle {
                    field: format!("{path}.disclosure_graph"),
                    detail: ce.to_string(),
                });
            }
        }

        if let Err(ce) = graph.validate_confession() {
            let msg = match ce {
                crate::disclosure::ConfessionValidationError::MultipleConfessionNodes => {
                    "Multiple Confession nodes found"
                }
                crate::disclosure::ConfessionValidationError::MissingActionPrerequisite => {
                    "Confession node must have at least one FullAction/PartialAction/Intent prerequisite"
                }
            };
            errors.push(ValidationError::DisclosureCycle {
                field: format!("{path}.disclosure_graph"),
                detail: msg.to_string(),
            });
        }

        // Check that non-culprit characters do not have a Confession node
        // Culprit determination is based on whether the character has a Confession node
        // in their disclosure graph. This is a v0.1 heuristic.

        errors
    }

    fn check_culprit_reachability(&self) -> Vec<ValidationError> {
        let culprits: Vec<&CharacterDefinition> = self
            .characters
            .iter()
            .filter(|c| c.disclosure_graph.confession_node().is_some())
            .collect();

        if culprits.is_empty() {
            return vec![ValidationError::NoCulprit];
        }

        let mut errors = Vec::new();
        for culprit in culprits {
            if let Some(confession) = culprit.disclosure_graph.confession_node() {
                let has_fact_reveals = !confession.reveals.is_empty();
                let has_element_coverage = confession
                    .prerequisites
                    .iter()
                    .any(|p| matches!(p, DisclosurePrerequisite::EvidencePresented { .. }));

                if !has_fact_reveals && !has_element_coverage {
                    errors.push(ValidationError::CulpritUnreachable {
                        character: culprit.id.to_string(),
                        detail: "Confession node reveals no facts and has no evidence-based prerequisites"
                            .to_string(),
                    });
                }
            }
        }

        errors
    }

    fn check_required_elements(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let all_elements: BTreeSet<CaseElement> = self
            .evidence
            .iter()
            .flat_map(|e| e.elements.iter())
            .copied()
            .collect();

        for elem in &self.required_case_elements {
            if !all_elements.contains(elem) {
                errors.push(ValidationError::RequiredElementNotCovered {
                    element: format!("{elem:?}"),
                });
            }
        }
        errors
    }

    fn check_initial_knowledge(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let fact_ids: BTreeSet<&FactId> = self.facts.iter().map(|f| &f.id).collect();
        let evidence_ids: BTreeSet<&EvidenceId> = self.evidence.iter().map(|e| &e.id).collect();

        for (i, fid) in self.initial_player_knowledge.fact_ids.iter().enumerate() {
            if !fact_ids.contains(fid) {
                errors.push(ValidationError::ReferenceNotFound {
                    field: format!("initial_player_knowledge.fact_ids[{i}]"),
                    reference: fid.to_string(),
                    target_type: "Fact".to_string(),
                });
            }
        }

        for (i, eid) in self
            .initial_player_knowledge
            .evidence_ids
            .iter()
            .enumerate()
        {
            if !evidence_ids.contains(eid) {
                errors.push(ValidationError::ReferenceNotFound {
                    field: format!("initial_player_knowledge.evidence_ids[{i}]"),
                    reference: eid.to_string(),
                    target_type: "Evidence".to_string(),
                });
            }
        }

        errors
    }
}
