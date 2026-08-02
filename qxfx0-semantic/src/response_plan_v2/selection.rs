//! Deterministic self-selection over certified candidate plans (ADR-0034 §8).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::candidate::CandidateResponsePlan;

pub const NUMERIC_SEMANTICS_VERSION: &str = "basis-points-half-away-v1";
pub const RANKING_VERSION: &str = "score-desc-merkle-asc-v1";

/// Fixed-point scalar with four decimal places. The representation, rounding
/// and invalid-input rule are bound by [`NUMERIC_SEMANTICS_VERSION`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BasisPoints(i32);

impl BasisPoints {
    pub const ZERO: Self = Self(0);
    pub const ONE: Self = Self(10_000);

    pub const fn from_raw(value: i32) -> Self {
        Self(value)
    }

    /// Quantize once at the floating-point/self boundary. Non-finite values
    /// fail closed to zero; ties are rounded away from zero.
    pub fn quantize(value: f64) -> Self {
        if !value.is_finite() {
            return Self::ZERO;
        }
        let scaled = value * 10_000.0;
        let rounded = if scaled >= 0.0 {
            (scaled + 0.5).floor()
        } else {
            (scaled - 0.5).ceil()
        };
        Self(rounded.clamp(i32::MIN as f64, i32::MAX as f64) as i32)
    }

    pub const fn raw(self) -> i32 {
        self.0
    }
}

/// Immutable fixed-point view of the self state used during selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelfSelectionContext {
    pub conatus: BasisPoints,
    pub salience: BasisPoints,
    pub doubt: BasisPoints,
}

impl SelfSelectionContext {
    pub fn quantize(conatus: f64, salience: f64, doubt: f64) -> Self {
        Self {
            conatus: BasisPoints::quantize(conatus.max(0.0)),
            salience: BasisPoints::quantize(clamp_unit(salience)),
            doubt: BasisPoints::quantize(clamp_unit(doubt)),
        }
    }
}

/// Candidate-local preferences. They are fixed-point before entering the
/// selector and cannot mutate the candidate after it has been certified.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateSelectionSignals {
    pub base_utility: BasisPoints,
    pub preferred_conatus: BasisPoints,
    pub preferred_salience: BasisPoints,
    /// `0..=10_000`: how strongly doubt should penalize this candidate.
    pub doubt_sensitivity: BasisPoints,
}

impl CandidateSelectionSignals {
    pub const fn neutral() -> Self {
        Self {
            base_utility: BasisPoints::ZERO,
            preferred_conatus: BasisPoints::ZERO,
            preferred_salience: BasisPoints::ZERO,
            doubt_sensitivity: BasisPoints::ZERO,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionPolicy {
    pub base_weight: i32,
    pub conatus_distance_weight: i32,
    pub salience_distance_weight: i32,
    pub doubt_weight: i32,
}

impl Default for SelectionPolicy {
    fn default() -> Self {
        Self {
            base_weight: 4,
            conatus_distance_weight: 1,
            salience_distance_weight: 1,
            doubt_weight: 2,
        }
    }
}

impl SelectionPolicy {
    pub fn digest(&self) -> String {
        let encoded = serde_json::to_vec(self).expect("selection policy serializes");
        domain_digest(b"qxfx0:self-selection-policy:v1", &encoded)
    }

    fn score(self, context: SelfSelectionContext, signals: CandidateSelectionSignals) -> i64 {
        let conatus_distance =
            (i128::from(context.conatus.raw()) - i128::from(signals.preferred_conatus.raw())).abs();
        let salience_distance = (i128::from(context.salience.raw())
            - i128::from(signals.preferred_salience.raw()))
        .abs();
        let doubt_penalty =
            i128::from(context.doubt.raw()) * i128::from(signals.doubt_sensitivity.raw()) / 10_000;

        let score = i128::from(signals.base_utility.raw()) * i128::from(self.base_weight)
            - conatus_distance * i128::from(self.conatus_distance_weight)
            - salience_distance * i128::from(self.salience_distance_weight)
            - doubt_penalty * i128::from(self.doubt_weight);
        score.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
    }
}

/// An immutable candidate plus the fixed-point signals used to rank it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionCandidate {
    plan: CandidateResponsePlan,
    merkle_root: String,
    signals: CandidateSelectionSignals,
}

impl SelectionCandidate {
    pub fn new(plan: CandidateResponsePlan, signals: CandidateSelectionSignals) -> Self {
        let merkle_root = plan.candidate_digest();
        Self {
            plan,
            merkle_root,
            signals,
        }
    }

    pub fn merkle_root(&self) -> &str {
        &self.merkle_root
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionReceipt {
    pub candidate_merkle_root: String,
    pub score: i64,
    pub context: SelfSelectionContext,
    pub policy_digest: String,
    pub ranking_version: String,
    pub numeric_semantics_version: String,
}

/// The selected plan is moved into this artifact. Selection observes the
/// candidate but exposes no mutation path back into it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedCandidate {
    plan: CandidateResponsePlan,
    receipt: SelectionReceipt,
}

impl SelectedCandidate {
    pub fn plan(&self) -> &CandidateResponsePlan {
        &self.plan
    }

    pub fn receipt(&self) -> &SelectionReceipt {
        &self.receipt
    }

    pub fn into_plan(self) -> CandidateResponsePlan {
        self.plan
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SelectionError {
    #[error("self-selection requires at least one candidate")]
    NoCandidates,
    #[error("candidate Merkle root changed after enumeration")]
    CandidateMutated,
}

/// Select by score descending and candidate Merkle root ascending.
pub fn select_candidate(
    mut candidates: Vec<SelectionCandidate>,
    context: SelfSelectionContext,
    policy: SelectionPolicy,
) -> Result<SelectedCandidate, SelectionError> {
    if candidates.is_empty() {
        return Err(SelectionError::NoCandidates);
    }
    if candidates
        .iter()
        .any(|candidate| candidate.plan.candidate_digest() != candidate.merkle_root)
    {
        return Err(SelectionError::CandidateMutated);
    }

    candidates.sort_by(|left, right| {
        policy
            .score(context, right.signals)
            .cmp(&policy.score(context, left.signals))
            .then_with(|| left.merkle_root.cmp(&right.merkle_root))
    });
    let selected = candidates.remove(0);
    Ok(SelectedCandidate {
        receipt: SelectionReceipt {
            candidate_merkle_root: selected.merkle_root,
            score: policy.score(context, selected.signals),
            context,
            policy_digest: policy.digest(),
            ranking_version: RANKING_VERSION.to_string(),
            numeric_semantics_version: NUMERIC_SEMANTICS_VERSION.to_string(),
        },
        plan: selected.plan,
    })
}

fn clamp_unit(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn domain_digest(domain: &[u8], payload: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update((payload.len() as u64).to_be_bytes());
    hasher.update(payload);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::response_plan_v2::build_audited_topic;

    #[derive(Deserialize)]
    struct NumericReferenceVectors {
        schema: String,
        numeric_semantics_version: String,
        vector_digest: String,
        vectors: Vec<NumericReferenceVector>,
    }

    #[derive(Deserialize, Serialize)]
    struct NumericReferenceVector {
        name: String,
        input_bits_hex: String,
        expected_raw: i32,
    }

    fn candidate(topic: &str, signals: CandidateSelectionSignals) -> SelectionCandidate {
        let plan = build_audited_topic(topic).expect("audited plan");
        SelectionCandidate::new(plan.authorized().certified().candidate().clone(), signals)
    }

    #[test]
    fn floating_point_is_quantized_once_with_bound_rounding() {
        assert_eq!(BasisPoints::quantize(0.12345).raw(), 1_235);
        assert_eq!(BasisPoints::quantize(-0.12345).raw(), -1_235);
        assert_eq!(BasisPoints::quantize(f64::NAN), BasisPoints::ZERO);
        assert_eq!(
            SelfSelectionContext::quantize(1.23456, 2.0, -1.0),
            SelfSelectionContext {
                conatus: BasisPoints::from_raw(12_346),
                salience: BasisPoints::ONE,
                doubt: BasisPoints::ZERO,
            }
        );
    }

    #[test]
    fn numeric_semantics_reference_vectors_are_cross_platform_stable() {
        let source = include_str!(
            "../../../docs/reference-vectors/response-plan-v2-numeric-semantics-v1.json"
        );
        let vectors: NumericReferenceVectors =
            serde_json::from_str(source).expect("numeric reference vectors parse");
        assert_eq!(
            vectors.schema,
            "qxfx0.response-plan-v2.numeric-semantics.v1"
        );
        assert_eq!(vectors.numeric_semantics_version, NUMERIC_SEMANTICS_VERSION);

        for vector in &vectors.vectors {
            let bits = u64::from_str_radix(&vector.input_bits_hex, 16)
                .unwrap_or_else(|_| panic!("{} has invalid IEEE-754 bits", vector.name));
            assert_eq!(
                BasisPoints::quantize(f64::from_bits(bits)).raw(),
                vector.expected_raw,
                "{}",
                vector.name
            );
        }
        let encoded = serde_json::to_vec(&vectors.vectors).expect("vectors serialize");
        assert_eq!(
            domain_digest(b"qxfx0:numeric-semantics-vectors:v1", &encoded),
            vectors.vector_digest
        );
    }

    #[test]
    fn score_saturates_instead_of_overflowing() {
        let policy = SelectionPolicy {
            base_weight: i32::MAX,
            conatus_distance_weight: i32::MIN,
            salience_distance_weight: i32::MIN,
            doubt_weight: i32::MIN,
        };
        let score = policy.score(
            SelfSelectionContext {
                conatus: BasisPoints::from_raw(i32::MAX),
                salience: BasisPoints::from_raw(i32::MAX),
                doubt: BasisPoints::ZERO,
            },
            CandidateSelectionSignals {
                base_utility: BasisPoints::from_raw(i32::MAX),
                preferred_conatus: BasisPoints::from_raw(i32::MIN),
                preferred_salience: BasisPoints::from_raw(i32::MIN),
                doubt_sensitivity: BasisPoints::ZERO,
            },
        );
        assert_eq!(score, i64::MAX);

        let score = SelectionPolicy {
            base_weight: i32::MIN,
            conatus_distance_weight: i32::MAX,
            salience_distance_weight: i32::MAX,
            doubt_weight: i32::MAX,
        }
        .score(
            SelfSelectionContext {
                conatus: BasisPoints::from_raw(i32::MAX),
                salience: BasisPoints::from_raw(i32::MAX),
                doubt: BasisPoints::from_raw(i32::MAX),
            },
            CandidateSelectionSignals {
                base_utility: BasisPoints::from_raw(i32::MAX),
                preferred_conatus: BasisPoints::from_raw(i32::MIN),
                preferred_salience: BasisPoints::from_raw(i32::MIN),
                doubt_sensitivity: BasisPoints::from_raw(i32::MAX),
            },
        );
        assert_eq!(score, i64::MIN);
    }

    #[test]
    fn score_precedes_merkle_tie_break() {
        let context = SelfSelectionContext::quantize(1.0, 0.5, 0.1);
        let low = CandidateSelectionSignals {
            base_utility: BasisPoints::from_raw(1_000),
            preferred_conatus: context.conatus,
            preferred_salience: context.salience,
            doubt_sensitivity: BasisPoints::ZERO,
        };
        let high = CandidateSelectionSignals {
            base_utility: BasisPoints::from_raw(2_000),
            ..low
        };
        let selected = select_candidate(
            vec![candidate("свобода", low), candidate("истина", high)],
            context,
            SelectionPolicy::default(),
        )
        .expect("selection");
        assert_eq!(
            selected.receipt().candidate_merkle_root,
            selected.plan().candidate_digest()
        );
        assert_eq!(selected.receipt().score, 8_000);
    }

    #[test]
    fn merkle_root_is_the_total_order_tie_break() {
        let context = SelfSelectionContext::quantize(1.0, 0.5, 0.1);
        let signals = CandidateSelectionSignals {
            preferred_conatus: context.conatus,
            preferred_salience: context.salience,
            ..CandidateSelectionSignals::neutral()
        };
        let left = candidate("свобода", signals);
        let right = candidate("истина", signals);
        let expected = left.merkle_root().min(right.merkle_root()).to_string();
        let selected = select_candidate(vec![right, left], context, SelectionPolicy::default())
            .expect("selection");
        assert_eq!(selected.receipt().candidate_merkle_root, expected);
    }
}
