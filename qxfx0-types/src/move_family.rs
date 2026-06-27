use serde::{Deserialize, Serialize};

/// 15 CanonicalMoveFamily — routing decision for each turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CanonicalMoveFamily {
    CMDefine,
    CMDistinguish,
    CMGround,
    CMReflect,
    CMDescribe,
    CMPurpose,
    CMHypothesis,
    CMRepair,
    CMContact,
    CMConnect,
    CMConfront,
    CMDeepen,
    CMNextStep,
    CMClarify,
    CMAnchor,
}

impl CanonicalMoveFamily {
    /// Decode a `Debug`-formatted family string (e.g. `"CMDefine"`) back to the enum.
    /// Single source of truth for hint→enum decoding, used by all pipeline stages.
    pub fn from_hint(s: &str) -> Self {
        match s {
            "CMDefine" => Self::CMDefine,
            "CMDistinguish" => Self::CMDistinguish,
            "CMGround" => Self::CMGround,
            "CMReflect" => Self::CMReflect,
            "CMDescribe" => Self::CMDescribe,
            "CMPurpose" => Self::CMPurpose,
            "CMHypothesis" => Self::CMHypothesis,
            "CMRepair" => Self::CMRepair,
            "CMContact" => Self::CMContact,
            "CMConnect" => Self::CMConnect,
            "CMConfront" => Self::CMConfront,
            "CMDeepen" => Self::CMDeepen,
            "CMNextStep" => Self::CMNextStep,
            "CMClarify" => Self::CMClarify,
            "CMAnchor" => Self::CMAnchor,
            _ => Self::CMGround,
        }
    }
}
