use serde::{Deserialize, Serialize};
use std::fmt;

/// 34 PropositionType variants — intent classification for routing.
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

impl fmt::Display for PropositionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}
