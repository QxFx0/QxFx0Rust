use serde::{Deserialize, Serialize};

/// 14 CanonicalMoveFamily — routing decision for each turn.
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
    CMConfront,
    CMDeepen,
    CMNextStep,
    CMClarify,
    CMAnchor,
}
