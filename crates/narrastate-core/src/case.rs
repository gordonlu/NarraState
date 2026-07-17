use crate::character::CharacterDefinition;
use crate::disclosure::DisclosurePrerequisite;
use crate::evidence::{CaseElement, DiscoveryRule, EvidenceDefinition};
use crate::fact::{Fact, FactVisibility};
use crate::id::{
    CaseId, CharacterId, ClaimId, DefenseStrategyId, DisclosureId, EvidenceId, FactId,
};
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
        self.validate_internal(true)
    }

    /// Variant compilation already has an explicit responsible character, so a confession is
    /// optional. If a confession path exists it is still validated for deterministic reachability.
    pub fn validate_for_variant(
        &self,
        _responsible_character_id: &CharacterId,
    ) -> Result<(), Vec<ValidationError>> {
        self.validate_internal(false)
    }

    fn validate_internal(&self, require_confession: bool) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();
        errors.extend(self.check_duplicate_ids());
        errors.extend(self.check_ranges());
        errors.extend(self.check_references());
        errors.extend(self.check_disclosure_graphs());
        errors.extend(self.check_required_elements());
        errors.extend(self.check_initial_knowledge());
        errors.extend(self.check_culprit_reachability(require_confession));
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn check_duplicate_ids(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        check_unique(
            self.entities.iter().map(|v| v.id.as_str()),
            "entities[].id",
            &mut errors,
        );
        check_unique(
            self.facts.iter().map(|v| v.id.as_ref()),
            "facts[].id",
            &mut errors,
        );
        check_unique(
            self.evidence.iter().map(|v| v.id.as_ref()),
            "evidence[].id",
            &mut errors,
        );
        check_unique(
            self.characters.iter().map(|v| v.id.as_ref()),
            "characters[].id",
            &mut errors,
        );
        let claims = self
            .characters
            .iter()
            .flat_map(|c| c.claims.iter().map(|v| v.id.as_ref()));
        check_unique(claims, "characters[].claims[].id", &mut errors);
        errors
    }

    fn check_ranges(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        for (i, evidence) in self.evidence.iter().enumerate() {
            for (name, value) in [
                ("reliability", evidence.reliability),
                ("directness", evidence.directness),
                ("exclusivity", evidence.exclusivity),
            ] {
                if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                    errors.push(ValidationError::Semantic {
                        field: format!("evidence[{i}].{name}"),
                        detail: "must be a finite number in 0.0..=1.0".into(),
                    });
                }
            }
        }
        for (ci, character) in self.characters.iter().enumerate() {
            if character.resilience > 100 {
                errors.push(ValidationError::Semantic {
                    field: format!("characters[{ci}].resilience"),
                    detail: "must be in 0..=100".into(),
                });
            }
            for (bi, belief) in character.initial_beliefs.iter().enumerate() {
                if belief.confidence > 100 {
                    errors.push(ValidationError::Semantic {
                        field: format!("characters[{ci}].initial_beliefs[{bi}].confidence"),
                        detail: "must be in 0..=100".into(),
                    });
                }
            }
        }
        errors
    }

    fn check_references(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let facts: BTreeSet<&FactId> = self.facts.iter().map(|v| &v.id).collect();
        let evidence: BTreeSet<&EvidenceId> = self.evidence.iter().map(|v| &v.id).collect();
        let claims: BTreeSet<&ClaimId> = self
            .characters
            .iter()
            .flat_map(|c| c.claims.iter().map(|v| &v.id))
            .collect();

        for (ei, item) in self.evidence.iter().enumerate() {
            for (ci, claim) in item.contradicts.iter().enumerate() {
                reference(
                    &claims,
                    claim,
                    format!("evidence[{ei}].contradicts[{ci}]"),
                    "Claim",
                    &mut errors,
                );
            }
            for (di, rule) in item.discoverable_by.iter().enumerate() {
                if let DiscoveryRule::AfterEvidencePresented {
                    evidence_id: required,
                } = rule
                {
                    reference(
                        &evidence,
                        required,
                        format!("evidence[{ei}].discoverable_by[{di}]"),
                        "Evidence",
                        &mut errors,
                    );
                }
            }
        }

        for (ci, character) in self.characters.iter().enumerate() {
            let own_claims: BTreeSet<&ClaimId> = character.claims.iter().map(|v| &v.id).collect();
            let defenses: BTreeSet<&DefenseStrategyId> =
                character.defenses.iter().map(|v| &v.id).collect();
            let disclosures: BTreeSet<&DisclosureId> = character
                .disclosure_graph
                .nodes
                .iter()
                .map(|v| &v.id)
                .collect();
            if defenses.len() != character.defenses.len() {
                errors.push(ValidationError::Semantic {
                    field: format!("characters[{ci}].defenses[].id"),
                    detail: "duplicate defense strategy ID".into(),
                });
            }
            if disclosures.len() != character.disclosure_graph.nodes.len() {
                errors.push(ValidationError::Semantic {
                    field: format!("characters[{ci}].disclosure_graph.nodes[].id"),
                    detail: "duplicate disclosure ID".into(),
                });
            }
            for (ki, fact) in character.knowledge.iter().enumerate() {
                reference(
                    &facts,
                    fact,
                    format!("characters[{ci}].knowledge[{ki}]"),
                    "Fact",
                    &mut errors,
                );
            }
            for (cli, claim) in character.claims.iter().enumerate() {
                if claim.owner != character.id {
                    errors.push(ValidationError::Semantic {
                        field: format!("characters[{ci}].claims[{cli}].owner"),
                        detail: format!("must equal character ID {}", character.id),
                    });
                }
                for (ei, id) in claim.invalidated_by.iter().enumerate() {
                    reference(
                        &evidence,
                        id,
                        format!("characters[{ci}].claims[{cli}].invalidated_by[{ei}]"),
                        "Evidence",
                        &mut errors,
                    );
                }
                if let Some(id) = &claim.fallback_claim {
                    reference(
                        &own_claims,
                        id,
                        format!("characters[{ci}].claims[{cli}].fallback_claim"),
                        "Claim",
                        &mut errors,
                    );
                }
            }
            for (di, defense) in character.defenses.iter().enumerate() {
                if defense.max_uses == 0 {
                    errors.push(ValidationError::Semantic {
                        field: format!("characters[{ci}].defenses[{di}].max_uses"),
                        detail: "must be greater than zero".into(),
                    });
                }
                for (ai, id) in defense.applicable_claims.iter().enumerate() {
                    reference(
                        &own_claims,
                        id,
                        format!("characters[{ci}].defenses[{di}].applicable_claims[{ai}]"),
                        "Claim",
                        &mut errors,
                    );
                }
                if let Some(id) = &defense.fallback_strategy {
                    reference(
                        &defenses,
                        id,
                        format!("characters[{ci}].defenses[{di}].fallback_strategy"),
                        "DefenseStrategy",
                        &mut errors,
                    );
                }
            }
            for (ni, node) in character.disclosure_graph.nodes.iter().enumerate() {
                for (fi, id) in node.reveals.iter().enumerate() {
                    reference(
                        &facts,
                        id,
                        format!("characters[{ci}].disclosure_graph.nodes[{ni}].reveals[{fi}]"),
                        "Fact",
                        &mut errors,
                    );
                    if !character.knowledge.contains(id) {
                        errors.push(ValidationError::Semantic {
                            field: format!(
                                "characters[{ci}].disclosure_graph.nodes[{ni}].reveals[{fi}]"
                            ),
                            detail: "character cannot disclose a fact outside its knowledge".into(),
                        });
                    }
                }
                for (pi, prereq) in node.prerequisites.iter().enumerate() {
                    match prereq {
                        DisclosurePrerequisite::Disclosure { disclosure } => reference(
                            &disclosures,
                            disclosure,
                            format!(
                                "characters[{ci}].disclosure_graph.nodes[{ni}].prerequisites[{pi}]"
                            ),
                            "Disclosure",
                            &mut errors,
                        ),
                        DisclosurePrerequisite::EvidencePresented { evidence: ids } => {
                            for (i, id) in ids.iter().enumerate() {
                                reference(&evidence, id, format!("characters[{ci}].disclosure_graph.nodes[{ni}].prerequisites[{pi}].evidence[{i}]"), "Evidence", &mut errors);
                            }
                        }
                        DisclosurePrerequisite::ClaimInvalidated { claim } => reference(
                            &own_claims,
                            claim,
                            format!(
                                "characters[{ci}].disclosure_graph.nodes[{ni}].prerequisites[{pi}]"
                            ),
                            "Claim",
                            &mut errors,
                        ),
                        DisclosurePrerequisite::PhaseAtLeast { .. } => {}
                    }
                }
            }
        }
        errors
    }

    fn check_disclosure_graphs(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        for (ci, character) in self.characters.iter().enumerate() {
            let field = format!("characters[{ci}].disclosure_graph");
            if let Err(items) = character.disclosure_graph.validate_acyclic() {
                errors.extend(
                    items
                        .into_iter()
                        .map(|item| ValidationError::DisclosureCycle {
                            field: field.clone(),
                            detail: item.to_string(),
                        }),
                );
            }
            if let Err(item) = character.disclosure_graph.validate_confession() {
                errors.push(ValidationError::DisclosureCycle {
                    field,
                    detail: item.to_string(),
                });
            }
        }
        errors
    }

    fn check_required_elements(&self) -> Vec<ValidationError> {
        self.required_case_elements
            .iter()
            .filter(|element| {
                !self
                    .evidence
                    .iter()
                    .any(|item| item.elements.contains(element) && !item.discoverable_by.is_empty())
            })
            .map(|element| ValidationError::RequiredElementNotCovered {
                element: format!("{element:?}"),
            })
            .collect()
    }

    fn check_initial_knowledge(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        for (i, id) in self.initial_player_knowledge.fact_ids.iter().enumerate() {
            match self.facts.iter().find(|v| &v.id == id) {
                None => errors.push(not_found(
                    format!("initial_player_knowledge.fact_ids[{i}]"),
                    id,
                    "Fact",
                )),
                Some(fact) if fact.visibility == FactVisibility::Hidden => {
                    errors.push(ValidationError::Semantic {
                        field: format!("initial_player_knowledge.fact_ids[{i}]"),
                        detail: "hidden fact cannot be initial player knowledge".into(),
                    })
                }
                Some(_) => {}
            }
        }
        for (i, id) in self
            .initial_player_knowledge
            .evidence_ids
            .iter()
            .enumerate()
        {
            match self.evidence.iter().find(|v| &v.id == id) {
                None => errors.push(not_found(
                    format!("initial_player_knowledge.evidence_ids[{i}]"),
                    id,
                    "Evidence",
                )),
                Some(item)
                    if !item
                        .discoverable_by
                        .iter()
                        .any(|v| matches!(v, DiscoveryRule::StartingEvidence)) =>
                {
                    errors.push(ValidationError::Semantic {
                        field: format!("initial_player_knowledge.evidence_ids[{i}]"),
                        detail: "initial evidence must use StartingEvidence discovery rule".into(),
                    })
                }
                Some(_) => {}
            }
        }
        errors
    }

    fn check_culprit_reachability(&self, require_confession: bool) -> Vec<ValidationError> {
        let culprits: Vec<_> = self
            .characters
            .iter()
            .filter(|c| c.disclosure_graph.confession_node().is_some())
            .collect();
        if require_confession && culprits.is_empty() {
            return vec![ValidationError::NoCulprit];
        }
        let available_evidence: BTreeSet<EvidenceId> = self
            .evidence
            .iter()
            .filter(|e| !e.discoverable_by.is_empty())
            .map(|e| e.id.clone())
            .collect();
        let invalidated_claims: BTreeSet<ClaimId> = self
            .evidence
            .iter()
            .filter(|e| available_evidence.contains(&e.id))
            .flat_map(|e| e.contradicts.iter().cloned())
            .collect();
        let mut errors = Vec::new();
        for culprit in culprits {
            let mut reached = BTreeSet::new();
            loop {
                let before = reached.len();
                for node in &culprit.disclosure_graph.nodes {
                    if node.prerequisites.iter().all(|p| match p {
                        DisclosurePrerequisite::Disclosure { disclosure } => {
                            reached.contains(disclosure)
                        }
                        DisclosurePrerequisite::EvidencePresented { evidence } => {
                            !evidence.is_empty()
                                && evidence.iter().all(|id| available_evidence.contains(id))
                        }
                        DisclosurePrerequisite::ClaimInvalidated { claim } => {
                            invalidated_claims.contains(claim)
                        }
                        DisclosurePrerequisite::PhaseAtLeast { .. } => true,
                    }) {
                        reached.insert(node.id.clone());
                    }
                }
                if reached.len() == before {
                    break;
                }
            }
            let confession = culprit
                .disclosure_graph
                .confession_node()
                .expect("filtered above");
            if !reached.contains(&confession.id) {
                errors.push(ValidationError::CulpritUnreachable {
                    character: culprit.id.to_string(),
                    detail:
                        "no valid disclosure/evidence/claim prerequisite path reaches Confession"
                            .into(),
                });
            }
        }
        errors
    }
}

fn check_unique<'a>(
    values: impl Iterator<Item = &'a str>,
    field: &str,
    errors: &mut Vec<ValidationError>,
) {
    let mut seen = BTreeSet::new();
    for value in values {
        if !seen.insert(value) {
            errors.push(ValidationError::DuplicateId {
                field: field.into(),
                id: value.into(),
            });
        }
    }
}

fn reference<T: Ord + ToString>(
    set: &BTreeSet<&T>,
    value: &T,
    field: String,
    target: &str,
    errors: &mut Vec<ValidationError>,
) {
    if !set.contains(value) {
        errors.push(not_found(field, value, target));
    }
}

fn not_found(field: String, value: &impl ToString, target: &str) -> ValidationError {
    ValidationError::ReferenceNotFound {
        field,
        reference: value.to_string(),
        target_type: target.into(),
    }
}
