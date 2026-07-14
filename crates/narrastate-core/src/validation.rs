use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ValidationReport {
    pub valid: bool,
    pub errors: Vec<ValidationError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum ValidationError {
    DuplicateId {
        field: String,
        id: String,
    },
    ReferenceNotFound {
        field: String,
        reference: String,
        target_type: String,
    },
    DisclosureCycle {
        field: String,
        detail: String,
    },
    NoCulprit,
    CulpritUnreachable {
        character: String,
        detail: String,
    },
    RequiredElementNotCovered {
        element: String,
    },
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::DuplicateId { field, id } => {
                write!(f, "{field}: duplicate ID \"{id}\"")
            }
            ValidationError::ReferenceNotFound {
                field,
                reference,
                target_type,
            } => {
                write!(f, "{field}: {target_type} \"{reference}\" not found")
            }
            ValidationError::DisclosureCycle { field, detail } => {
                write!(f, "{field}: {detail}")
            }
            ValidationError::NoCulprit => {
                write!(f, "No character with a Confession disclosure node found")
            }
            ValidationError::CulpritUnreachable { character, detail } => {
                write!(f, "Culprit \"{character}\" unreachable: {detail}")
            }
            ValidationError::RequiredElementNotCovered { element } => {
                write!(
                    f,
                    "Required case element \"{element}\" is not covered by any evidence"
                )
            }
        }
    }
}
