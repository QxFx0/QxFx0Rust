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
