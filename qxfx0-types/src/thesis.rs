//! Typed semantic theses, canonical identity, and deterministic authority graph.

use crate::ConceptId;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub const THESIS_CANONICAL_VERSION: u8 = 1;
pub const MAX_THESIS_ID_BYTES: usize = 256;
pub const MAX_RELATION_ID_BYTES: usize = 256;
pub const MAX_CONCEPT_ID_BYTES: usize = 512;
pub const MAX_QUALIFIERS: usize = 64;
pub const MAX_QUALIFIER_KEY_BYTES: usize = 128;
pub const MAX_QUALIFIER_VALUE_BYTES: usize = 1024;
pub const MAX_SURFACE_TEXT_BYTES: usize = 16 * 1024;
pub const MAX_PROVENANCE_ENTRIES: usize = 64;
pub const MAX_PROVENANCE_KEY_BYTES: usize = 128;
pub const MAX_PROVENANCE_VALUE_BYTES: usize = 2048;
pub const MAX_TIMESTAMP_BYTES: usize = 128;
pub const MAX_GRAPH_THESES: usize = 4096;
pub const MAX_GRAPH_RELATIONS: usize = 16_384;

const THESIS_DOMAIN_V1: &[u8] = b"qxfx0:thesis:canonical:v1";

macro_rules! bounded_id {
    ($name:ident, $error:literal, $max:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn try_new(value: impl Into<String>) -> Result<Self, ThesisError> {
                let value = value.into();
                validate_text($error, &value, $max)?;
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

bounded_id!(ThesisId, "thesis id", MAX_THESIS_ID_BYTES);
bounded_id!(RelationId, "relation id", MAX_RELATION_ID_BYTES);
/// A relation identifier used as the predicate slot of a thesis.
pub type ThesisPredicate = RelationId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThesisKind {
    Definition,
    InterpretiveClaim,
    EmpiricalClaim,
    NormativeClaim,
    Hypothesis,
}

impl ThesisKind {
    const fn canonical_tag(self) -> u8 {
        match self {
            Self::Definition => 0,
            Self::InterpretiveClaim => 1,
            Self::EmpiricalClaim => 2,
            Self::NormativeClaim => 3,
            Self::Hypothesis => 4,
        }
    }
}

/// Semantic thesis. Only subject, predicate, object, kind, and qualifiers form
/// canonical identity; presentation and authority metadata are deliberately excluded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Thesis {
    pub id: ThesisId,
    pub subject: ConceptId,
    pub predicate: ThesisPredicate,
    pub object: ConceptId,
    pub kind: ThesisKind,
    #[serde(default)]
    pub qualifiers: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence_basis_points: Option<u16>,
    #[serde(default)]
    pub provenance: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_to: Option<String>,
}

impl Thesis {
    pub fn validate(&self) -> Result<(), ThesisError> {
        validate_text("thesis id", self.id.as_str(), MAX_THESIS_ID_BYTES)?;
        validate_text("subject concept id", &self.subject.0, MAX_CONCEPT_ID_BYTES)?;
        validate_text("predicate", self.predicate.as_str(), MAX_RELATION_ID_BYTES)?;
        validate_text("object concept id", &self.object.0, MAX_CONCEPT_ID_BYTES)?;
        validate_map(
            "qualifiers",
            &self.qualifiers,
            MAX_QUALIFIERS,
            MAX_QUALIFIER_KEY_BYTES,
            MAX_QUALIFIER_VALUE_BYTES,
        )?;
        if let Some(text) = &self.surface_text {
            validate_text("surface text", text, MAX_SURFACE_TEXT_BYTES)?;
        }
        if self
            .confidence_basis_points
            .is_some_and(|value| value > 10_000)
        {
            return Err(ThesisError::ConfidenceOutOfRange);
        }
        validate_map(
            "provenance",
            &self.provenance,
            MAX_PROVENANCE_ENTRIES,
            MAX_PROVENANCE_KEY_BYTES,
            MAX_PROVENANCE_VALUE_BYTES,
        )?;
        for (name, value) in [
            ("valid_from", &self.valid_from),
            ("valid_to", &self.valid_to),
        ] {
            if let Some(value) = value {
                validate_text(name, value, MAX_TIMESTAMP_BYTES)?;
            }
        }
        Ok(())
    }

    pub fn canonical_bytes_v1(&self) -> Result<Vec<u8>, ThesisError> {
        self.validate()?;
        let mut out = Vec::new();
        put_bytes(&mut out, THESIS_DOMAIN_V1)?;
        out.push(THESIS_CANONICAL_VERSION);
        put_str(&mut out, &self.subject.0)?;
        put_str(&mut out, self.predicate.as_str())?;
        put_str(&mut out, &self.object.0)?;
        out.push(self.kind.canonical_tag());
        put_len(&mut out, self.qualifiers.len())?;
        for (key, value) in &self.qualifiers {
            put_str(&mut out, key)?;
            put_str(&mut out, value)?;
        }
        Ok(out)
    }

    pub fn canonical_digest(&self) -> Result<ThesisDigest, ThesisError> {
        let bytes = self.canonical_bytes_v1()?;
        Ok(ThesisDigest(Sha256::digest(bytes).into()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ThesisDigest([u8; 32]);

impl ThesisDigest {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
    pub fn to_hex(self) -> String {
        use fmt::Write;
        self.0
            .iter()
            .fold(String::with_capacity(64), |mut out, byte| {
                write!(&mut out, "{byte:02x}").expect("writing to String cannot fail");
                out
            })
    }
}

impl fmt::Display for ThesisDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}
impl Serialize for ThesisDigest {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}
impl<'de> Deserialize<'de> for ThesisDigest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        if value.len() != 64
            || !value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(de::Error::custom(
                "thesis digest must be 64 lowercase hexadecimal characters",
            ));
        }
        let mut bytes = [0_u8; 32];
        for (index, slot) in bytes.iter_mut().enumerate() {
            *slot = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
                .map_err(de::Error::custom)?;
        }
        Ok(Self(bytes))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThesisRelationKind {
    Counters,
    Contradicts,
    FollowsFrom,
    Revises,
    Supersedes,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ThesisRelation {
    pub from: ThesisId,
    pub kind: ThesisRelationKind,
    pub to: ThesisId,
}

/// Bounded deterministic authority graph. Map/set iteration is canonical.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThesisGraph {
    theses: BTreeMap<ThesisId, Thesis>,
    relations: BTreeSet<ThesisRelation>,
}

/// Bounded per-session projection of validated catalog theses and lifecycle heads.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThesisState {
    #[serde(default)]
    pub pack_fingerprint: String,
    #[serde(default)]
    pub lifecycles: BTreeMap<ThesisId, ThesisLifecycle>,
    #[serde(default)]
    pub projected_digests: BTreeSet<ThesisDigest>,
}
impl ThesisState {
    pub fn is_empty(&self) -> bool {
        self.pack_fingerprint.is_empty()
            && self.lifecycles.is_empty()
            && self.projected_digests.is_empty()
    }
    pub fn validate_state(&self) -> Result<(), ThesisLifecycleValidationError> {
        if self.lifecycles.len() > MAX_GRAPH_THESES
            || self.projected_digests.len() > MAX_GRAPH_THESES
        {
            return Err(ThesisLifecycleValidationError::TooManyLifecycles);
        }
        if !self.pack_fingerprint.is_empty()
            && (self.pack_fingerprint.len() != 64
                || !self.pack_fingerprint.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err(ThesisLifecycleValidationError::InvalidPackFingerprint);
        }
        for (id, lifecycle) in &self.lifecycles {
            if id != &lifecycle.thesis_id {
                return Err(ThesisLifecycleValidationError::LifecycleKeyMismatch);
            }
            lifecycle.validate_lifecycle()?;
            if let Some(head) = lifecycle.active_head {
                if !self.projected_digests.contains(&head) {
                    return Err(ThesisLifecycleValidationError::UnprojectedHead(head));
                }
            }
        }
        Ok(())
    }
}

impl ThesisGraph {
    pub fn theses(&self) -> &BTreeMap<ThesisId, Thesis> {
        &self.theses
    }
    pub fn relations(&self) -> &BTreeSet<ThesisRelation> {
        &self.relations
    }

    /// Inserts a thesis. Re-inserting an identical value is idempotent.
    pub fn insert_thesis(&mut self, thesis: Thesis) -> Result<bool, ThesisGraphError> {
        thesis.validate()?;
        if let Some(existing) = self.theses.get(&thesis.id) {
            return if existing == &thesis {
                Ok(false)
            } else {
                Err(ThesisGraphError::ConflictingThesis(thesis.id))
            };
        }
        if self.theses.len() >= MAX_GRAPH_THESES {
            return Err(ThesisGraphError::TooManyTheses);
        }
        self.theses.insert(thesis.id.clone(), thesis);
        Ok(true)
    }

    /// Inserts an edge after endpoint, self-edge, bound, and acyclicity checks.
    pub fn insert_relation(&mut self, relation: ThesisRelation) -> Result<bool, ThesisGraphError> {
        if relation.from == relation.to {
            return Err(ThesisGraphError::SelfEdge(relation.from));
        }
        for endpoint in [&relation.from, &relation.to] {
            if !self.theses.contains_key(endpoint) {
                return Err(ThesisGraphError::DanglingEndpoint(endpoint.clone()));
            }
        }
        if self.relations.contains(&relation) {
            return Ok(false);
        }
        if self.relations.len() >= MAX_GRAPH_RELATIONS {
            return Err(ThesisGraphError::TooManyRelations);
        }
        if matches!(
            relation.kind,
            ThesisRelationKind::Revises | ThesisRelationKind::Supersedes
        ) && self.reaches_revision(&relation.to, &relation.from)
        {
            return Err(ThesisGraphError::RevisionCycle);
        }
        self.relations.insert(relation);
        Ok(true)
    }

    fn reaches_revision(&self, start: &ThesisId, target: &ThesisId) -> bool {
        let mut pending = vec![start];
        let mut seen = BTreeSet::new();
        while let Some(current) = pending.pop() {
            if current == target {
                return true;
            }
            if !seen.insert(current.clone()) {
                continue;
            }
            pending.extend(
                self.relations
                    .iter()
                    .filter(|edge| {
                        &edge.from == current
                            && matches!(
                                edge.kind,
                                ThesisRelationKind::Revises | ThesisRelationKind::Supersedes
                            )
                    })
                    .map(|edge| &edge.to),
            );
        }
        false
    }

    pub fn validate(&self) -> Result<(), ThesisGraphError> {
        if self.theses.len() > MAX_GRAPH_THESES {
            return Err(ThesisGraphError::TooManyTheses);
        }
        if self.relations.len() > MAX_GRAPH_RELATIONS {
            return Err(ThesisGraphError::TooManyRelations);
        }
        let mut rebuilt = Self::default();
        for thesis in self.theses.values() {
            if self.theses.get(&thesis.id) != Some(thesis) {
                return Err(ThesisGraphError::KeyMismatch(thesis.id.clone()));
            }
            rebuilt.insert_thesis(thesis.clone())?;
        }
        for relation in &self.relations {
            rebuilt.insert_relation(relation.clone())?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ThesisError {
    #[error("{field} must not be empty")]
    Empty { field: &'static str },
    #[error("{field} exceeds its {max}-byte bound")]
    TooLong { field: &'static str, max: usize },
    #[error("{field} exceeds its {max}-entry bound")]
    TooMany { field: &'static str, max: usize },
    #[error("confidence must be at most 10000 basis points")]
    ConfidenceOutOfRange,
    #[error("canonical component is too large for a u32 length prefix")]
    CanonicalLengthOverflow,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ThesisGraphError {
    #[error(transparent)]
    Thesis(#[from] ThesisError),
    #[error("graph has too many theses")]
    TooManyTheses,
    #[error("graph has too many relations")]
    TooManyRelations,
    #[error("conflicting thesis for id {0}")]
    ConflictingThesis(ThesisId),
    #[error("thesis map key differs from embedded id {0}")]
    KeyMismatch(ThesisId),
    #[error("relation has dangling endpoint {0}")]
    DanglingEndpoint(ThesisId),
    #[error("self-edge is forbidden for thesis {0}")]
    SelfEdge(ThesisId),
    #[error("Revises/Supersedes relation would create a cycle")]
    RevisionCycle,
}

fn validate_text(field: &'static str, value: &str, max: usize) -> Result<(), ThesisError> {
    if value.trim().is_empty() {
        return Err(ThesisError::Empty { field });
    }
    if value.len() > max {
        return Err(ThesisError::TooLong { field, max });
    }
    Ok(())
}
fn validate_map(
    field: &'static str,
    map: &BTreeMap<String, String>,
    max: usize,
    key_max: usize,
    value_max: usize,
) -> Result<(), ThesisError> {
    if map.len() > max {
        return Err(ThesisError::TooMany { field, max });
    }
    for (key, value) in map {
        validate_text("map key", key, key_max)?;
        validate_text("map value", value, value_max)?;
    }
    Ok(())
}
fn put_len(out: &mut Vec<u8>, len: usize) -> Result<(), ThesisError> {
    let len = u32::try_from(len).map_err(|_| ThesisError::CanonicalLengthOverflow)?;
    out.extend_from_slice(&len.to_be_bytes());
    Ok(())
}
fn put_bytes(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), ThesisError> {
    put_len(out, bytes.len())?;
    out.extend_from_slice(bytes);
    Ok(())
}
fn put_str(out: &mut Vec<u8>, value: &str) -> Result<(), ThesisError> {
    put_bytes(out, value.as_bytes())
}

// P1 lifecycle contracts. They are intentionally data-only; pure transitions live in
// qxfx0-commitment and no persistence contract is implied.
pub const MAX_THESIS_REVISIONS: usize = 128;
pub const MAX_THESIS_EVENTS: usize = 512;
pub const MAX_THESIS_TRIGGERS: usize = 512;

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ThesisStatus {
    #[default]
    Draft,
    Active,
    Contested,
    Superseded,
    Retracted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThesisRevisionReason {
    InitialActivation,
    NewEvidence,
    CorrectedError,
    RefinedScope,
    ExplicitCounterargument,
    ExplicitContradiction,
    ReplacedByStrongerThesis,
    ExplicitRetraction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThesisRevisionAction {
    Activate,
    RegisterCounterargument,
    RegisterContradiction,
    Revise,
    Supersede,
    Retract,
}

/// The only closed, non-textual rules accepted as contradiction evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClosedContradictionRule {
    MutuallyExclusiveObjects,
    ExclusiveQualifierValues,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "rule")]
pub enum RelationTriggerBasis {
    ExplicitRelation,
    ClosedRule(ClosedContradictionRule),
}

/// Immutable typed receipt for a relation that caused a lifecycle transition.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ThesisRelationTrigger {
    pub relation: ThesisRelation,
    pub from_digest: ThesisDigest,
    pub to_digest: ThesisDigest,
    pub basis: RelationTriggerBasis,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThesisRevision {
    pub thesis: Thesis,
    pub digest: ThesisDigest,
    #[serde(default)]
    pub status: ThesisStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThesisLifecycleEvent {
    pub sequence: u64,
    pub logical_turn: u64,
    pub action: ThesisRevisionAction,
    pub reason: ThesisRevisionReason,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_head: Option<ThesisDigest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resulting_head: Option<ThesisDigest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation_trigger: Option<ThesisRelationTrigger>,
}

/// Bounded append-only lifecycle. Digests and history keys are immutable identities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThesisLifecycle {
    pub thesis_id: ThesisId,
    #[serde(default)]
    pub status: ThesisStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_head: Option<ThesisDigest>,
    #[serde(default)]
    pub revisions: BTreeMap<ThesisDigest, ThesisRevision>,
    #[serde(default)]
    pub history: BTreeMap<u64, ThesisLifecycleEvent>,
    #[serde(default)]
    pub relation_triggers: BTreeSet<ThesisRelationTrigger>,
}

impl ThesisLifecycle {
    pub fn draft(thesis: Thesis) -> Result<Self, ThesisError> {
        thesis.validate()?;
        let digest = thesis.canonical_digest()?;
        let thesis_id = thesis.id.clone();
        Ok(Self {
            thesis_id,
            status: ThesisStatus::Draft,
            active_head: Some(digest),
            revisions: BTreeMap::from([(
                digest,
                ThesisRevision {
                    thesis,
                    digest,
                    status: ThesisStatus::Draft,
                },
            )]),
            history: BTreeMap::new(),
            relation_triggers: BTreeSet::new(),
        })
    }

    pub fn validate_lifecycle(&self) -> Result<(), ThesisLifecycleValidationError> {
        if self.revisions.len() > MAX_THESIS_REVISIONS {
            return Err(ThesisLifecycleValidationError::TooManyRevisions);
        }
        if self.history.len() > MAX_THESIS_EVENTS {
            return Err(ThesisLifecycleValidationError::TooManyEvents);
        }
        if self.relation_triggers.len() > MAX_THESIS_TRIGGERS {
            return Err(ThesisLifecycleValidationError::TooManyTriggers);
        }
        if let Some(head) = self.active_head {
            if !self.revisions.contains_key(&head) {
                return Err(ThesisLifecycleValidationError::UnknownHead(head));
            }
        }
        let mut prior_turn = None;
        let mut prior_sequence = None;
        for (key, event) in &self.history {
            if key != &event.sequence {
                return Err(ThesisLifecycleValidationError::EventKeyMismatch);
            }
            if prior_sequence.is_some_and(|value| event.sequence <= value) {
                return Err(ThesisLifecycleValidationError::NonIncreasingSequence);
            }
            if prior_turn.is_some_and(|value| event.logical_turn <= value) {
                return Err(ThesisLifecycleValidationError::NonIncreasingLogicalTurn);
            }
            prior_sequence = Some(event.sequence);
            prior_turn = Some(event.logical_turn);
        }
        for (digest, revision) in &self.revisions {
            if revision.thesis.id != self.thesis_id {
                return Err(ThesisLifecycleValidationError::UnstableThesisId);
            }
            if digest != &revision.digest || revision.thesis.canonical_digest()? != *digest {
                return Err(ThesisLifecycleValidationError::DigestMismatch);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ThesisLifecycleValidationError {
    #[error(transparent)]
    Thesis(#[from] ThesisError),
    #[error("lifecycle has too many revisions")]
    TooManyRevisions,
    #[error("lifecycle has too many events")]
    TooManyEvents,
    #[error("lifecycle has too many relation triggers")]
    TooManyTriggers,
    #[error("active head references unknown digest {0}")]
    UnknownHead(ThesisDigest),
    #[error("history key differs from event sequence")]
    EventKeyMismatch,
    #[error("event sequence is not strictly increasing")]
    NonIncreasingSequence,
    #[error("logical turn is not strictly increasing")]
    NonIncreasingLogicalTurn,
    #[error("revision changed the stable thesis id")]
    UnstableThesisId,
    #[error("revision digest is not immutable canonical identity")]
    DigestMismatch,
    #[error("thesis projection contains too many lifecycles or digests")]
    TooManyLifecycles,
    #[error("thesis projection pack fingerprint is invalid")]
    InvalidPackFingerprint,
    #[error("thesis lifecycle map key differs from its embedded id")]
    LifecycleKeyMismatch,
    #[error("active thesis head is not present in projected digests: {0}")]
    UnprojectedHead(ThesisDigest),
}
