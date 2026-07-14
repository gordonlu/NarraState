use crate::character::CharacterRuntimeState;
use crate::disclosure::DialogueAct;
use crate::id::{
    CaseId, CharacterId, ClaimId, ClientActionId, DisclosureId, EvidenceId, FactId, SessionId,
    TurnId,
};
use crate::transition::{InterpretedAction, TransitionReason};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SessionState {
    pub session_id: SessionId,
    pub case_id: CaseId,
    #[serde(default)]
    pub mode: SessionMode,
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

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SessionMode {
    #[default]
    Mock,
    Llm,
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
    pub payload: NarrativeEventPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NarrativeEventPayload {
    SessionCreated {
        state: Box<SessionState>,
    },
    PlayerActionAccepted {
        client_action_id: ClientActionId,
        target: CharacterId,
        attached_evidence: Vec<EvidenceId>,
    },
    ActionInterpreted {
        action: InterpretedAction,
    },
    EvidencePresented {
        evidence_ids: Vec<EvidenceId>,
    },
    ClaimContradicted {
        claim_ids: Vec<ClaimId>,
    },
    CharacterStateChanged {
        character_id: CharacterId,
        reason: Option<TransitionReason>,
    },
    DisclosureUnlocked {
        disclosure_ids: Vec<DisclosureId>,
    },
    DialoguePlanned {
        act: DialogueAct,
    },
    DialogueRendered,
    TurnCommitted {
        client_action_id: ClientActionId,
        state: Box<SessionState>,
    },
    AccusationSubmitted {
        state: Box<SessionState>,
    },
    CaseResolved {
        state: Box<SessionState>,
    },
    SnapshotTaken {
        revision: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum NarrativeEventKind {
    SessionCreated,
    PlayerActionAccepted,
    ActionInterpreted,
    EvidencePresented,
    ClaimContradicted,
    CharacterStateChanged,
    DisclosureUnlocked,
    DialoguePlanned,
    DialogueRendered,
    TurnCommitted,
    AccusationSubmitted,
    CaseResolved,
    SnapshotTaken,
}
