use crate::{
    CaseElement, CaseId, CharacterDefinition, CharacterId, Ending, Entity, EvidenceDefinition,
    EvidenceId, Fact, FactId, GenerationJobId, PlayerKnowledge, VariantAdditions, VariantId,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NarrativeTone {
    Realistic,
    Noir,
    Suspenseful,
    Light,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Difficulty {
    Easy,
    Medium,
    Hard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RealismLevel {
    Grounded,
    Dramatic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConfessionPolicy {
    NeverRequired,
    PartialThenFull,
    EvidenceOnlyAllowed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GenerationRequest {
    pub theme: String,
    pub setting: String,
    pub tone: NarrativeTone,
    pub target_duration_minutes: u32,
    pub difficulty: Difficulty,
    pub character_count: u32,
    pub variant_count: u32,
    pub realism: RealismLevel,
    pub confession_policy: ConfessionPolicy,
    #[serde(default)]
    pub content_constraints: Vec<String>,
    pub language: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenerationLimits {
    pub min_characters: u32,
    pub max_characters: u32,
    pub min_variants: u32,
    pub max_variants: u32,
    pub min_duration_minutes: u32,
    pub max_duration_minutes: u32,
    pub max_constraints: usize,
    pub max_text_scalars: usize,
    pub max_draft_bytes: usize,
    pub max_repairs: u32,
}

impl Default for GenerationLimits {
    fn default() -> Self {
        Self {
            min_characters: 2,
            max_characters: 4,
            min_variants: 1,
            max_variants: 5,
            min_duration_minutes: 10,
            max_duration_minutes: 120,
            max_constraints: 20,
            max_text_scalars: 4_000,
            max_draft_bytes: 2 * 1024 * 1024,
            max_repairs: 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GenerationIssue {
    pub code: String,
    pub path: String,
    pub message: String,
}

impl GenerationRequest {
    pub fn validate(&self, limits: GenerationLimits) -> Vec<GenerationIssue> {
        let mut issues = Vec::new();
        required_text(&mut issues, "$.theme", &self.theme, limits.max_text_scalars);
        optional_text(
            &mut issues,
            "$.setting",
            &self.setting,
            limits.max_text_scalars,
        );
        required_text(
            &mut issues,
            "$.language",
            &self.language,
            limits.max_text_scalars,
        );
        bounded_u32(
            &mut issues,
            "$.character_count",
            self.character_count,
            limits.min_characters,
            limits.max_characters,
        );
        let minimum_for_difficulty = match self.difficulty {
            Difficulty::Easy => 10,
            Difficulty::Medium => 25,
            Difficulty::Hard => 45,
        };
        if self.target_duration_minutes < minimum_for_difficulty {
            issues.push(issue(
                "GENERATION_INCOHERENT_SCOPE",
                "$.difficulty",
                format!("selected difficulty requires at least {minimum_for_difficulty} minutes"),
            ));
        }
        let (max_characters, max_variants) = match self.target_duration_minutes {
            0..=24 => (3, 1),
            25..=44 => (4, 2),
            45..=74 => (4, 3),
            _ => (4, 5),
        };
        if self.character_count > max_characters {
            issues.push(issue(
                "GENERATION_INCOHERENT_SCOPE",
                "$.character_count",
                format!(
                    "{} minutes supports at most {max_characters} characters",
                    self.target_duration_minutes
                ),
            ));
        }
        if self.variant_count > max_variants {
            issues.push(issue(
                "GENERATION_INCOHERENT_SCOPE",
                "$.variant_count",
                format!(
                    "{} minutes supports at most {max_variants} variants",
                    self.target_duration_minutes
                ),
            ));
        }
        bounded_u32(
            &mut issues,
            "$.variant_count",
            self.variant_count,
            limits.min_variants,
            limits.max_variants,
        );
        bounded_u32(
            &mut issues,
            "$.target_duration_minutes",
            self.target_duration_minutes,
            limits.min_duration_minutes,
            limits.max_duration_minutes,
        );
        if self.content_constraints.len() > limits.max_constraints {
            issues.push(issue(
                "GENERATION_TOO_MANY_CONSTRAINTS",
                "$.content_constraints",
                format!("maximum is {}", limits.max_constraints),
            ));
        }
        for (index, constraint) in self.content_constraints.iter().enumerate() {
            required_text(
                &mut issues,
                &format!("$.content_constraints[{index}]"),
                constraint,
                limits.max_text_scalars,
            );
        }
        issues
    }
}

fn optional_text(issues: &mut Vec<GenerationIssue>, path: &str, value: &str, max: usize) {
    if value.chars().count() > max {
        issues.push(issue(
            "GENERATION_TEXT_TOO_LONG",
            path,
            format!("maximum is {max} Unicode scalars"),
        ));
    }
}

fn required_text(issues: &mut Vec<GenerationIssue>, path: &str, value: &str, max: usize) {
    if value.trim().is_empty() {
        issues.push(issue(
            "GENERATION_REQUIRED_TEXT_EMPTY",
            path,
            "value must not be blank",
        ));
    } else if value.chars().count() > max {
        issues.push(issue(
            "GENERATION_TEXT_TOO_LONG",
            path,
            format!("maximum is {max} Unicode scalars"),
        ));
    }
}

fn bounded_u32(
    issues: &mut Vec<GenerationIssue>,
    path: &str,
    value: u32,
    minimum: u32,
    maximum: u32,
) {
    if !(minimum..=maximum).contains(&value) {
        issues.push(issue(
            "GENERATION_VALUE_OUT_OF_RANGE",
            path,
            format!("expected {minimum}..={maximum}, got {value}"),
        ));
    }
}

fn issue(code: &str, path: &str, message: impl Into<String>) -> GenerationIssue {
    GenerationIssue {
        code: code.into(),
        path: path.into(),
        message: message.into(),
    }
}

/// Non-authoritative model output. Required authoring fields remain optional until normalization.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GeneratedCaseDraft {
    pub generation_request: GenerationRequest,
    pub schema_version: String,
    pub case: DraftCaseTemplate,
}

/// Small first-pass plan used to freeze identities and variant intent before detailed generation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GeneratedCaseBlueprint {
    pub case: DraftCaseBlueprint,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DraftCaseBlueprint {
    pub id: CaseId,
    pub title: String,
    pub summary: String,
    pub entities: Vec<Entity>,
    pub characters: Vec<DraftCharacterPlan>,
    pub variants: Vec<DraftVariantPlan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DraftCharacterPlan {
    pub id: CharacterId,
    pub name: String,
    pub role: String,
    pub public_profile: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DraftVariantPlan {
    pub id: VariantId,
    pub title: String,
    pub description: String,
    pub responsible_character_id: CharacterId,
    pub core_truth: String,
    pub motive: String,
    pub decisive_evidence_plan: Vec<String>,
}

/// Shared detailed content generated once and reused by every truth variant.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GeneratedSharedCaseDraft {
    pub required_case_elements: BTreeSet<CaseElement>,
    pub shared_facts: Vec<Fact>,
    pub shared_evidence: Vec<EvidenceDefinition>,
    pub shared_characters: Vec<CharacterDefinition>,
    pub initial_player_knowledge: PlayerKnowledge,
}

/// One independently generated truth variant. The envelope makes identity checks explicit.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GeneratedVariantDraft {
    pub weight: Option<u32>,
    pub enabled: Option<bool>,
    #[serde(default)]
    pub fact_replacements: BTreeMap<FactId, Fact>,
    #[serde(default)]
    pub evidence_replacements: BTreeMap<EvidenceId, EvidenceDefinition>,
    #[serde(default)]
    pub character_replacements: BTreeMap<CharacterId, CharacterDefinition>,
    #[serde(default)]
    pub additions: VariantAdditions,
    pub required_case_elements: Option<BTreeSet<CaseElement>>,
    pub ending: Option<Ending>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DraftCaseTemplate {
    pub id: Option<CaseId>,
    pub version: Option<String>,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub locale: Option<String>,
    pub required_case_elements: Option<BTreeSet<CaseElement>>,
    pub entities: Option<Vec<Entity>>,
    pub shared_facts: Option<Vec<Fact>>,
    pub shared_evidence: Option<Vec<EvidenceDefinition>>,
    pub shared_characters: Option<Vec<CharacterDefinition>>,
    pub initial_player_knowledge: Option<PlayerKnowledge>,
    #[serde(default)]
    pub solution_variants: Vec<DraftSolutionVariant>,
    pub default_variant_id: Option<VariantId>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DraftSolutionVariant {
    pub id: Option<VariantId>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub weight: Option<u32>,
    pub enabled: Option<bool>,
    pub responsible_character_id: Option<CharacterId>,
    #[serde(default)]
    pub fact_replacements: BTreeMap<FactId, Fact>,
    #[serde(default)]
    pub evidence_replacements: BTreeMap<EvidenceId, EvidenceDefinition>,
    #[serde(default)]
    pub character_replacements: BTreeMap<CharacterId, CharacterDefinition>,
    #[serde(default)]
    pub additions: VariantAdditions,
    pub required_case_elements: Option<BTreeSet<CaseElement>>,
    pub ending: Option<Ending>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GenerationStatus {
    Pending,
    Drafting,
    Parsing,
    Normalizing,
    Compiling,
    Validating,
    Simulating,
    Repairing,
    Completed,
    Failed,
}

impl GenerationStatus {
    pub fn can_transition_to(self, next: Self) -> bool {
        use GenerationStatus::*;
        matches!(
            (self, next),
            (Pending, Drafting)
                | (Drafting, Parsing | Failed)
                | (Parsing, Normalizing | Repairing | Failed)
                | (Normalizing, Compiling | Repairing | Failed)
                | (Compiling, Validating | Repairing | Failed)
                | (Validating, Simulating | Repairing | Failed)
                | (Simulating, Completed | Repairing | Failed)
                | (Repairing, Parsing | Failed)
        )
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GenerationStatusEvent {
    pub job_id: GenerationJobId,
    pub sequence: u32,
    pub from: GenerationStatus,
    pub to: GenerationStatus,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GenerationRepairRequest {
    pub draft: GeneratedCaseDraft,
    pub issues: Vec<GenerationIssue>,
}
