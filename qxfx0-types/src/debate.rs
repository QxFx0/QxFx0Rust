//! Versioned, observation-only contracts for argument analysis.
//!
//! These types contain no user text, rendering authority, persistence, clock,
//! or mutable session state. They describe evidence already produced by a turn.

use crate::FactId;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use thiserror::Error;

pub const DEBATE_OBSERVATION_VERSION: u8 = 1;
const RECEIPT_DOMAIN: &[u8] = b"qxfx0.debate-observation.v1\0";
const MAX_ID_BYTES: usize = 256;
const MAX_NODES: usize = 16;
const MAX_EDGES: usize = 32;
const MAX_LEDGER_ENTRIES: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DebateMove {
    Define,
    Assert,
    Challenge,
    Distinguish,
    Ground,
    Counter,
    InferConsequence,
    Clarify,
    Reflect,
    Connect,
    Contact,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DebateParticipant {
    User,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArgumentNodeKind {
    Thesis,
    Support,
    Counterpoint,
    Consequence,
    DialogueAct,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArgumentNode {
    pub id: String,
    pub kind: ArgumentNodeKind,
    pub participant: DebateParticipant,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fact_id: Option<FactId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArgumentEdgeKind {
    Supports,
    Counters,
    Entails,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArgumentEdge {
    pub from: String,
    pub to: String,
    pub kind: ArgumentEdgeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PositionPolarity {
    Proposed,
    Supported,
    Opposed,
    Qualified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerEntry {
    pub sequence: u16,
    pub participant: DebateParticipant,
    pub node_id: String,
    pub polarity: PositionPolarity,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "id")]
pub enum DebateEvidenceRef {
    Fact(FactId),
    ArgumentNode(String),
    RouteFamily(String),
    PlanOutcome(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RubricDimension {
    ClaimClarity,
    EvidenceGrounding,
    CounterargumentCoverage,
    ConsequenceCoverage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RubricScore(u16);

impl RubricScore {
    pub const MAX_BASIS_POINTS: u16 = 10_000;

    pub fn from_basis_points(value: u16) -> Result<Self, DebateValidationError> {
        if value > Self::MAX_BASIS_POINTS {
            Err(DebateValidationError::RubricScoreOutOfRange(value))
        } else {
            Ok(Self(value))
        }
    }

    pub const fn basis_points(self) -> u16 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RubricAssessment {
    pub dimension: RubricDimension,
    pub score: RubricScore,
    pub evidence: Vec<DebateEvidenceRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DebateObservationReceipt {
    pub version: u8,
    pub topic_id: String,
    pub move_type: DebateMove,
    pub nodes: Vec<ArgumentNode>,
    pub edges: Vec<ArgumentEdge>,
    pub ledger: Vec<LedgerEntry>,
    pub rubric: Vec<RubricAssessment>,
    pub digest: [u8; 32],
}

impl DebateObservationReceipt {
    pub fn new(
        topic_id: String,
        move_type: DebateMove,
        nodes: Vec<ArgumentNode>,
        edges: Vec<ArgumentEdge>,
        ledger: Vec<LedgerEntry>,
        rubric: Vec<RubricAssessment>,
    ) -> Result<Self, DebateValidationError> {
        let mut receipt = Self {
            version: DEBATE_OBSERVATION_VERSION,
            topic_id,
            move_type,
            nodes,
            edges,
            ledger,
            rubric,
            digest: [0; 32],
        };
        receipt.validate_structure()?;
        receipt.digest = receipt.calculate_digest();
        Ok(receipt)
    }

    pub fn validate(&self) -> Result<(), DebateValidationError> {
        self.validate_structure()?;
        if self.digest != self.calculate_digest() {
            return Err(DebateValidationError::DigestMismatch);
        }
        Ok(())
    }

    fn validate_structure(&self) -> Result<(), DebateValidationError> {
        if self.version != DEBATE_OBSERVATION_VERSION {
            return Err(DebateValidationError::UnsupportedVersion(self.version));
        }
        validate_id("topic_id", &self.topic_id)?;
        if self.nodes.len() > MAX_NODES {
            return Err(DebateValidationError::BoundExceeded("nodes"));
        }
        if self.edges.len() > MAX_EDGES {
            return Err(DebateValidationError::BoundExceeded("edges"));
        }
        if self.ledger.len() > MAX_LEDGER_ENTRIES {
            return Err(DebateValidationError::BoundExceeded("ledger"));
        }
        let mut node_ids = BTreeSet::new();
        for node in &self.nodes {
            validate_id("node.id", &node.id)?;
            if !node_ids.insert(node.id.as_str()) {
                return Err(DebateValidationError::DuplicateNode(node.id.clone()));
            }
        }
        for edge in &self.edges {
            if !node_ids.contains(edge.from.as_str()) || !node_ids.contains(edge.to.as_str()) {
                return Err(DebateValidationError::UnknownNodeReference);
            }
            if edge.from == edge.to {
                return Err(DebateValidationError::SelfEdge(edge.from.clone()));
            }
        }
        for (index, entry) in self.ledger.iter().enumerate() {
            if entry.sequence as usize != index || !node_ids.contains(entry.node_id.as_str()) {
                return Err(DebateValidationError::InvalidLedger);
            }
        }
        let mut dimensions = BTreeSet::new();
        for assessment in &self.rubric {
            if assessment.score.basis_points() > RubricScore::MAX_BASIS_POINTS {
                return Err(DebateValidationError::RubricScoreOutOfRange(
                    assessment.score.basis_points(),
                ));
            }
            if assessment.evidence.is_empty() {
                return Err(DebateValidationError::RubricWithoutEvidence);
            }
            if !dimensions.insert(assessment.dimension as u8) {
                return Err(DebateValidationError::DuplicateRubricDimension);
            }
            for evidence in &assessment.evidence {
                if let DebateEvidenceRef::ArgumentNode(id) = evidence {
                    if !node_ids.contains(id.as_str()) {
                        return Err(DebateValidationError::UnknownNodeReference);
                    }
                }
            }
        }
        Ok(())
    }

    fn calculate_digest(&self) -> [u8; 32] {
        let mut digest = Sha256::new();
        digest.update(RECEIPT_DOMAIN);
        digest.update([self.version, self.move_type as u8]);
        push_bytes(&mut digest, self.topic_id.as_bytes());
        push_u64(&mut digest, self.nodes.len());
        for node in &self.nodes {
            push_bytes(&mut digest, node.id.as_bytes());
            digest.update([node.kind as u8, node.participant as u8]);
            match &node.fact_id {
                Some(fact_id) => {
                    digest.update([1]);
                    push_bytes(&mut digest, fact_id.as_str().as_bytes());
                }
                None => digest.update([0]),
            }
        }
        push_u64(&mut digest, self.edges.len());
        for edge in &self.edges {
            push_bytes(&mut digest, edge.from.as_bytes());
            push_bytes(&mut digest, edge.to.as_bytes());
            digest.update([edge.kind as u8]);
        }
        push_u64(&mut digest, self.ledger.len());
        for entry in &self.ledger {
            digest.update(entry.sequence.to_be_bytes());
            digest.update([entry.participant as u8, entry.polarity as u8]);
            push_bytes(&mut digest, entry.node_id.as_bytes());
        }
        push_u64(&mut digest, self.rubric.len());
        for assessment in &self.rubric {
            digest.update([
                assessment.dimension as u8,
                (assessment.score.basis_points() >> 8) as u8,
                assessment.score.basis_points() as u8,
            ]);
            push_u64(&mut digest, assessment.evidence.len());
            for evidence in &assessment.evidence {
                match evidence {
                    DebateEvidenceRef::Fact(id) => {
                        digest.update([1]);
                        push_bytes(&mut digest, id.as_str().as_bytes());
                    }
                    DebateEvidenceRef::ArgumentNode(id) => {
                        digest.update([2]);
                        push_bytes(&mut digest, id.as_bytes());
                    }
                    DebateEvidenceRef::RouteFamily(id) => {
                        digest.update([3]);
                        push_bytes(&mut digest, id.as_bytes());
                    }
                    DebateEvidenceRef::PlanOutcome(id) => {
                        digest.update([4]);
                        push_bytes(&mut digest, id.as_bytes());
                    }
                }
            }
        }
        digest.finalize().into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DebateValidationError {
    #[error("unsupported debate observation version {0}")]
    UnsupportedVersion(u8),
    #[error("{0} is empty, too long, or contains a control character")]
    InvalidId(&'static str),
    #[error("debate observation bound exceeded: {0}")]
    BoundExceeded(&'static str),
    #[error("duplicate argument node '{0}'")]
    DuplicateNode(String),
    #[error("argument edge or evidence references an unknown node")]
    UnknownNodeReference,
    #[error("argument node '{0}' has a self-edge")]
    SelfEdge(String),
    #[error("position ledger is not a contiguous append-only projection")]
    InvalidLedger,
    #[error("rubric score {0} exceeds 10000 basis points")]
    RubricScoreOutOfRange(u16),
    #[error("rubric assessment has no typed evidence")]
    RubricWithoutEvidence,
    #[error("rubric dimension occurs more than once")]
    DuplicateRubricDimension,
    #[error("debate observation digest does not match its payload")]
    DigestMismatch,
}

fn validate_id(field: &'static str, value: &str) -> Result<(), DebateValidationError> {
    if value.trim().is_empty() || value.len() > MAX_ID_BYTES || value.chars().any(char::is_control)
    {
        Err(DebateValidationError::InvalidId(field))
    } else {
        Ok(())
    }
}

fn push_u64(digest: &mut Sha256, value: usize) {
    digest.update((value as u64).to_be_bytes());
}

fn push_bytes(digest: &mut Sha256, value: &[u8]) {
    digest.update((value.len() as u64).to_be_bytes());
    digest.update(value);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn receipt() -> DebateObservationReceipt {
        DebateObservationReceipt::new(
            "freedom".into(),
            DebateMove::Define,
            vec![ArgumentNode {
                id: "freedom.thesis".into(),
                kind: ArgumentNodeKind::Thesis,
                participant: DebateParticipant::System,
                fact_id: Some(FactId::try_new("freedom.thesis").unwrap()),
            }],
            vec![],
            vec![LedgerEntry {
                sequence: 0,
                participant: DebateParticipant::System,
                node_id: "freedom.thesis".into(),
                polarity: PositionPolarity::Proposed,
            }],
            vec![RubricAssessment {
                dimension: RubricDimension::EvidenceGrounding,
                score: RubricScore::from_basis_points(10_000).unwrap(),
                evidence: vec![DebateEvidenceRef::ArgumentNode("freedom.thesis".into())],
            }],
        )
        .unwrap()
    }

    #[test]
    fn receipt_digest_is_deterministic_and_tamper_evident() {
        let first = receipt();
        let mut second = receipt();
        assert_eq!(first.digest, second.digest);
        second.topic_id = "justice".into();
        assert_eq!(
            second.validate(),
            Err(DebateValidationError::DigestMismatch)
        );
    }

    #[test]
    fn rejects_dangling_edges_and_unbounded_scores() {
        assert!(matches!(
            RubricScore::from_basis_points(10_001),
            Err(DebateValidationError::RubricScoreOutOfRange(10_001))
        ));
        let error = DebateObservationReceipt::new(
            "freedom".into(),
            DebateMove::Counter,
            receipt().nodes,
            vec![ArgumentEdge {
                from: "missing".into(),
                to: "freedom.thesis".into(),
                kind: ArgumentEdgeKind::Counters,
            }],
            vec![],
            vec![],
        )
        .unwrap_err();
        assert_eq!(error, DebateValidationError::UnknownNodeReference);
    }

    #[test]
    fn deserialized_score_cannot_bypass_validation() {
        let mut value = serde_json::to_value(receipt()).unwrap();
        value["rubric"][0]["score"] = serde_json::json!(10_001);
        value["digest"] = serde_json::to_value([0_u8; 32]).unwrap();
        let malformed: DebateObservationReceipt = serde_json::from_value(value).unwrap();
        assert_eq!(
            malformed.validate(),
            Err(DebateValidationError::RubricScoreOutOfRange(10_001))
        );
    }
}
