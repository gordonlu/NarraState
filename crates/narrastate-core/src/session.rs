use crate::character::CharacterRuntimeState;
use crate::id::{CaseId, CharacterId, EvidenceId, FactId, SessionId, TurnId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SessionState {
    pub session_id: SessionId,
    pub case_id: CaseId,
    pub status: SessionStatus,
    pub current_turn: u32,
    pub active_character: Option<CharacterId>,
    pub discovered_facts: BTreeSet<FactId>,
    pub discovered_evidence: BTreeSet<EvidenceId>,
    pub character_states: BTreeMap<CharacterId, CharacterRuntimeState>,
    pub conversation: Vec<DialogueEntry>,
    pub accusations: Vec<Accusation>,
    pub revision: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum SessionStatus {
    Active,
    Resolved,
    Abandoned,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DialogueEntry {
    pub turn_id: TurnId,
    pub speaker: DialogueSpeaker,
    pub text: String,
    pub attached_evidence: Vec<EvidenceId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum DialogueSpeaker {
    Player,
    Character(CharacterId),
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Accusation {
    pub turn_id: TurnId,
    pub target: CharacterId,
    pub evidence_ids: Vec<EvidenceId>,
    pub reasoning: String,
    pub result: AccusationResult,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum AccusationResult {
    WrongSuspect,
    CorrectButInsufficient,
    CaseProvenWithoutConfession,
    CaseProvenWithConfession,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NarrativeEvent {
    pub event_id: Uuid,
    pub session_id: SessionId,
    pub turn_id: Option<TurnId>,
    pub sequence: u64,
    pub event_type: NarrativeEventKind,
    pub schema_version: u32,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum NarrativeEventKind {
    SessionCreated,
    TurnProcessed,
    EvidenceConfronted,
    AccusationMade,
    PhaseChanged,
    DisclosureRevealed,
    SessionResolved,
    SnapshotTaken,
}
