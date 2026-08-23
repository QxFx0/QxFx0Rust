//! qxfx0-self-v2 — the canonical subject-core port (ADR-0043 U2).
//!
//! Two modules, ported phase-by-phase from the Haskell reference (its
//! fact-checked AGENTS.md is the readiness map) with V1 `qxfx0-self`
//! untouched:
//!
//! - [`conatus`]: the Phase-2 scalar functional over the self blanket —
//!   configurable weights, per-axis decomposition, analytic gradient,
//!   violation penalty, low-energy threshold.
//! - [`essence`]: the Phase-9/10 Σ-typed commitment trajectory — pure
//!   morphisms (witness / should_commit / extract_mode / commit /
//!   collapse), post-commitment plan validation, the replay-visible reset
//!   event, and the B2 Control-A ablation hook present from day one.
//!
//! Everything here is pure and total; the pipeline integration (Prepare/
//! Finalize hooks, persisted trace fields) is the next U2 step.

pub mod conatus;
pub mod essence;

pub use conatus::{
    compute_conatus_energy, compute_conatus_energy_with, compute_conatus_gradient,
    compute_conatus_gradient_with, conatus_violation_penalty, gradient_magnitude,
    gradient_normalize, BlanketViolation, ConatusComponents, ConatusEnergy, ConatusGradient,
    ConatusWeights, SelfBlanketSnapshot, BUILTIN_CONATUS_WEIGHTS, LOW_ENERGY_THRESHOLD,
};
pub use essence::{
    admissible_families, advance_essence, collapse_essence, collapse_essence_at, commit,
    empty_essence, empty_trajectory, extract_mode, field_signature, phase9_essence_modulation,
    render_commitment_trigger, render_essence_mode, render_essence_violation, should_commit,
    validate_invariants, validate_plan, witness, Band, CommitmentTrigger, Essence, EssenceAblation,
    EssenceAdvanceTrace, EssenceCommitment, EssenceMode, EssenceModulation, EssenceResetEvent,
    EssenceTrajectory, EssenceViolation, EssenceWitness, FieldSignature, ValenceBand,
};

#[cfg(test)]
mod tests {
    use super::essence::*;
    use super::*;
    use qxfx0_self::deliberation::{Agreement, DeliberationTrace, ReconcileRule, SalienceDriver};
    use qxfx0_types::field::Field;
    use qxfx0_types::CanonicalMoveFamily;

    fn field_mid() -> Field {
        Field::default()
    }

    fn trace(rule: ReconcileRule, agreement: Agreement, divergence: f64) -> DeliberationTrace {
        DeliberationTrace {
            salience_driver: SalienceDriver::DrivenBySalienceDefault,
            rule,
            agreement,
            divergence,
        }
    }

    fn healthy_conatus() -> crate::conatus::ConatusEnergy {
        // Production-shaped blanket (≈13.8): above the calibrated 7.0
        // structural floor, unlike the small doc example (20,5,10)≈4.5
        // which is healthy for the threshold doc but sub-floor here.
        compute_conatus_energy(
            crate::conatus::SelfBlanketSnapshot {
                morphology_total_size: 20_000,
                identity_claims_count: 100,
                turn_count: 500,
            },
            &[],
        )
    }

    fn eroded_conatus() -> crate::conatus::ConatusEnergy {
        compute_conatus_energy(
            crate::conatus::SelfBlanketSnapshot {
                morphology_total_size: 0,
                identity_claims_count: 0,
                turn_count: 0,
            },
            &[crate::conatus::BlanketViolation {
                code: "rupture".into(),
                detail: "test".into(),
            }],
        )
    }

    #[test]
    fn empty_trajectory_never_commits_and_extracts_contemplative() {
        let modulation = EssenceModulation::default();
        let trajectory = empty_trajectory();
        assert_eq!(should_commit(&modulation, &trajectory), None);
        assert_eq!(extract_mode(&trajectory), EssenceMode::Contemplative);
    }

    #[test]
    fn angst_accrues_on_advantage_divergence_and_decays_on_full_agreement() {
        let modulation = EssenceModulation::default();
        let mut trajectory = empty_trajectory();
        let accrue = trace(
            ReconcileRule::RuleHolisticAdvantage,
            Agreement::PartialAgreement,
            0.8,
        );
        for _ in 0..3 {
            witness(
                &modulation,
                1,
                healthy_conatus(),
                &field_mid(),
                &accrue,
                &mut trajectory,
            );
        }
        assert!((trajectory.angst_level - 0.15).abs() < 1e-12);
        // ConatusOverride never moves angst.
        witness(
            &modulation,
            4,
            healthy_conatus(),
            &field_mid(),
            &trace(
                ReconcileRule::RuleConatusOverride,
                Agreement::NoAgreement,
                0.9,
            ),
            &mut trajectory,
        );
        assert!((trajectory.angst_level - 0.15).abs() < 1e-12);
        // Advantage below the divergence floor holds.
        witness(
            &modulation,
            5,
            healthy_conatus(),
            &field_mid(),
            &trace(
                ReconcileRule::RuleFormalAdvantage,
                Agreement::NoAgreement,
                0.1,
            ),
            &mut trajectory,
        );
        assert!((trajectory.angst_level - 0.15).abs() < 1e-12);
        // Full agreement with zero divergence decays.
        witness(
            &modulation,
            6,
            healthy_conatus(),
            &field_mid(),
            &trace(ReconcileRule::RuleAgreement, Agreement::FullAgreement, 0.0),
            &mut trajectory,
        );
        assert!((trajectory.angst_level - 0.13).abs() < 1e-12);
        // Angst can never leave [0, 1].
        let mut saturated = empty_trajectory();
        for _ in 0..100 {
            witness(
                &modulation,
                1,
                healthy_conatus(),
                &field_mid(),
                &accrue,
                &mut saturated,
            );
        }
        assert_eq!(saturated.angst_level, 1.0);
    }

    #[test]
    fn angst_threshold_fires_commitment_with_priority_over_erosion() {
        let modulation = EssenceModulation::default();
        let mut trajectory = empty_trajectory();
        let accrue = trace(
            ReconcileRule::RuleHolisticAdvantage,
            Agreement::PartialAgreement,
            0.8,
        );
        for turn in 0..16 {
            witness(
                &modulation,
                turn,
                eroded_conatus(),
                &field_mid(),
                &accrue,
                &mut trajectory,
            );
        }
        // Angst hit 0.8 ≥ 0.75; erosion also holds (all scalars < 7.0).
        // Priority: angst wins.
        assert_eq!(
            should_commit(&modulation, &trajectory),
            Some(CommitmentTrigger::AngstThreshold)
        );
    }

    #[test]
    fn conatus_erosion_requires_a_full_sub_floor_window() {
        let modulation = EssenceModulation::default();
        let mut trajectory = empty_trajectory();
        let agreement = trace(ReconcileRule::RuleAgreement, Agreement::FullAgreement, 0.0);
        // 7 eroded witnesses (< window of 8): no trigger.
        for turn in 0..7 {
            witness(
                &modulation,
                turn,
                eroded_conatus(),
                &field_mid(),
                &agreement,
                &mut trajectory,
            );
        }
        assert_eq!(should_commit(&modulation, &trajectory), None);
        // One healthy witness splits the window: still no trigger.
        witness(
            &modulation,
            7,
            healthy_conatus(),
            &field_mid(),
            &agreement,
            &mut trajectory,
        );
        assert_eq!(should_commit(&modulation, &trajectory), None);
        // Fill the window past the healthy witness: 8 more eroded
        // witnesses push it out of the window entirely.
        for turn in 8..16 {
            witness(
                &modulation,
                turn,
                eroded_conatus(),
                &field_mid(),
                &agreement,
                &mut trajectory,
            );
        }
        assert_eq!(
            should_commit(&modulation, &trajectory),
            Some(CommitmentTrigger::ConatusErosion)
        );
    }

    #[test]
    fn extract_mode_tallies_and_ties_break_to_contemplative() {
        let modulation = EssenceModulation::default();
        let mut trajectory = empty_trajectory();
        let integrative = trace(ReconcileRule::RuleAgreement, Agreement::FullAgreement, 0.0);
        let dialogical = trace(
            ReconcileRule::RuleHolisticAdvantage,
            Agreement::PartialAgreement,
            0.6,
        );
        let contemplative = trace(
            ReconcileRule::RuleFormalAdvantage,
            Agreement::NoAgreement,
            0.6,
        );
        for turn in 0..2 {
            witness(
                &modulation,
                turn,
                healthy_conatus(),
                &field_mid(),
                &integrative,
                &mut trajectory,
            );
        }
        for turn in 2..3 {
            witness(
                &modulation,
                turn,
                healthy_conatus(),
                &field_mid(),
                &dialogical,
                &mut trajectory,
            );
        }
        assert_eq!(extract_mode(&trajectory), EssenceMode::Integrative);

        // Holistic-dominant trajectory extracts dialogical.
        let mut dialogic = empty_trajectory();
        for turn in 0..5 {
            witness(
                &modulation,
                turn,
                healthy_conatus(),
                &field_mid(),
                &dialogical,
                &mut dialogic,
            );
        }
        assert_eq!(extract_mode(&dialogic), EssenceMode::Dialogical);

        // Formal-dominant trajectory extracts contemplative.
        let mut contemplative_traj = empty_trajectory();
        for turn in 0..5 {
            witness(
                &modulation,
                turn,
                healthy_conatus(),
                &field_mid(),
                &contemplative,
                &mut contemplative_traj,
            );
        }
        assert_eq!(
            extract_mode(&contemplative_traj),
            EssenceMode::Contemplative
        );
    }

    #[test]
    fn commit_hash_is_stable_and_tamper_evident() {
        let modulation = EssenceModulation::default();
        let mut trajectory = empty_trajectory();
        let agreement = trace(ReconcileRule::RuleAgreement, Agreement::FullAgreement, 0.0);
        for turn in 0..4 {
            witness(
                &modulation,
                turn,
                healthy_conatus(),
                &field_mid(),
                &agreement,
                &mut trajectory,
            );
        }
        let first = commit(10, CommitmentTrigger::ConatusErosion, &trajectory);
        let second = commit(11, CommitmentTrigger::AngstThreshold, &trajectory);
        // Same witnesses → same hash regardless of trigger/turn.
        assert_eq!(first.witness_hash, second.witness_hash);
        assert!(first.witness_hash.starts_with("sha256:"));
        assert_eq!(first.witness_hash.len(), "sha256:".len() + 64);
        // Any tampering changes the hash.
        let mut tampered = trajectory.clone();
        tampered.witnesses[0].divergence += 0.5;
        let third = commit(10, CommitmentTrigger::ConatusErosion, &tampered);
        assert_ne!(first.witness_hash, third.witness_hash);
    }

    #[test]
    fn capacity_trims_the_oldest_witnesses() {
        let modulation = EssenceModulation::default();
        let mut trajectory = empty_trajectory();
        let agreement = trace(ReconcileRule::RuleAgreement, Agreement::FullAgreement, 0.0);
        for turn in 0..50 {
            witness(
                &modulation,
                turn,
                healthy_conatus(),
                &field_mid(),
                &agreement,
                &mut trajectory,
            );
        }
        assert_eq!(trajectory.witnesses.len(), modulation.trajectory_capacity);
        assert_eq!(
            trajectory
                .witnesses
                .front()
                .expect("non-empty")
                .turn_ordinal,
            50 - modulation.trajectory_capacity
        );
    }

    #[test]
    fn collapse_is_total_and_replay_visible() {
        let modulation = EssenceModulation::default();
        let mut trajectory = empty_trajectory();
        let accrue = trace(
            ReconcileRule::RuleHolisticAdvantage,
            Agreement::PartialAgreement,
            0.8,
        );
        for turn in 0..5 {
            witness(
                &modulation,
                turn,
                eroded_conatus(),
                &field_mid(),
                &accrue,
                &mut trajectory,
            );
        }
        let commitment = commit(5, CommitmentTrigger::AngstThreshold, &trajectory);
        let committed = Essence::Committed(trajectory, commitment);
        let (reset, event) = collapse_essence_at(6, committed);
        assert_eq!(event.turn, 6);
        assert_eq!(event.previous_witness_count, 5);
        assert!((event.previous_angst - 0.25).abs() < 1e-12);
        let Essence::Uncommitted(reset_trajectory) = reset else {
            panic!("collapse must always yield Uncommitted");
        };
        assert!(reset_trajectory.witnesses.is_empty());
        assert_eq!(reset_trajectory.angst_level, 0.0);
        assert_eq!(reset_trajectory.conatus_floor, 1.0);
    }

    #[test]
    fn committed_plans_validate_against_admissible_families() {
        let commitment = EssenceCommitment {
            mode: EssenceMode::Contemplative,
            trigger: CommitmentTrigger::ConatusErosion,
            committed_at: 9,
            witness_hash: "sha256:test".into(),
        };
        assert_eq!(
            validate_plan(&commitment, CanonicalMoveFamily::CMRepair),
            Ok(())
        );
        assert_eq!(
            validate_plan(&commitment, CanonicalMoveFamily::CMHypothesis),
            Ok(())
        );
        let violation = validate_plan(&commitment, CanonicalMoveFamily::CMConfront)
            .expect_err("confront is not admissible for contemplative");
        assert!(render_essence_violation(&violation).starts_with("family_mismatch:contemplative:"));
    }

    #[test]
    fn b2_ablation_suppresses_commit_but_not_witnessing() {
        let modulation = EssenceModulation::default();
        let accrue = trace(
            ReconcileRule::RuleHolisticAdvantage,
            Agreement::PartialAgreement,
            0.8,
        );
        let mut essence = empty_essence();
        for turn in 0..16 {
            let summary = advance_essence(
                &modulation,
                EssenceAblation::CommitDisabled,
                EssenceTurnInput {
                    turn_ordinal: turn,
                    conatus: eroded_conatus(),
                    field: &field_mid(),
                    trace: &accrue,
                    proposed_family: CanonicalMoveFamily::CMReflect,
                },
                &mut essence,
            );
            assert!(
                matches!(essence, Essence::Uncommitted(_)),
                "ablated arm must never commit"
            );
            if turn == 15 {
                assert_eq!(summary.trigger, Some(CommitmentTrigger::AngstThreshold));
                assert!(summary.ablated_commit_suppressed);
                let Essence::Uncommitted(trajectory) = &essence else {
                    unreachable!("checked above");
                };
                assert_eq!(trajectory.witnesses.len(), 16);
            }
        }
        // The control arm is matched on witnessing: same inputs through the
        // enabled arm DO commit at the same threshold.
        let mut enabled = empty_essence();
        for turn in 0..16 {
            advance_essence(
                &modulation,
                EssenceAblation::Enabled,
                EssenceTurnInput {
                    turn_ordinal: turn,
                    conatus: eroded_conatus(),
                    field: &field_mid(),
                    trace: &accrue,
                    proposed_family: CanonicalMoveFamily::CMReflect,
                },
                &mut enabled,
            );
        }
        assert!(matches!(enabled, Essence::Committed(_, _)));
    }

    #[test]
    fn advance_essence_validates_plans_after_commitment() {
        let modulation = EssenceModulation::default();
        let accrue = trace(
            ReconcileRule::RuleHolisticAdvantage,
            Agreement::PartialAgreement,
            0.8,
        );
        let mut essence = empty_essence();
        let mut committed_fired = false;
        for turn in 0..16 {
            let summary = advance_essence(
                &modulation,
                EssenceAblation::Enabled,
                EssenceTurnInput {
                    turn_ordinal: turn,
                    conatus: eroded_conatus(),
                    field: &field_mid(),
                    // CMReflect: admissible for Dialogical (the mode a
                    // holistic-advantage trajectory extracts), so no
                    // violation.
                    proposed_family: CanonicalMoveFamily::CMReflect,
                    trace: &accrue,
                },
                &mut essence,
            );
            if summary.committed.is_some() {
                committed_fired = true;
                assert_eq!(summary.violation, None);
            }
        }
        assert!(committed_fired, "the enabled arm must commit");
        assert!(matches!(essence, Essence::Committed(_, _)));
        // The very next turn with an inadmissible family surfaces a
        // violation — the post-commitment guard is reachable.
        let summary = advance_essence(
            &modulation,
            EssenceAblation::Enabled,
            EssenceTurnInput {
                turn_ordinal: 16,
                conatus: healthy_conatus(),
                field: &field_mid(),
                trace: &accrue,
                proposed_family: CanonicalMoveFamily::CMDistinguish,
            },
            &mut essence,
        );
        assert!(summary.violation.is_some());
    }

    #[test]
    fn invariants_hold_on_the_defaults() {
        assert!(validate_invariants().is_empty());
    }
}
