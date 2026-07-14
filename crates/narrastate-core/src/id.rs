use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

macro_rules! string_id {
    ($name:ident, $doc:expr) => {
        #[derive(
            Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
        )]
        #[doc = $doc]
        pub struct $name(pub String);

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl From<String> for $name {
            fn from(s: String) -> Self {
                $name(s)
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                $name(s.to_string())
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }
    };
}

macro_rules! uuid_id {
    ($name:ident, $doc:expr) => {
        #[derive(
            Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
        )]
        #[doc = $doc]
        pub struct $name(pub Uuid);

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl $name {
            pub fn new() -> Self {
                $name(Uuid::new_v4())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
    };
}

string_id!(CaseId, "Author-provided stable case identifier.");
string_id!(CharacterId, "Author-provided stable character identifier.");
string_id!(FactId, "Author-provided stable fact identifier.");
string_id!(EvidenceId, "Author-provided stable evidence identifier.");
string_id!(ClaimId, "Author-provided stable claim identifier.");
string_id!(
    DisclosureId,
    "Author-provided stable disclosure node identifier."
);
string_id!(
    DefenseStrategyId,
    "Author-provided stable defense strategy identifier."
);
string_id!(
    EntityRef,
    "Reference to a named entity within the case world."
);
string_id!(PropositionRef, "Reference to a proposition by ID.");

uuid_id!(SessionId, "UUID for a game session.");
uuid_id!(TurnId, "UUID for a single turn.");
