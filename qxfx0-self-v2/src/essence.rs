//! Essence — the Σ-typed commitment trajectory (ADR-0043 U2, ported from
//! Haskell `QxFx0.Self.Essence`, Phases 9–10 / ADR-0012 there).
//!
//! The trajectory accumulates one [`EssenceWitness`] per turn from the
//! deliberation verdict; angst accrues on hemispheric divergence and decays
//! on full agreement; [`should_commit`] fires on the angst threshold or on
//! sustained conatus erosion (a true sliding window over the last
//! `conatus_floor_window` witnesses); [`commit`] then fixes an irrevocable
//! [`EssenceCommitment`] whose mode is extracted deterministically from the
//! trajectory, with a witness hash for replay tamper detection. After
//! commitment, [`validate_plan`] guards every plan against the mode's
//! admissible move families.
//!
//! Two deliberate law boundaries, mirroring the Haskell canon:
//! - Essence is unconditional where wired — no runtime feature flag can
//!   suppress commitment. The ONE intentional exception is the B2 Control-A
//!   ablation arm: [`EssenceAblation::CommitDisabled`] exists solely for
//!   the ablated control group and skips the commitment step while the
//!   trajectory still accumulates (the hook is present from day one so the
//!   control arm never needs retrofitted plumbing).
//! - Every runtime reset goes through [`collapse_essence_at`]: exactly one
//!   branch, and it always returns a replay-visible [`EssenceResetEvent`].
//!   A reset is either visible in the trace or it never happened.
//!
//! Tone/style admissibility from the Haskell `validatePlan` is deferred:
//! the Rust plan vocabulary carries family only, so V2 validates family
//! (plus refusal); tone/style land when the plan vocabulary grows.

use std::collections::VecDeque;

use qxfx0_self::deliberation::{Agreement, DeliberationTrace, ReconcileRule, SalienceDriver};
use qxfx0_types::field::Field;
use qxfx0_types::CanonicalMoveFamily;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::conatus::ConatusEnergy;

/// Operational essence mode. Never produced by [`extract_mode`] for an
/// empty trajectory's commitment — `Contemplative` is the tie-break.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EssenceMode {
    Witnessing,
    Contemplative,
    Dialogical,
    Integrative,
}

/// Snake_case JSON-schema-stable tag; any change is a breaking schema
/// change.
pub fn render_essence_mode(mode: EssenceMode) -> &'static str {
    match mode {
        EssenceMode::Witnessing => "witnessing",
        EssenceMode::Contemplative => "contemplative",
        EssenceMode::Dialogical => "dialogical",
        EssenceMode::Integrative => "integrative",
    }
}

/// What crossed the commitment threshold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommitmentTrigger {
    AngstThreshold,
    ConatusErosion,
}

/// Snake_case JSON-schema-stable tag.
pub fn render_commitment_trigger(trigger: CommitmentTrigger) -> &'static str {
    match trigger {
        CommitmentTrigger::AngstThreshold => "angst_threshold",
        CommitmentTrigger::ConatusErosion => "conatus_erosion",
    }
}

/// Coarse band of a unit-range field component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Band {
    Low,
    Mid,
    High,
}

/// Coarse band of a valence-range component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValenceBand {
    Negative,
    Neutral,
    Positive,
}

/// The banded snapshot of a turn's affective field, stored per witness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldSignature {
    pub resonance: Band,
    pub arousal: Band,
    pub valence: ValenceBand,
    pub consolidation: Band,
    pub counterfactual: Band,
}

/// Coarse-hash a `Field` into its signature under the modulation's band
/// edges. Components in [0,1] band at `band_low_edge`/`band_high_edge`;
/// valence in [-1,1] at `valence_low_edge`/`valence_high_edge`.
pub fn field_signature(modulation: &EssenceModulation, field: &Field) -> FieldSignature {
    let unit = |x: f64| {
        if x < modulation.band_low_edge {
            Band::Low
        } else if x > modulation.band_high_edge {
            Band::High
        } else {
            Band::Mid
        }
    };
    let valence = |x: f64| {
        if x < modulation.valence_low_edge {
            ValenceBand::Negative
        } else if x > modulation.valence_high_edge {
            ValenceBand::Positive
        } else {
            ValenceBand::Neutral
        }
    };
    FieldSignature {
        resonance: unit(field.resonance),
        arousal: unit(field.atmosphere.arousal),
        valence: valence(field.atmosphere.valence),
        consolidation: unit(field.consolidation),
        counterfactual: unit(field.counterfactual),
    }
}

/// One turn's deliberation record inside the trajectory.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EssenceWitness {
    pub turn_ordinal: usize,
    pub salience_driver: SalienceDriver,
    pub reconcile_rule: ReconcileRule,
    pub agreement: Agreement,
    pub divergence: f64,
    pub conatus_scalar: f64,
    pub field_signature: FieldSignature,
}

/// The bounded accumulator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EssenceTrajectory {
    pub witnesses: VecDeque<EssenceWitness>,
    pub angst_level: f64,
    pub conatus_floor: f64,
    pub capacity: usize,
    /// Consecutive plan-family violations against the live commitment.
    /// Reset by any admissible turn; reaching `violation_release_window`
    /// releases a stale commitment (hysteresis). Added post-U2; old
    /// snapshots load it as zero via the serde default.
    #[serde(default)]
    pub consecutive_violations: u32,
}

/// Tunables. Defaults follow the Haskell calibrated set (Phase 10 §4):
/// the conatus structural floor is 7.0, matching the log-scale codomain of
/// the V2 canonical conatus (healthy ≈ 4.5–15); angst-side parameters keep
/// their Phase 9 values.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EssenceModulation {
    pub angst_commitment_threshold: f64,
    pub angst_accrual_rate: f64,
    pub angst_decay_rate: f64,
    pub angst_accrual_divergence_floor: f64,
    pub conatus_floor_window: usize,
    pub conatus_structural_floor: f64,
    pub trajectory_capacity: usize,
    pub band_low_edge: f64,
    pub band_high_edge: f64,
    pub valence_low_edge: f64,
    pub valence_high_edge: f64,
    /// Sustained-violation hysteresis: a live commitment is released after
    /// this many consecutive plan-family violations, so a stale commitment
    /// cannot violate forever (commit-once-violate-everything). Must be
    /// positive; 8 matches the conatus window scale.
    #[serde(default = "default_violation_release_window")]
    pub violation_release_window: usize,
}

fn default_violation_release_window() -> usize {
    8
}

/// The original Phase 9 defaults, kept for regression locks (their
/// `conatus_structural_floor = 0.5` was a unit-mismatch error against the
/// log-scale conatus codomain — Haskell ADR-0012 §15.1).
pub fn phase9_essence_modulation() -> EssenceModulation {
    EssenceModulation {
        angst_commitment_threshold: 0.75,
        angst_accrual_rate: 0.05,
        angst_decay_rate: 0.02,
        angst_accrual_divergence_floor: 0.5,
        conatus_floor_window: 8,
        conatus_structural_floor: 0.5,
        trajectory_capacity: 32,
        band_low_edge: 0.33,
        band_high_edge: 0.67,
        valence_low_edge: -0.33,
        valence_high_edge: 0.33,
        violation_release_window: default_violation_release_window(),
    }
}

impl Default for EssenceModulation {
    fn default() -> Self {
        let mut calibrated = phase9_essence_modulation();
        calibrated.conatus_structural_floor = 7.0;
        calibrated
    }
}

/// The irrevocable fixation of a trajectory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EssenceCommitment {
    pub mode: EssenceMode,
    pub trigger: CommitmentTrigger,
    pub committed_at: usize,
    pub witness_hash: String,
}

/// The Σ-type: a trajectory is either uncommitted or committed (the
/// commitment is irrevocable; the trajectory keeps accumulating either
/// way).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Essence {
    Uncommitted(EssenceTrajectory),
    Committed(EssenceTrajectory, EssenceCommitment),
}

/// Replay-visible record of a self-referential collapse (Anomaly-3): what
/// was lost, not silent amnesia.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EssenceResetEvent {
    pub turn: usize,
    pub previous_angst: f64,
    pub previous_witness_count: usize,
}

/// A plan-validation violation. Priority: family first; tone/style checks
/// are deferred until the plan vocabulary carries them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EssenceViolation {
    FamilyMismatch {
        mode: EssenceMode,
        family: CanonicalMoveFamily,
    },
    RefusedCommitment {
        trigger: CommitmentTrigger,
    },
}

/// Snake_case JSON-schema-stable summary.
pub fn render_essence_violation(violation: &EssenceViolation) -> String {
    match violation {
        EssenceViolation::FamilyMismatch { mode, family } => format!(
            "family_mismatch:{}:{:?}",
            render_essence_mode(*mode),
            family
        ),
        EssenceViolation::RefusedCommitment { trigger } => {
            format!("refused_commitment:{}", render_commitment_trigger(*trigger))
        }
    }
}

/// The B2 Control-A ablation arm. `Enabled` is the law; `CommitDisabled`
/// exists solely for the ablated control group and skips the commitment
/// step while witnessing continues.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum EssenceAblation {
    #[default]
    Enabled,
    CommitDisabled,
}

/// The carrier-empty trajectory: no witnesses, zero angst, conatus floor
/// at 1.0 (the floor only descends with witnessed evidence).
pub fn empty_trajectory() -> EssenceTrajectory {
    EssenceTrajectory {
        witnesses: VecDeque::new(),
        angst_level: 0.0,
        conatus_floor: 1.0,
        capacity: EssenceModulation::default().trajectory_capacity,
        consecutive_violations: 0,
    }
}

/// The carrier-empty essence.
pub fn empty_essence() -> Essence {
    Essence::Uncommitted(empty_trajectory())
}

/// Ingest one turn's deliberation into the trajectory.
///
/// Angst dynamics: `RuleConatusOverride` never moves angst; full agreement
/// with zero divergence decays; a hemispheric advantage rule with
/// divergence at or above the accrual floor accrues; everything else
/// holds. Clamped to `[0, 1]`. The conatus floor descends to the running
/// minimum witnessed scalar (diagnostics; the erosion *trigger* is the
/// sliding window, not this floor).
pub fn witness(
    modulation: &EssenceModulation,
    turn_ordinal: usize,
    conatus: ConatusEnergy,
    field: &Field,
    trace: &DeliberationTrace,
    trajectory: &mut EssenceTrajectory,
) {
    if trajectory.capacity == 0 {
        trajectory.capacity = modulation.trajectory_capacity;
    }
    let record = EssenceWitness {
        turn_ordinal,
        salience_driver: trace.salience_driver,
        reconcile_rule: trace.rule,
        agreement: trace.agreement,
        divergence: trace.divergence,
        conatus_scalar: conatus.scalar,
        field_signature: field_signature(modulation, field),
    };
    trajectory.witnesses.push_back(record);
    while trajectory.witnesses.len() > trajectory.capacity {
        trajectory.witnesses.pop_front();
    }

    trajectory.angst_level = (match trace.rule {
        ReconcileRule::RuleConatusOverride => trajectory.angst_level,
        ReconcileRule::RuleAgreement if trace.divergence == 0.0 => {
            trajectory.angst_level - modulation.angst_decay_rate
        }
        ReconcileRule::RuleHolisticAdvantage
            if trace.divergence >= modulation.angst_accrual_divergence_floor =>
        {
            trajectory.angst_level + modulation.angst_accrual_rate
        }
        ReconcileRule::RuleFormalAdvantage
            if trace.divergence >= modulation.angst_accrual_divergence_floor =>
        {
            trajectory.angst_level + modulation.angst_accrual_rate
        }
        _ => trajectory.angst_level,
    })
    .clamp(0.0, 1.0);

    trajectory.conatus_floor = trajectory.conatus_floor.min(conatus.scalar);
}

/// `Some(trigger)` when the trajectory has crossed a commitment threshold.
/// Priority: angst over conatus erosion. Erosion requires a *full* sliding
/// window of witnesses whose conatus scalar is below the structural floor.
pub fn should_commit(
    modulation: &EssenceModulation,
    trajectory: &EssenceTrajectory,
) -> Option<CommitmentTrigger> {
    let angst_fires = trajectory.angst_level >= modulation.angst_commitment_threshold;
    let window = modulation.conatus_floor_window;
    let witness_count = trajectory.witnesses.len();
    let window_full = witness_count >= window;
    let all_sub_floor = window_full
        && trajectory
            .witnesses
            .iter()
            .rev()
            .take(window)
            .all(|w| w.conatus_scalar < modulation.conatus_structural_floor);
    match (angst_fires, all_sub_floor) {
        (true, _) => Some(CommitmentTrigger::AngstThreshold),
        (false, true) => Some(CommitmentTrigger::ConatusErosion),
        _ => None,
    }
}

/// Deterministic mode extraction. Never yields `Witnessing`. Rates are the
/// share of full agreement (integrative), holistic-advantage (dialogical)
/// and formal-advantage (contemplative) witnesses; the winner is the max,
/// with ties resolved toward contemplative (the formal hemisphere is the
/// safer default in the absence of signal).
pub fn extract_mode(trajectory: &EssenceTrajectory) -> EssenceMode {
    let n = trajectory.witnesses.len() as f64;
    if n == 0.0 {
        return EssenceMode::Contemplative;
    }
    let (mut full, mut holistic, mut formal) = (0usize, 0usize, 0usize);
    for witness in &trajectory.witnesses {
        match (witness.agreement, witness.reconcile_rule) {
            (Agreement::FullAgreement, _) => full += 1,
            (_, ReconcileRule::RuleHolisticAdvantage) => holistic += 1,
            (_, ReconcileRule::RuleFormalAdvantage) => formal += 1,
            _ => {}
        }
    }
    let candidates = [
        (EssenceMode::Integrative, full as f64 / n),
        (EssenceMode::Dialogical, holistic as f64 / n),
        (EssenceMode::Contemplative, formal as f64 / n),
    ];
    // Strictly-greater scan with contemplative listed last: on a tie the
    // earlier candidate does not displace the later, so ties fall to
    // contemplative, matching the Haskell maximumBy pick-semantics.
    let mut best = candidates[2];
    for candidate in &candidates {
        if candidate.1 > best.1 {
            best = *candidate;
        }
    }
    if best.1 > 0.0 {
        best.0
    } else {
        EssenceMode::Contemplative
    }
}

/// Construct the commitment. Total, but meaningful only when
/// [`should_commit`] fired. The witness hash is a structural SHA-256 over
/// the canonically serialized witness sequence — tamper detection across
/// replay, not a cryptographic guarantee.
pub fn commit(
    turn_ordinal: usize,
    trigger: CommitmentTrigger,
    trajectory: &EssenceTrajectory,
) -> EssenceCommitment {
    EssenceCommitment {
        mode: extract_mode(trajectory),
        trigger,
        committed_at: turn_ordinal,
        witness_hash: hash_witnesses(&trajectory.witnesses),
    }
}

/// Structural SHA-256 over the witness sequence, hex-prefixed. Canonical
/// JSON keeps the hash stable across builds (struct field order is fixed;
/// floats serialize deterministically via serde_json).
fn hash_witnesses(witnesses: &VecDeque<EssenceWitness>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(
        serde_json::to_vec(witnesses)
            .expect("essence witnesses serialize deterministically for the witness hash"),
    );
    format!("sha256:{:x}", hasher.finalize())
}

/// Self-referential collapse of a trajectory: clears witnesses, zeroes
/// angst, resets the conatus floor — and returns the replay-visible record
/// of what was lost.
pub fn collapse_essence(turn: usize, trajectory: &mut EssenceTrajectory) -> EssenceResetEvent {
    let event = EssenceResetEvent {
        turn,
        previous_angst: trajectory.angst_level,
        previous_witness_count: trajectory.witnesses.len(),
    };
    trajectory.witnesses.clear();
    trajectory.angst_level = 0.0;
    trajectory.conatus_floor = 1.0;
    event
}

/// The canonical Essence-level collapse entry point (single-branch rule).
/// Total over both constructors: the commitment, if any, is dropped — the
/// result is always `Uncommitted` — and the loss is always visible as a
/// returned [`EssenceResetEvent`]. Every runtime reset goes through here.
pub fn collapse_essence_at(turn: usize, essence: Essence) -> (Essence, EssenceResetEvent) {
    let mut trajectory = match essence {
        Essence::Uncommitted(trajectory) | Essence::Committed(trajectory, _) => trajectory,
    };
    let event = collapse_essence(turn, &mut trajectory);
    (Essence::Uncommitted(trajectory), event)
}

/// Admissible move families per committed mode. `CMRepair` is admissible
/// in every mode: recovery is orthogonal to essence.
pub fn admissible_families(mode: EssenceMode) -> &'static [CanonicalMoveFamily] {
    use CanonicalMoveFamily::*;
    const ALL: &[CanonicalMoveFamily] = &[
        CMDefine,
        CMDistinguish,
        CMGround,
        CMReflect,
        CMDescribe,
        CMPurpose,
        CMHypothesis,
        CMRepair,
        CMContact,
        CMConnect,
        CMConfront,
        CMDeepen,
        CMNextStep,
        CMClarify,
        CMAnchor,
    ];
    match mode {
        EssenceMode::Witnessing | EssenceMode::Integrative => ALL,
        EssenceMode::Contemplative => &[CMDescribe, CMHypothesis, CMPurpose, CMRepair],
        EssenceMode::Dialogical => &[CMContact, CMDeepen, CMRepair, CMReflect],
    }
}

/// Validate a plan family against a committed essence. Family mismatch is
/// the first and (for now) only admissibility check — tone/style land when
/// the plan vocabulary carries them.
pub fn validate_plan(
    commitment: &EssenceCommitment,
    family: CanonicalMoveFamily,
) -> Result<(), EssenceViolation> {
    if admissible_families(commitment.mode).contains(&family) {
        Ok(())
    } else {
        Err(EssenceViolation::FamilyMismatch {
            mode: commitment.mode,
            family,
        })
    }
}

/// The summary of one [`advance_essence`] step — the nullable trace fields
/// the pipeline records (replay-visible, observational).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct EssenceAdvanceTrace {
    /// The angst level after witnessing this turn.
    pub angst_level: f64,
    /// The conatus scalar witnessed this turn.
    pub conatus_scalar: f64,
    /// The trigger that fired this turn, if any.
    pub trigger: Option<CommitmentTrigger>,
    /// The commitment fixed this turn, if any.
    pub committed: Option<EssenceCommitment>,
    /// A plan-family violation against an existing commitment, if any.
    pub violation: Option<EssenceViolation>,
    /// True when the B2 ablation arm suppressed a would-be commitment.
    pub ablated_commit_suppressed: bool,
    /// True when sustained violations released the live commitment this
    /// turn (hysteresis). The violation itself is still recorded above.
    #[serde(default)]
    pub released_commitment: bool,
    /// ADR-0043 U3 shadow: the canonical V2 salience controller verdict over
    /// the turn's Field × Conatus (content saliency 0.0 until spectral
    /// clustering lands). Trace evidence only — it never feeds routing, the
    /// witness hash or any persisted field. Old snapshots load `None`.
    #[serde(default)]
    pub self_verdict: Option<crate::salience::SelfVerdictV2>,
    /// ADR-0043 U3: structural self-blanket violations detected across this
    /// turn's transition (session stability, morphology presence, turn and
    /// identity-claim monotonicity). Empty means the blanket held. The same
    /// list feeds the conatus violation penalty, so a nonzero count is
    /// witnessable as a lower `conatus_scalar` too — this field names the
    /// rupture instead of only its scalar shadow. Set by the pipeline after
    /// [`advance_essence`]; pre-U3 trace JSONs load it as empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blanket_violations: Vec<crate::conatus::BlanketViolation>,
}

/// Groups the per-turn inputs of [`advance_essence`] (argument-count
/// hygiene, the same pattern as V1's `WitnessInput`).
#[derive(Debug, Clone, Copy)]
pub struct EssenceTurnInput<'a> {
    pub turn_ordinal: usize,
    pub conatus: ConatusEnergy,
    pub field: &'a Field,
    pub trace: &'a DeliberationTrace,
    pub proposed_family: CanonicalMoveFamily,
}

/// The single integration point: witness this turn, then (unless ablated)
/// commit when a threshold fires, then validate the proposed plan family
/// against an existing commitment. Pure — the caller owns the `Essence`.
pub fn advance_essence(
    modulation: &EssenceModulation,
    ablation: EssenceAblation,
    input: EssenceTurnInput<'_>,
    essence: &mut Essence,
) -> EssenceAdvanceTrace {
    let EssenceTurnInput {
        turn_ordinal,
        conatus,
        field,
        trace,
        proposed_family,
    } = input;
    // Take the trajectory out to end the borrow on `essence`; the state
    // is repacked at the end.
    let (mut trajectory, existing_commitment) =
        match std::mem::replace(essence, Essence::Uncommitted(empty_trajectory())) {
            Essence::Uncommitted(trajectory) => (trajectory, None),
            Essence::Committed(trajectory, commitment) => (trajectory, Some(commitment)),
        };
    witness(
        modulation,
        turn_ordinal,
        conatus,
        field,
        trace,
        &mut trajectory,
    );

    let mut summary = EssenceAdvanceTrace {
        angst_level: trajectory.angst_level,
        conatus_scalar: conatus.scalar,
        self_verdict: Some(crate::salience::compute_self_verdict(
            crate::salience::SalienceWeightsV2::default(),
            conatus,
            field,
            0.0,
        )),
        ..EssenceAdvanceTrace::default()
    };

    let mut commitment = match existing_commitment {
        Some(commitment) => Some(commitment),
        None => match should_commit(modulation, &trajectory) {
            Some(trigger) => match ablation {
                EssenceAblation::Enabled => {
                    let fixed = commit(turn_ordinal, trigger, &trajectory);
                    summary.trigger = Some(trigger);
                    summary.committed = Some(fixed.clone());
                    Some(fixed)
                }
                EssenceAblation::CommitDisabled => {
                    summary.trigger = Some(trigger);
                    summary.ablated_commit_suppressed = true;
                    None
                }
            },
            None => None,
        },
    };

    if let Some(fixed) = &commitment {
        if let Err(violation) = validate_plan(fixed, proposed_family) {
            summary.violation = Some(violation);
            // Hysteresis: sustained counter-evidence releases a stale
            // commitment instead of violating forever. The release halves
            // angst (deadband below the commitment threshold, so no
            // immediate recommit) and keeps the witnesses; the violation
            // that triggered it stays visible in the trace.
            trajectory.consecutive_violations = trajectory.consecutive_violations.saturating_add(1);
            if trajectory.consecutive_violations >= modulation.violation_release_window as u32 {
                trajectory.consecutive_violations = 0;
                trajectory.angst_level *= 0.5;
                summary.released_commitment = true;
                commitment = None;
            }
        } else {
            trajectory.consecutive_violations = 0;
        }
    }

    *essence = match commitment {
        Some(fixed) => Essence::Committed(trajectory, fixed),
        None => Essence::Uncommitted(trajectory),
    };
    summary
}

/// Structural invariant checks for `doctor`: the default modulation must be
/// internally coherent, the morphisms must satisfy their laws on the empty
/// carrier, and the builtin conatus weights must be positive. Returns the
/// violations (empty = ok).
pub fn validate_invariants() -> Vec<String> {
    let mut violations = Vec::new();
    let modulation = EssenceModulation::default();
    let mut check_unit = |name: &str, value: f64| {
        if !(0.0..=1.0).contains(&value) || !value.is_finite() {
            violations.push(format!("essence-v2 default {name} out of (0,1]: {value}"));
        }
    };
    check_unit(
        "angst_commitment_threshold",
        modulation.angst_commitment_threshold,
    );
    check_unit("angst_accrual_rate", modulation.angst_accrual_rate);
    check_unit("angst_decay_rate", modulation.angst_decay_rate);
    if modulation.trajectory_capacity == 0 {
        violations.push("essence-v2 default trajectory_capacity must be positive".into());
    }
    if modulation.conatus_floor_window == 0 {
        violations.push("essence-v2 default conatus_floor_window must be positive".into());
    }
    if modulation.violation_release_window == 0 {
        violations.push("essence-v2 default violation_release_window must be positive".into());
    }
    if modulation.band_low_edge >= modulation.band_high_edge {
        violations.push("essence-v2 default band edges are not ordered".into());
    }
    if modulation.valence_low_edge >= modulation.valence_high_edge {
        violations.push("essence-v2 default valence edges are not ordered".into());
    }
    let weights = crate::conatus::ConatusWeights::default();
    if weights.morphology <= 0.0
        || weights.identity <= 0.0
        || weights.turns <= 0.0
        || weights.violation <= 0.0
    {
        violations.push("essence-v2 builtin conatus weights must be positive".into());
    }
    let empty = empty_trajectory();
    if should_commit(&modulation, &empty).is_some() {
        violations.push("essence-v2 empty trajectory must never commit".into());
    }
    if extract_mode(&empty) != EssenceMode::Contemplative {
        violations.push("essence-v2 empty trajectory must extract contemplative".into());
    }
    // ADR-0043 U3: the canonical salience controller rides the same doctor
    // check — its builtin weights are the shadow verdict's only tuning.
    violations.extend(crate::salience::validate_salience_invariants());
    violations
}
