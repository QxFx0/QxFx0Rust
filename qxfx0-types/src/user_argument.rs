//! Versioned, privacy-bounded contracts for observing user argument structure.
//!
//! The contract deliberately contains no raw user text, span offsets, rendered
//! response, session identity, persistence handle, or response authority. It
//! is a data boundary for a future observation-only parser, not a parser.

use crate::FactId;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fmt};
use thiserror::Error;

pub const USER_ARGUMENT_PARSE_VERSION: u8 = 1;
const RECEIPT_DOMAIN: &[u8] = b"qxfx0.user-argument-parse.v1\0";
const MAX_ID_BYTES: usize = 256;
const MAX_NODES: usize = 16;
const MAX_RELATIONS: usize = 32;
const MAX_OMISSIONS: usize = 16;
const MAX_TYPED_EVIDENCE_ITEMS: usize = 32;

macro_rules! stable_id {
    ($name:ident, $field:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn try_new(value: impl Into<String>) -> Result<Self, UserArgumentValidationError> {
                let value = value.into();
                validate_id($field, &value)?;
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }

            fn validate(&self) -> Result<(), UserArgumentValidationError> {
                validate_id($field, &self.0)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }
    };
}

stable_id!(UserArgumentNodeId, "node.id");
stable_id!(ArgumentRelationId, "relation.id");
stable_id!(CanonicalArgumentTopicId, "proposition.canonical_topic_id");

/// Stable identity and schema version of one deterministic parser rule.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParserRuleId {
    id: String,
    version: u16,
}

impl ParserRuleId {
    pub fn try_new(
        id: impl Into<String>,
        version: u16,
    ) -> Result<Self, UserArgumentValidationError> {
        let rule = Self {
            id: id.into(),
            version,
        };
        rule.validate()?;
        Ok(rule)
    }

    pub fn as_str(&self) -> &str {
        &self.id
    }

    pub const fn version(&self) -> u16 {
        self.version
    }

    fn validate(&self) -> Result<(), UserArgumentValidationError> {
        validate_id("parser_rule.id", &self.id)?;
        if self.version == 0 {
            return Err(UserArgumentValidationError::InvalidParserRuleVersion);
        }
        Ok(())
    }
}

/// Confidence expressed in basis points. This is evidence, never authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ParseConfidence(u16);

impl ParseConfidence {
    pub const MAX_BASIS_POINTS: u16 = 10_000;

    pub fn from_basis_points(value: u16) -> Result<Self, UserArgumentValidationError> {
        if value > Self::MAX_BASIS_POINTS {
            Err(UserArgumentValidationError::ConfidenceOutOfRange(value))
        } else {
            Ok(Self(value))
        }
    }

    pub const fn basis_points(self) -> u16 {
        self.0
    }

    fn validate(self) -> Result<(), UserArgumentValidationError> {
        Self::from_basis_points(self.0).map(|_| ())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum ArgumentSourceClass {
    Direct,
    Quoted,
    Reported,
    Hypothetical,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum ArgumentPolarity {
    Affirmed,
    Negated,
    Unknown,
}

/// Privacy-safe subject identity. Unknown and external names are categorical.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "id")]
pub enum ArgumentSubject {
    CanonicalTopic(CanonicalArgumentTopicId),
    UnresolvedTopic,
    ExternalSubject,
    Dialogue,
    NoTopic,
}

/// Closed v1 predicate vocabulary. It intentionally cannot carry surface text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum ArgumentPredicate {
    Is,
    Defines,
    HasProperty,
    Causes,
    Enables,
    Prevents,
    Requires,
    Permits,
    Prohibits,
    Values,
    Justifies,
    FollowsFrom,
    Contradicts,
    NeedsEvidence,
    NeedsDefinition,
}

/// Closed or typed object identity; it cannot retain an unknown surface label.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "id")]
pub enum ArgumentObject {
    CanonicalTopic(CanonicalArgumentTopicId),
    Fact(FactId),
    UnresolvedTopic,
    ExternalSubject,
    Evidence,
    Definition,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NormalizedArgumentProposition {
    subject: ArgumentSubject,
    predicate: ArgumentPredicate,
    #[serde(skip_serializing_if = "Option::is_none")]
    object: Option<ArgumentObject>,
}

impl NormalizedArgumentProposition {
    pub fn new(
        subject: ArgumentSubject,
        predicate: ArgumentPredicate,
        object: Option<ArgumentObject>,
    ) -> Result<Self, UserArgumentValidationError> {
        let proposition = Self {
            subject,
            predicate,
            object,
        };
        proposition.validate()?;
        Ok(proposition)
    }

    pub fn subject(&self) -> &ArgumentSubject {
        &self.subject
    }

    pub const fn predicate(&self) -> ArgumentPredicate {
        self.predicate
    }

    pub fn object(&self) -> Option<&ArgumentObject> {
        self.object.as_ref()
    }

    fn validate(&self) -> Result<(), UserArgumentValidationError> {
        validate_subject(&self.subject)?;
        if let Some(object) = &self.object {
            validate_object(object)?;
        }
        Ok(())
    }
}

/// Only these two scopes may carry a span digest into this contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum ArgumentSpanDigestScope {
    /// The source formulation belongs to an explicitly reviewed gold corpus.
    ReviewedGold,
    /// An integrating service supplied a separately governed keyed digest.
    ServiceKeyed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArgumentSpanDigest {
    scope: ArgumentSpanDigestScope,
    digest: [u8; 32],
}

impl ArgumentSpanDigest {
    pub fn new(
        scope: ArgumentSpanDigestScope,
        digest: [u8; 32],
    ) -> Result<Self, UserArgumentValidationError> {
        let value = Self { scope, digest };
        value.validate()?;
        Ok(value)
    }

    pub const fn scope(&self) -> ArgumentSpanDigestScope {
        self.scope
    }

    pub const fn digest(&self) -> &[u8; 32] {
        &self.digest
    }

    fn validate(&self) -> Result<(), UserArgumentValidationError> {
        if self.digest == [0; 32] {
            Err(UserArgumentValidationError::InvalidSpanDigest)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct UserArgumentNodeData {
    id: UserArgumentNodeId,
    proposition: NormalizedArgumentProposition,
    source: ArgumentSourceClass,
    polarity: ArgumentPolarity,
    confidence: ParseConfidence,
    parser_rule: ParserRuleId,
    #[serde(skip_serializing_if = "Option::is_none")]
    span_digest: Option<ArgumentSpanDigest>,
}

impl UserArgumentNodeData {
    fn new(
        id: UserArgumentNodeId,
        proposition: NormalizedArgumentProposition,
        source: ArgumentSourceClass,
        polarity: ArgumentPolarity,
        confidence: ParseConfidence,
        parser_rule: ParserRuleId,
        span_digest: Option<ArgumentSpanDigest>,
    ) -> Result<Self, UserArgumentValidationError> {
        let node = Self {
            id,
            proposition,
            source,
            polarity,
            confidence,
            parser_rule,
            span_digest,
        };
        node.validate()?;
        Ok(node)
    }

    fn validate(&self) -> Result<(), UserArgumentValidationError> {
        self.id.validate()?;
        self.proposition.validate()?;
        self.confidence.validate()?;
        self.parser_rule.validate()?;
        if let Some(span_digest) = &self.span_digest {
            span_digest.validate()?;
        }
        Ok(())
    }
}

macro_rules! argument_node_wrapper {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(UserArgumentNodeData);

        impl $name {
            #[allow(clippy::too_many_arguments)]
            pub fn new(
                id: UserArgumentNodeId,
                proposition: NormalizedArgumentProposition,
                source: ArgumentSourceClass,
                polarity: ArgumentPolarity,
                confidence: ParseConfidence,
                parser_rule: ParserRuleId,
                span_digest: Option<ArgumentSpanDigest>,
            ) -> Result<Self, UserArgumentValidationError> {
                UserArgumentNodeData::new(
                    id,
                    proposition,
                    source,
                    polarity,
                    confidence,
                    parser_rule,
                    span_digest,
                )
                .map(Self)
            }

            pub fn id(&self) -> &UserArgumentNodeId {
                &self.0.id
            }

            pub fn proposition(&self) -> &NormalizedArgumentProposition {
                &self.0.proposition
            }

            pub const fn source(&self) -> ArgumentSourceClass {
                self.0.source
            }

            pub const fn polarity(&self) -> ArgumentPolarity {
                self.0.polarity
            }

            pub const fn confidence(&self) -> ParseConfidence {
                self.0.confidence
            }

            pub fn parser_rule(&self) -> &ParserRuleId {
                &self.0.parser_rule
            }

            pub fn span_digest(&self) -> Option<&ArgumentSpanDigest> {
                self.0.span_digest.as_ref()
            }
        }
    };
}

argument_node_wrapper!(UserClaim);
argument_node_wrapper!(UserPremise);
argument_node_wrapper!(UserConclusion);
argument_node_wrapper!(UserQualifier);
argument_node_wrapper!(UserCounterclaim);

/// Tagged storage form retaining the distinct node construction boundaries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "node")]
pub enum UserArgumentNode {
    Claim(UserClaim),
    Premise(UserPremise),
    Conclusion(UserConclusion),
    Qualifier(UserQualifier),
    Counterclaim(UserCounterclaim),
}

impl UserArgumentNode {
    pub fn id(&self) -> &UserArgumentNodeId {
        &self.data().id
    }

    pub fn proposition(&self) -> &NormalizedArgumentProposition {
        &self.data().proposition
    }

    pub fn parser_rule(&self) -> &ParserRuleId {
        &self.data().parser_rule
    }

    fn data(&self) -> &UserArgumentNodeData {
        match self {
            Self::Claim(node) => &node.0,
            Self::Premise(node) => &node.0,
            Self::Conclusion(node) => &node.0,
            Self::Qualifier(node) => &node.0,
            Self::Counterclaim(node) => &node.0,
        }
    }

    fn kind_tag(&self) -> u8 {
        match self {
            Self::Claim(_) => 1,
            Self::Premise(_) => 2,
            Self::Conclusion(_) => 3,
            Self::Qualifier(_) => 4,
            Self::Counterclaim(_) => 5,
        }
    }

    fn validate(&self) -> Result<(), UserArgumentValidationError> {
        self.data().validate()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum ArgumentRelationKind {
    Supports,
    Attacks,
    Qualifies,
    Rebuts,
    Undercuts,
    Entails,
    Contradicts,
    RequestsEvidence,
    RequestsDefinition,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArgumentRelation {
    id: ArgumentRelationId,
    from: UserArgumentNodeId,
    to: UserArgumentNodeId,
    kind: ArgumentRelationKind,
    confidence: ParseConfidence,
    parser_rule: ParserRuleId,
}

impl ArgumentRelation {
    pub fn new(
        id: ArgumentRelationId,
        from: UserArgumentNodeId,
        to: UserArgumentNodeId,
        kind: ArgumentRelationKind,
        confidence: ParseConfidence,
        parser_rule: ParserRuleId,
    ) -> Result<Self, UserArgumentValidationError> {
        let relation = Self {
            id,
            from,
            to,
            kind,
            confidence,
            parser_rule,
        };
        relation.validate()?;
        Ok(relation)
    }

    pub fn id(&self) -> &ArgumentRelationId {
        &self.id
    }

    pub fn from(&self) -> &UserArgumentNodeId {
        &self.from
    }

    pub fn to(&self) -> &UserArgumentNodeId {
        &self.to
    }

    pub const fn kind(&self) -> ArgumentRelationKind {
        self.kind
    }

    pub const fn confidence(&self) -> ParseConfidence {
        self.confidence
    }

    pub fn parser_rule(&self) -> &ParserRuleId {
        &self.parser_rule
    }

    fn validate(&self) -> Result<(), UserArgumentValidationError> {
        self.id.validate()?;
        self.from.validate()?;
        self.to.validate()?;
        self.confidence.validate()?;
        self.parser_rule.validate()?;
        if self.from == self.to {
            return Err(UserArgumentValidationError::SelfRelation(
                self.from.to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum ParseDisposition {
    Parsed,
    Partial,
    Abstained,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum ParseOmissionReason {
    AmbiguousAttachment,
    UnresolvedProposition,
    QuotedPositionAmbiguity,
    UnsupportedRelation,
    NegationAmbiguity,
    InsufficientEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParseOmission {
    reason: ParseOmissionReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    parser_rule: Option<ParserRuleId>,
}

impl ParseOmission {
    pub fn new(
        reason: ParseOmissionReason,
        parser_rule: Option<ParserRuleId>,
    ) -> Result<Self, UserArgumentValidationError> {
        let omission = Self {
            reason,
            parser_rule,
        };
        omission.validate()?;
        Ok(omission)
    }

    pub const fn reason(&self) -> ParseOmissionReason {
        self.reason
    }

    pub fn parser_rule(&self) -> Option<&ParserRuleId> {
        self.parser_rule.as_ref()
    }

    fn validate(&self) -> Result<(), UserArgumentValidationError> {
        if let Some(parser_rule) = &self.parser_rule {
            parser_rule.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserArgumentParseReceipt {
    pub version: u8,
    pub disposition: ParseDisposition,
    pub nodes: Vec<UserArgumentNode>,
    pub relations: Vec<ArgumentRelation>,
    pub omissions: Vec<ParseOmission>,
    pub digest: [u8; 32],
}

impl UserArgumentParseReceipt {
    pub fn new(
        disposition: ParseDisposition,
        nodes: Vec<UserArgumentNode>,
        relations: Vec<ArgumentRelation>,
        omissions: Vec<ParseOmission>,
    ) -> Result<Self, UserArgumentValidationError> {
        let mut receipt = Self {
            version: USER_ARGUMENT_PARSE_VERSION,
            disposition,
            nodes,
            relations,
            omissions,
            digest: [0; 32],
        };
        receipt.validate_structure()?;
        receipt.digest = receipt.calculate_digest();
        Ok(receipt)
    }

    pub fn validate(&self) -> Result<(), UserArgumentValidationError> {
        self.validate_structure()?;
        if self.digest != self.calculate_digest() {
            return Err(UserArgumentValidationError::DigestMismatch);
        }
        Ok(())
    }

    fn validate_structure(&self) -> Result<(), UserArgumentValidationError> {
        if self.version != USER_ARGUMENT_PARSE_VERSION {
            return Err(UserArgumentValidationError::UnsupportedVersion(
                self.version,
            ));
        }
        if self.nodes.len() > MAX_NODES {
            return Err(UserArgumentValidationError::BoundExceeded("nodes"));
        }
        if self.relations.len() > MAX_RELATIONS {
            return Err(UserArgumentValidationError::BoundExceeded("relations"));
        }
        if self.omissions.len() > MAX_OMISSIONS {
            return Err(UserArgumentValidationError::BoundExceeded("omissions"));
        }

        let evidence_items = self.nodes.len()
            + self.relations.len()
            + self
                .omissions
                .iter()
                .filter(|omission| omission.parser_rule.is_some())
                .count();
        if evidence_items > MAX_TYPED_EVIDENCE_ITEMS {
            return Err(UserArgumentValidationError::BoundExceeded("typed_evidence"));
        }

        validate_disposition(
            self.disposition,
            self.nodes.len(),
            self.relations.len(),
            self.omissions.len(),
        )?;

        let mut node_ids = BTreeSet::new();
        for node in &self.nodes {
            node.validate()?;
            if !node_ids.insert(node.id().clone()) {
                return Err(UserArgumentValidationError::DuplicateNode(
                    node.id().to_string(),
                ));
            }
        }

        let mut relation_ids = BTreeSet::new();
        let mut relation_tuples = BTreeSet::new();
        for relation in &self.relations {
            relation.validate()?;
            if !relation_ids.insert(relation.id.clone()) {
                return Err(UserArgumentValidationError::DuplicateRelation(
                    relation.id.to_string(),
                ));
            }
            if !node_ids.contains(&relation.from) || !node_ids.contains(&relation.to) {
                return Err(UserArgumentValidationError::UnknownNodeReference);
            }
            if !relation_tuples.insert((relation.from.clone(), relation.to.clone(), relation.kind))
            {
                return Err(UserArgumentValidationError::DuplicateRelationTuple);
            }
        }

        for omission in &self.omissions {
            omission.validate()?;
        }
        Ok(())
    }

    fn calculate_digest(&self) -> [u8; 32] {
        let mut digest = Sha256::new();
        digest.update(RECEIPT_DOMAIN);
        digest.update([self.version, self.disposition as u8]);

        push_u64(&mut digest, self.nodes.len());
        for node in &self.nodes {
            digest.update([node.kind_tag()]);
            hash_node(&mut digest, node.data());
        }

        push_u64(&mut digest, self.relations.len());
        for relation in &self.relations {
            push_bytes(&mut digest, relation.id.as_str().as_bytes());
            push_bytes(&mut digest, relation.from.as_str().as_bytes());
            push_bytes(&mut digest, relation.to.as_str().as_bytes());
            digest.update([relation.kind as u8]);
            push_confidence(&mut digest, relation.confidence);
            hash_parser_rule(&mut digest, &relation.parser_rule);
        }

        push_u64(&mut digest, self.omissions.len());
        for omission in &self.omissions {
            digest.update([omission.reason as u8]);
            match &omission.parser_rule {
                Some(parser_rule) => {
                    digest.update([1]);
                    hash_parser_rule(&mut digest, parser_rule);
                }
                None => digest.update([0]),
            }
        }
        digest.finalize().into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UserArgumentValidationError {
    #[error("unsupported user argument parse version {0}")]
    UnsupportedVersion(u8),
    #[error("{0} is empty, too long, or contains a control character")]
    InvalidId(&'static str),
    #[error("parser rule version must be greater than zero")]
    InvalidParserRuleVersion,
    #[error("parse confidence {0} exceeds 10000 basis points")]
    ConfidenceOutOfRange(u16),
    #[error("argument span digest must not be all zeroes")]
    InvalidSpanDigest,
    #[error("user argument parse bound exceeded: {0}")]
    BoundExceeded(&'static str),
    #[error("parse disposition {0:?} is inconsistent with graph and omissions")]
    InvalidDisposition(ParseDisposition),
    #[error("duplicate user argument node '{0}'")]
    DuplicateNode(String),
    #[error("duplicate user argument relation '{0}'")]
    DuplicateRelation(String),
    #[error("duplicate user argument relation tuple")]
    DuplicateRelationTuple,
    #[error("user argument relation references an unknown node")]
    UnknownNodeReference,
    #[error("user argument node '{0}' has a self-relation")]
    SelfRelation(String),
    #[error("user argument parse digest does not match its payload")]
    DigestMismatch,
}

fn validate_id(field: &'static str, value: &str) -> Result<(), UserArgumentValidationError> {
    if value.trim().is_empty() || value.len() > MAX_ID_BYTES || value.chars().any(char::is_control)
    {
        Err(UserArgumentValidationError::InvalidId(field))
    } else {
        Ok(())
    }
}

fn validate_subject(subject: &ArgumentSubject) -> Result<(), UserArgumentValidationError> {
    if let ArgumentSubject::CanonicalTopic(id) = subject {
        id.validate()?;
    }
    Ok(())
}

fn validate_object(object: &ArgumentObject) -> Result<(), UserArgumentValidationError> {
    match object {
        ArgumentObject::CanonicalTopic(id) => id.validate(),
        ArgumentObject::Fact(id) => validate_id("proposition.fact_id", id.as_str()),
        ArgumentObject::UnresolvedTopic
        | ArgumentObject::ExternalSubject
        | ArgumentObject::Evidence
        | ArgumentObject::Definition => Ok(()),
    }
}

fn validate_disposition(
    disposition: ParseDisposition,
    nodes: usize,
    relations: usize,
    omissions: usize,
) -> Result<(), UserArgumentValidationError> {
    let valid = match disposition {
        ParseDisposition::Parsed => nodes > 0 && omissions == 0,
        ParseDisposition::Partial => nodes > 0 && omissions > 0,
        ParseDisposition::Abstained => nodes == 0 && relations == 0 && omissions > 0,
    };
    if valid {
        Ok(())
    } else {
        Err(UserArgumentValidationError::InvalidDisposition(disposition))
    }
}

fn hash_node(digest: &mut Sha256, node: &UserArgumentNodeData) {
    push_bytes(digest, node.id.as_str().as_bytes());
    hash_proposition(digest, &node.proposition);
    digest.update([node.source as u8, node.polarity as u8]);
    push_confidence(digest, node.confidence);
    hash_parser_rule(digest, &node.parser_rule);
    match &node.span_digest {
        Some(span_digest) => {
            digest.update([1, span_digest.scope as u8]);
            digest.update(span_digest.digest);
        }
        None => digest.update([0]),
    }
}

fn hash_proposition(digest: &mut Sha256, proposition: &NormalizedArgumentProposition) {
    hash_subject(digest, &proposition.subject);
    digest.update([proposition.predicate as u8]);
    match &proposition.object {
        Some(object) => {
            digest.update([1]);
            hash_object(digest, object);
        }
        None => digest.update([0]),
    }
}

fn hash_subject(digest: &mut Sha256, subject: &ArgumentSubject) {
    match subject {
        ArgumentSubject::CanonicalTopic(id) => {
            digest.update([1]);
            push_bytes(digest, id.as_str().as_bytes());
        }
        ArgumentSubject::UnresolvedTopic => digest.update([2]),
        ArgumentSubject::ExternalSubject => digest.update([3]),
        ArgumentSubject::Dialogue => digest.update([4]),
        ArgumentSubject::NoTopic => digest.update([5]),
    }
}

fn hash_object(digest: &mut Sha256, object: &ArgumentObject) {
    match object {
        ArgumentObject::CanonicalTopic(id) => {
            digest.update([1]);
            push_bytes(digest, id.as_str().as_bytes());
        }
        ArgumentObject::Fact(id) => {
            digest.update([2]);
            push_bytes(digest, id.as_str().as_bytes());
        }
        ArgumentObject::UnresolvedTopic => digest.update([3]),
        ArgumentObject::ExternalSubject => digest.update([4]),
        ArgumentObject::Evidence => digest.update([5]),
        ArgumentObject::Definition => digest.update([6]),
    }
}

fn hash_parser_rule(digest: &mut Sha256, parser_rule: &ParserRuleId) {
    push_bytes(digest, parser_rule.as_str().as_bytes());
    digest.update(parser_rule.version().to_be_bytes());
}

fn push_confidence(digest: &mut Sha256, confidence: ParseConfidence) {
    digest.update(confidence.basis_points().to_be_bytes());
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

    fn id(value: &str) -> UserArgumentNodeId {
        UserArgumentNodeId::try_new(value).unwrap()
    }

    fn rule(value: &str) -> ParserRuleId {
        ParserRuleId::try_new(value, 1).unwrap()
    }

    fn proposition() -> NormalizedArgumentProposition {
        NormalizedArgumentProposition::new(
            ArgumentSubject::CanonicalTopic(CanonicalArgumentTopicId::try_new("freedom").unwrap()),
            ArgumentPredicate::Requires,
            Some(ArgumentObject::Evidence),
        )
        .unwrap()
    }

    fn premise(node_id: &str) -> UserArgumentNode {
        UserArgumentNode::Premise(
            UserPremise::new(
                id(node_id),
                proposition(),
                ArgumentSourceClass::Direct,
                ArgumentPolarity::Affirmed,
                ParseConfidence::from_basis_points(9_000).unwrap(),
                rule("premise.direct"),
                None,
            )
            .unwrap(),
        )
    }

    fn conclusion(node_id: &str) -> UserArgumentNode {
        UserArgumentNode::Conclusion(
            UserConclusion::new(
                id(node_id),
                proposition(),
                ArgumentSourceClass::Direct,
                ArgumentPolarity::Affirmed,
                ParseConfidence::from_basis_points(8_500).unwrap(),
                rule("conclusion.direct"),
                None,
            )
            .unwrap(),
        )
    }

    fn relation(
        relation_id: &str,
        from: &str,
        to: &str,
        kind: ArgumentRelationKind,
    ) -> ArgumentRelation {
        ArgumentRelation::new(
            ArgumentRelationId::try_new(relation_id).unwrap(),
            id(from),
            id(to),
            kind,
            ParseConfidence::from_basis_points(8_000).unwrap(),
            rule("relation.explicit"),
        )
        .unwrap()
    }

    fn receipt() -> UserArgumentParseReceipt {
        UserArgumentParseReceipt::new(
            ParseDisposition::Parsed,
            vec![premise("premise.1"), conclusion("conclusion.1")],
            vec![relation(
                "relation.1",
                "premise.1",
                "conclusion.1",
                ArgumentRelationKind::Supports,
            )],
            vec![],
        )
        .unwrap()
    }

    #[test]
    fn receipt_digest_is_deterministic_and_tamper_evident() {
        let first = receipt();
        let mut second = receipt();
        assert_eq!(first.digest, second.digest);
        second.disposition = ParseDisposition::Partial;
        assert_eq!(
            second.validate(),
            Err(UserArgumentValidationError::InvalidDisposition(
                ParseDisposition::Partial
            ))
        );

        let mut tampered = receipt();
        tampered.relations[0].confidence = ParseConfidence::from_basis_points(7_999).unwrap();
        assert_eq!(
            tampered.validate(),
            Err(UserArgumentValidationError::DigestMismatch)
        );
    }

    #[test]
    fn deserialization_cannot_bypass_version_confidence_or_rule_validation() {
        let mut value = serde_json::to_value(receipt()).unwrap();
        value["version"] = serde_json::json!(2);
        let malformed: UserArgumentParseReceipt = serde_json::from_value(value).unwrap();
        assert_eq!(
            malformed.validate(),
            Err(UserArgumentValidationError::UnsupportedVersion(2))
        );

        let mut value = serde_json::to_value(receipt()).unwrap();
        value["nodes"][0]["node"]["confidence"] = serde_json::json!(10_001);
        let malformed: UserArgumentParseReceipt = serde_json::from_value(value).unwrap();
        assert_eq!(
            malformed.validate(),
            Err(UserArgumentValidationError::ConfidenceOutOfRange(10_001))
        );

        let mut value = serde_json::to_value(receipt()).unwrap();
        value["relations"][0]["parser_rule"]["version"] = serde_json::json!(0);
        let malformed: UserArgumentParseReceipt = serde_json::from_value(value).unwrap();
        assert_eq!(
            malformed.validate(),
            Err(UserArgumentValidationError::InvalidParserRuleVersion)
        );
    }

    #[test]
    fn deserialization_denies_unknown_fields_at_receipt_and_node_boundaries() {
        let mut value = serde_json::to_value(receipt()).unwrap();
        value["raw_input"] = serde_json::json!("forbidden");
        assert!(serde_json::from_value::<UserArgumentParseReceipt>(value).is_err());

        let mut value = serde_json::to_value(receipt()).unwrap();
        value["nodes"][0]["node"]["raw_span"] = serde_json::json!("forbidden");
        assert!(serde_json::from_value::<UserArgumentParseReceipt>(value).is_err());
    }

    #[test]
    fn categorical_subjects_cannot_smuggle_unknown_labels() {
        let safe = NormalizedArgumentProposition::new(
            ArgumentSubject::UnresolvedTopic,
            ArgumentPredicate::NeedsDefinition,
            Some(ArgumentObject::ExternalSubject),
        )
        .unwrap();
        let serialized = serde_json::to_string(&safe).unwrap();
        assert!(!serialized.contains("id"));

        let mut value = serde_json::to_value(safe).unwrap();
        value["subject"]["id"] = serde_json::json!("private_unknown_topic");
        assert!(serde_json::from_value::<NormalizedArgumentProposition>(value).is_err());
    }

    #[test]
    fn rejects_duplicate_dangling_self_and_duplicate_tuple_relations() {
        assert_eq!(
            UserArgumentParseReceipt::new(
                ParseDisposition::Parsed,
                vec![premise("same"), conclusion("same")],
                vec![],
                vec![],
            )
            .unwrap_err(),
            UserArgumentValidationError::DuplicateNode("same".into())
        );

        let duplicate = relation(
            "relation.1",
            "premise.1",
            "conclusion.1",
            ArgumentRelationKind::Supports,
        );
        assert_eq!(
            UserArgumentParseReceipt::new(
                ParseDisposition::Parsed,
                vec![premise("premise.1"), conclusion("conclusion.1")],
                vec![duplicate.clone(), duplicate],
                vec![],
            )
            .unwrap_err(),
            UserArgumentValidationError::DuplicateRelation("relation.1".into())
        );

        assert_eq!(
            UserArgumentParseReceipt::new(
                ParseDisposition::Parsed,
                vec![premise("premise.1"), conclusion("conclusion.1")],
                vec![relation(
                    "relation.1",
                    "missing",
                    "conclusion.1",
                    ArgumentRelationKind::Supports,
                )],
                vec![],
            )
            .unwrap_err(),
            UserArgumentValidationError::UnknownNodeReference
        );

        assert_eq!(
            ArgumentRelation::new(
                ArgumentRelationId::try_new("self").unwrap(),
                id("premise.1"),
                id("premise.1"),
                ArgumentRelationKind::Supports,
                ParseConfidence::from_basis_points(9_000).unwrap(),
                rule("relation.self"),
            )
            .unwrap_err(),
            UserArgumentValidationError::SelfRelation("premise.1".into())
        );

        assert_eq!(
            UserArgumentParseReceipt::new(
                ParseDisposition::Parsed,
                vec![premise("premise.1"), conclusion("conclusion.1")],
                vec![
                    relation(
                        "relation.1",
                        "premise.1",
                        "conclusion.1",
                        ArgumentRelationKind::Supports,
                    ),
                    relation(
                        "relation.2",
                        "premise.1",
                        "conclusion.1",
                        ArgumentRelationKind::Supports,
                    ),
                ],
                vec![],
            )
            .unwrap_err(),
            UserArgumentValidationError::DuplicateRelationTuple
        );
    }

    #[test]
    fn enforces_disposition_semantics() {
        let omission = ParseOmission::new(ParseOmissionReason::InsufficientEvidence, None).unwrap();
        for (disposition, nodes, omissions) in [
            (ParseDisposition::Parsed, vec![], vec![]),
            (
                ParseDisposition::Parsed,
                vec![premise("premise.1")],
                vec![omission.clone()],
            ),
            (
                ParseDisposition::Partial,
                vec![premise("premise.1")],
                vec![],
            ),
            (
                ParseDisposition::Abstained,
                vec![premise("premise.1")],
                vec![omission.clone()],
            ),
            (ParseDisposition::Abstained, vec![], vec![]),
        ] {
            assert_eq!(
                UserArgumentParseReceipt::new(disposition, nodes, vec![], omissions).unwrap_err(),
                UserArgumentValidationError::InvalidDisposition(disposition)
            );
        }

        UserArgumentParseReceipt::new(
            ParseDisposition::Partial,
            vec![premise("premise.1")],
            vec![],
            vec![omission.clone()],
        )
        .unwrap();
        UserArgumentParseReceipt::new(ParseDisposition::Abstained, vec![], vec![], vec![omission])
            .unwrap();
    }

    #[test]
    fn enforces_ids_confidence_span_digest_and_collection_bounds() {
        assert!(matches!(
            UserArgumentNodeId::try_new("\n"),
            Err(UserArgumentValidationError::InvalidId("node.id"))
        ));
        assert!(matches!(
            UserArgumentNodeId::try_new("a".repeat(257)),
            Err(UserArgumentValidationError::InvalidId("node.id"))
        ));
        assert_eq!(
            ParseConfidence::from_basis_points(10_001),
            Err(UserArgumentValidationError::ConfidenceOutOfRange(10_001))
        );
        assert_eq!(
            ArgumentSpanDigest::new(ArgumentSpanDigestScope::ReviewedGold, [0; 32]),
            Err(UserArgumentValidationError::InvalidSpanDigest)
        );

        let nodes = (0..17)
            .map(|index| premise(&format!("node.{index}")))
            .collect();
        assert_eq!(
            UserArgumentParseReceipt::new(ParseDisposition::Parsed, nodes, vec![], vec![])
                .unwrap_err(),
            UserArgumentValidationError::BoundExceeded("nodes")
        );

        let omissions =
            vec![ParseOmission::new(ParseOmissionReason::InsufficientEvidence, None).unwrap(); 17];
        assert_eq!(
            UserArgumentParseReceipt::new(ParseDisposition::Abstained, vec![], vec![], omissions,)
                .unwrap_err(),
            UserArgumentValidationError::BoundExceeded("omissions")
        );

        let relations = vec![
            relation(
                "relation.1",
                "premise.1",
                "conclusion.1",
                ArgumentRelationKind::Supports,
            );
            33
        ];
        assert_eq!(
            UserArgumentParseReceipt::new(
                ParseDisposition::Parsed,
                vec![premise("premise.1"), conclusion("conclusion.1")],
                relations,
                vec![],
            )
            .unwrap_err(),
            UserArgumentValidationError::BoundExceeded("relations")
        );

        let nodes: Vec<_> = (0..16)
            .map(|index| premise(&format!("node.{index}")))
            .collect();
        let relations: Vec<_> = (0..17)
            .map(|index| {
                relation(
                    &format!("relation.{index}"),
                    &format!("node.{}", index % 16),
                    &format!("node.{}", (index + 1) % 16),
                    ArgumentRelationKind::Supports,
                )
            })
            .collect();
        assert_eq!(
            UserArgumentParseReceipt::new(ParseDisposition::Parsed, nodes, relations, vec![],)
                .unwrap_err(),
            UserArgumentValidationError::BoundExceeded("typed_evidence")
        );

        let mut value = serde_json::to_value(receipt()).unwrap();
        value["nodes"][0]["node"]["span_digest"] = serde_json::json!({
            "scope": "reviewed_gold",
            "digest": serde_json::to_value([0_u8; 32]).unwrap(),
        });
        let malformed: UserArgumentParseReceipt = serde_json::from_value(value).unwrap();
        assert_eq!(
            malformed.validate(),
            Err(UserArgumentValidationError::InvalidSpanDigest)
        );
    }

    #[test]
    fn serialized_contract_has_no_raw_or_identity_bearing_fields() {
        let serialized = serde_json::to_string(&receipt()).unwrap();
        for forbidden in [
            "raw_input",
            "raw_span",
            "response_text",
            "session_id",
            "request_id",
            "user_id",
            "character_offset",
            "Секретная пользовательская формулировка",
        ] {
            assert!(!serialized.contains(forbidden));
        }
    }

    #[test]
    fn all_public_node_wrappers_retain_their_distinct_tags() {
        let make = |node_id: &str| {
            (
                id(node_id),
                proposition(),
                ArgumentSourceClass::Quoted,
                ArgumentPolarity::Unknown,
                ParseConfidence::from_basis_points(7_500).unwrap(),
                rule("node.typed"),
                Some(
                    ArgumentSpanDigest::new(ArgumentSpanDigestScope::ServiceKeyed, [7; 32])
                        .unwrap(),
                ),
            )
        };
        let (id, proposition, source, polarity, confidence, rule, span) = make("claim");
        let claim = UserArgumentNode::Claim(
            UserClaim::new(id, proposition, source, polarity, confidence, rule, span).unwrap(),
        );
        let (id, proposition, source, polarity, confidence, rule, span) = make("qualifier");
        let qualifier = UserArgumentNode::Qualifier(
            UserQualifier::new(id, proposition, source, polarity, confidence, rule, span).unwrap(),
        );
        let (id, proposition, source, polarity, confidence, rule, span) = make("counterclaim");
        let counterclaim = UserArgumentNode::Counterclaim(
            UserCounterclaim::new(id, proposition, source, polarity, confidence, rule, span)
                .unwrap(),
        );
        assert_eq!(claim.kind_tag(), 1);
        assert_eq!(qualifier.kind_tag(), 4);
        assert_eq!(counterclaim.kind_tag(), 5);
    }
}
