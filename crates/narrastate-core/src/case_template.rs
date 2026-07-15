use crate::{
    CaseDefinition, CaseElement, CaseId, CaseInstanceId, CharacterDefinition, CharacterId,
    ContentHash, Ending, Entity, EvidenceDefinition, EvidenceId, Fact, FactId, PlayerKnowledge,
    VariantId, VisualAssetId,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CaseManifest {
    pub id: CaseId,
    pub version: String,
    pub schema_version: String,
    pub title: String,
    pub language: String,
    pub default_variant_id: VariantId,
    pub variant_count: u32,
    pub generated: bool,
    pub entry: String,
    #[serde(default)]
    pub assets: Vec<AssetManifestEntry>,
    #[serde(default)]
    pub visual_assets: Vec<VisualAssetManifestEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AssetManifestEntry {
    pub path: String,
    pub content_hash: ContentHash,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AssetSemanticRole {
    #[default]
    Decorative,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GeneratedVisualType {
    CaseCover,
    ChapterIllustration,
    SceneBackground,
    LocationAtmosphere,
    CharacterPortrait,
    TransitionIllustration,
    EndingIllustration,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct VisualAssetManifestEntry {
    pub id: VisualAssetId,
    pub path: String,
    pub content_hash: ContentHash,
    pub visual_type: GeneratedVisualType,
    #[serde(default)]
    pub semantic_role: AssetSemanticRole,
    pub alt_text: String,
    pub shared_across_variants: bool,
}

/// Author-facing v0.2 case source. It must be compiled before runtime use.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CaseTemplate {
    pub schema_version: String,
    pub id: CaseId,
    pub version: String,
    pub title: String,
    pub summary: String,
    pub locale: String,
    pub required_case_elements: BTreeSet<CaseElement>,
    pub entities: Vec<Entity>,
    pub shared_facts: Vec<Fact>,
    pub shared_evidence: Vec<EvidenceDefinition>,
    pub shared_characters: Vec<CharacterDefinition>,
    pub initial_player_knowledge: PlayerKnowledge,
    pub solution_variants: Vec<SolutionVariant>,
    pub default_variant_id: VariantId,
}

/// Strongly typed replacements and additions for one complete world truth.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SolutionVariant {
    pub id: VariantId,
    pub title: String,
    pub description: String,
    pub weight: u32,
    pub enabled: bool,
    pub responsible_character_id: CharacterId,
    #[serde(default)]
    pub fact_replacements: BTreeMap<FactId, Fact>,
    #[serde(default)]
    pub evidence_replacements: BTreeMap<EvidenceId, EvidenceDefinition>,
    #[serde(default)]
    pub character_replacements: BTreeMap<CharacterId, CharacterDefinition>,
    #[serde(default)]
    pub additions: VariantAdditions,
    pub required_case_elements: Option<BTreeSet<CaseElement>>,
    pub ending: Ending,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct VariantAdditions {
    #[serde(default)]
    pub facts: Vec<Fact>,
    #[serde(default)]
    pub evidence: Vec<EvidenceDefinition>,
}

/// Complete server-side case selected for runtime. Internal truth fields must be redacted in DTOs.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CompiledCase {
    pub case_id: CaseId,
    pub case_version: String,
    pub schema_version: String,
    pub variant_id: VariantId,
    pub responsible_character_id: CharacterId,
    pub compiled_content_hash: ContentHash,
    pub definition: CaseDefinition,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, PartialOrd, Ord, Hash,
)]
pub struct Seed(pub u64);

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, PartialOrd, Ord, Hash,
)]
pub enum VariantSelectorVersion {
    V1,
}

/// Immutable domain snapshot. Storage adds created_at around this object.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CaseInstance {
    pub instance_id: CaseInstanceId,
    pub case_id: CaseId,
    pub case_version: String,
    pub variant_id: VariantId,
    pub selector_version: VariantSelectorVersion,
    pub seed: Seed,
    pub compiled_content_hash: ContentHash,
    pub instance_hash: ContentHash,
    pub compiled_case: CompiledCase,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum VariantSelection {
    Default,
    Random,
    Specific(VariantId),
}
