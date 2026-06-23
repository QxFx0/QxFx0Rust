use serde::{Deserialize, Serialize};

/// Semantic frames for render dispatch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SemanticFrame {
    DefinitionFrame {
        topic: String,
        scope: FrameScope,
        authority: FrameAuthority,
    },
    DistinctionFrame {
        left: String,
        right: String,
        criteria: Vec<String>,
    },
    ChallengeFrame {
        target: String,
        basis: String,
        strength: ChallengeStrength,
        raw_obj: String,
    },
    GroundFrame {
        topic: String,
        depth: FrameDepth,
    },
    RepairFrame,
    ContactFrame {
        greeting: String,
    },
    ReflectFrame {
        topic: String,
    },
    LearnFrame {
        topic: String,
        depth: FrameDepth,
    },
    HelpFrame {
        task: String,
    },
    PurposeFrame {
        topic: String,
    },
    WorldCauseFrame {
        topic: String,
    },
    DeepenFrame {
        topic: String,
    },
    NextStepFrame,
    ExploratoryFrame,
    OperationalFrame,
    SelfReferenceFrame,
    GenericFrame {
        content: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FrameScope {
    GeneralScope,
    SpecificScope,
    DomainScope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FrameAuthority {
    Known,
    Probable,
    Uncertain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChallengeStrength {
    Soft,
    Firm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FrameDepth {
    Shallow,
    Detailed,
}
