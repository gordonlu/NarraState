use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub enum InterrogationPhase {
    Calm,
    Guarded,
    Defensive,
    Pressured,
    Cornered,
    ConfessionEligible,
    Resolved,
}

impl InterrogationPhase {
    pub fn can_transition_to(self, target: InterrogationPhase) -> bool {
        use InterrogationPhase::*;
        matches!(
            (self, target),
            (Calm, Guarded | Resolved)
                | (Guarded, Defensive | Resolved)
                | (Defensive, Pressured | Resolved)
                | (Pressured, Cornered | Resolved)
                | (Cornered, ConfessionEligible | Resolved)
                | (ConfessionEligible, Resolved)
        )
    }

    pub fn allowed_targets(self) -> Vec<InterrogationPhase> {
        use InterrogationPhase::*;
        let all = [
            Calm,
            Guarded,
            Defensive,
            Pressured,
            Cornered,
            ConfessionEligible,
            Resolved,
        ];
        all.into_iter()
            .filter(|&t| t != self && self.can_transition_to(t))
            .collect()
    }

    /// Whether this phase is at or past the given phase in the progression order.
    pub fn is_at_least(self, other: InterrogationPhase) -> bool {
        self >= other
    }
}
