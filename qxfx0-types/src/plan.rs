//! Plan identity and agreement primitives shared by V1 and V2 (ADR-0042).
//!
//! These four types are *law*, not stratum detail: every plan — the V1
//! response plan and the V2 certificate chain alike — addresses semantic
//! content through [`SemanticId`], carries at least one claim through
//! [`NonEmptyVec`], ranks evidence through basis-point [`Confidence`], and
//! labels discourse function through [`ClaimRole`]. They live in the shared
//! data-model crate so the V2 chain depends on `qxfx0-semantic` only for
//! the authority registries, never for plan vocabulary.
//!
//! Same-named identity types are deliberately NOT unified: V1's `ClaimId`
//! is a plan-local index, V2's `ClaimId` is a content-addressed certificate
//! digest; their construction semantics differ and merging them would let a
//! plan-local index masquerade as a content address.

use serde::{Deserialize, Serialize};

/// Stable identifier used by response-plan contracts. Semantic content is
/// carried by identifiers; surface strings remain in audited renderer assets.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SemanticId(String);

impl SemanticId {
    pub fn try_new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.trim().is_empty() {
            Err("semantic id must not be empty".into())
        } else {
            Ok(Self(value))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Structurally non-empty ordered collection used for claims and predicate
/// references. Empty ready plans cannot be represented through constructors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NonEmptyVec<T> {
    first: T,
    additional: Vec<T>,
}

impl<T> NonEmptyVec<T> {
    pub fn one(first: T) -> Self {
        Self {
            first,
            additional: Vec::new(),
        }
    }

    pub fn push(&mut self, value: T) {
        self.additional.push(value);
    }

    pub fn len(&self) -> usize {
        1 + self.additional.len()
    }

    pub fn is_empty(&self) -> bool {
        false
    }

    pub fn first(&self) -> &T {
        &self.first
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        std::iter::once(&self.first).chain(self.additional.iter())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimRole {
    Thesis,
    Support,
    Counterpoint,
    Consequence,
    DialogueAct,
}

impl ClaimRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Thesis => "thesis",
            Self::Support => "support",
            Self::Counterpoint => "counterpoint",
            Self::Consequence => "consequence",
            Self::DialogueAct => "dialogue_act",
        }
    }
}

/// Confidence represented as basis points to exclude NaN and platform-level
/// floating-point drift from replay-visible plans.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Confidence(u16);

impl Confidence {
    pub const MAX_BASIS_POINTS: u16 = 10_000;

    pub fn from_basis_points(value: u16) -> Result<Self, String> {
        if value <= Self::MAX_BASIS_POINTS {
            Ok(Self(value))
        } else {
            Err(format!(
                "confidence {value} exceeds {} basis points",
                Self::MAX_BASIS_POINTS
            ))
        }
    }

    pub fn basis_points(self) -> u16 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_ids_reject_whitespace_only_values() {
        assert!(SemanticId::try_new("  ").is_err());
        let id = SemanticId::try_new("freedom_choice").expect("valid id");
        assert_eq!(id.as_str(), "freedom_choice");
    }

    #[test]
    fn non_empty_vec_cannot_be_constructed_empty() {
        let mut claims = NonEmptyVec::one(1);
        claims.push(2);
        claims.push(3);
        assert_eq!(claims.len(), 3);
        assert!(!claims.is_empty());
        assert_eq!(claims.iter().copied().collect::<Vec<_>>(), vec![1, 2, 3]);
        assert_eq!(*claims.first(), 1);
    }

    #[test]
    fn claim_roles_have_stable_wire_names() {
        assert_eq!(ClaimRole::Thesis.as_str(), "thesis");
        assert_eq!(ClaimRole::Counterpoint.as_str(), "counterpoint");
        let encoded = serde_json::to_string(&ClaimRole::Consequence).expect("serializes");
        assert_eq!(encoded, r#""consequence""#);
    }

    #[test]
    fn confidence_is_bounded_basis_points() {
        assert!(Confidence::from_basis_points(10_001).is_err());
        let full = Confidence::from_basis_points(10_000).expect("bounded");
        assert_eq!(full.basis_points(), 10_000);
        let half = Confidence::from_basis_points(5_000).expect("bounded");
        assert!(half < full);
    }
}
