use serde::{Deserialize, Serialize};

/// 16 SemanticIntent variants — classified from user input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SemanticIntent {
    IntentDefine(String),
    IntentDistinguish(String, String),
    IntentChallenge,
    IntentGround(String),
    IntentRepair,
    IntentContact,
    IntentReflect,
    IntentLearn(String),
    IntentHelp(String),
    IntentPurpose(String),
    IntentWorldCause(String),
    IntentDeepen(String),
    IntentNextStep,
    IntentExploratory,
    IntentOperational,
    IntentSelfReference,
    IntentUnknown(String),
}
