use crate::id::{EntityRef, FactId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct StoryTime {
    pub year: Option<i32>,
    pub month: Option<u8>,
    pub day: Option<u8>,
    pub hour: Option<u8>,
    pub minute: Option<u8>,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Fact {
    pub id: FactId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_text: Option<String>,
    pub subject: EntityRef,
    pub predicate: String,
    pub object: FactValue,
    pub happened_at: Option<StoryTime>,
    pub location: Option<EntityRef>,
    pub truth: TruthValue,
    pub tags: BTreeSet<String>,
    pub visibility: FactVisibility,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(untagged)]
pub enum FactValue {
    String(String),
    Number(f64),
    Boolean(bool),
    Entity(EntityRef),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq)]
pub enum TruthValue {
    True,
    False,
    Uncertain,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq)]
pub enum FactVisibility {
    PublicAtStart,
    Discoverable,
    Hidden,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Proposition {
    pub subject: EntityRef,
    pub predicate: String,
    pub object: FactValue,
}
