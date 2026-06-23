use serde::{Deserialize, Serialize};
use std::fmt;

/// 35 PropositionType variants — intent classification for routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PropositionType {
    DefinitionalQ,
    DistinctionQ,
    GroundQ,
    ReflectiveQ,
    SelfDescQ,
    PurposeQ,
    HypotheticalQ,
    RepairSignal,
    ContactSignal,
    AnchorSignal,
    ClarifyQ,
    DeepenQ,
    ConfrontQ,
    NextStepQ,
    PlainAssert,
    AffectiveQ,
    EpistemicQ,
    RequestQ,
    EvaluationQ,
    NarrativeQ,
    OperationalStatusQ,
    OperationalCauseQ,
    SystemLogicQ,
    SelfKnowledgeQ,
    DialogueInvitationQ,
    ConceptKnowledgeQ,
    WorldCauseQ,
    LocationFormationQ,
    SelfStateQ,
    ComparisonPlausibilityQ,
    MisunderstandingReport,
    GenerativePrompt,
    ContemplativeTopic,
    ExploratoryPrompt,
}

impl PropositionType {
    pub fn to_move_family(&self) -> CanonicalMoveFamily {
        use PropositionType::*;
        use CanonicalMoveFamily::*;
        match self {
            DefinitionalQ | ConceptKnowledgeQ => CMDefine,
            DistinctionQ => CMDistinguish,
            GroundQ => CMGround,
            ReflectiveQ => CMReflect,
            SelfDescQ => CMDescribe,
            PurposeQ => CMPurpose,
            HypotheticalQ => CMHypothesis,
            RepairSignal => CMRepair,
            ContactSignal => CMContact,
            ConfrontQ => CMConfront,
            DeepenQ | DialogueInvitationQ | ContemplativeTopic => CMDeepen,
            NextStepQ => CMNextStep,
            _ => CMGround,
        }
    }
}

impl fmt::Display for PropositionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

use crate::move_family::CanonicalMoveFamily;
