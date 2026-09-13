//! qxfx0-self-v2 — the canonical subject-core port (ADR-0043 U2/U3).
//!
//! Five modules, ported phase-by-phase from the Haskell reference (its
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
//! - [`salience`]: the Phase-5 canonical controller verdict over
//!   Field × Conatus (ADR-0043 U3) — per-signal contributions, the
//!   uncontested Conatus gate, the dead-band hemisphere dispatch and the
//!   bounded post-commitment weight adaptation.
//! - [`blanket`]: the structural self-identity invariants (ADR-0043 U3) —
//!   session stability, morphology presence and turn / identity-claim
//!   monotonicity across the commit-time transition.
//! - [`deliberation`]: the Phase-8 canonical six-rule reconciliation of
//!   the hemispheric proposals (ADR-0043 U3) — the ladder meant to replace
//!   route's priority switching, plus the doubt-loop escalation keyed on
//!   the canonical Conatus gate. Shadow evidence until the flip.
//!
//! Everything here is pure and total; the pipeline integrates it in shadow
//! (Finalize advance, replay-visible trace fields, fail-closed decode).

pub mod blanket;
pub mod conatus;
pub mod deliberation;
pub mod essence;
pub mod salience;

pub use blanket::{check_blanket_transition, check_initial_blanket, BlanketRecord};
pub use deliberation::{
    classify_agreement, compute_divergence, deliberate_shadow, plans_equal_mod_confidence,
    proposal_pair_from_field, reconcile, render_agreement_v2, render_reconcile_rule_v2,
    v2_result_to_v1_deliberation, validate_deliberation_invariants, AgreementV2,
    DeliberationModulationV2, DeliberationShadowTrace, DeliberationV2, PlanV2, ReconcileRuleV2,
    BUILTIN_DELIBERATION_MODULATION, DOUBT_CLARIFICATION_THRESHOLD,
};

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
    EssenceTrajectory, EssenceTurnInput, EssenceViolation, EssenceWitness, FieldSignature,
    ValenceBand,
};
pub use salience::{
    adapt_salience_weights, compute_salience, compute_salience_builtin, compute_self_verdict,
    conatus_gate_fires, contributions, is_holistic_family, render_v2_salience_driver,
    salience_hemisphere, validate_salience_invariants, Hemisphere, SalienceContributions,
    SalienceVerdictV2, SalienceWeightsV2, SelfVerdictV2, V2SalienceDriver,
    BUILTIN_SALIENCE_WEIGHTS,
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
        let first = commit(10, CommitmentTrigger::ConatusErosion, None, &trajectory);
        let second = commit(11, CommitmentTrigger::AngstThreshold, None, &trajectory);
        // Same witnesses → same hash regardless of trigger/turn.
        assert_eq!(first.witness_hash, second.witness_hash);
        assert!(first.witness_hash.starts_with("sha256:"));
        assert_eq!(first.witness_hash.len(), "sha256:".len() + 64);
        // Any tampering changes the hash.
        let mut tampered = trajectory.clone();
        tampered.witnesses[0].divergence += 0.5;
        let third = commit(10, CommitmentTrigger::ConatusErosion, None, &tampered);
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
        let commitment = commit(5, CommitmentTrigger::AngstThreshold, None, &trajectory);
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
            topic: None,
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
                    topic: None,
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
                    topic: None,
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
                    topic: None,
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
                topic: None,
            },
            &mut essence,
        );
        assert!(summary.violation.is_some());
    }

    #[test]
    fn invariants_hold_on_the_defaults() {
        assert!(validate_invariants().is_empty());
    }

    fn commit_dialogical(modulation: &EssenceModulation) -> Essence {
        // Drives the enabled arm to a commitment with an admissible
        // family, mirroring advance_essence_validates_plans_after_commitment.
        let accrue = trace(
            ReconcileRule::RuleHolisticAdvantage,
            Agreement::PartialAgreement,
            0.8,
        );
        let mut essence = empty_essence();
        for turn in 0..16 {
            let summary = advance_essence(
                modulation,
                EssenceAblation::Enabled,
                EssenceTurnInput {
                    turn_ordinal: turn,
                    conatus: eroded_conatus(),
                    field: &field_mid(),
                    trace: &accrue,
                    proposed_family: CanonicalMoveFamily::CMReflect,
                    topic: None,
                },
                &mut essence,
            );
            if summary.committed.is_some() {
                assert!(matches!(essence, Essence::Committed(_, _)));
                return essence;
            }
        }
        panic!("the enabled arm must commit within 16 turns");
    }

    fn violating_summary(
        modulation: &EssenceModulation,
        essence: &mut Essence,
        turn: usize,
    ) -> EssenceAdvanceTrace {
        advance_essence(
            modulation,
            EssenceAblation::Enabled,
            EssenceTurnInput {
                turn_ordinal: turn,
                conatus: eroded_conatus(),
                field: &field_mid(),
                trace: &trace(
                    ReconcileRule::RuleHolisticAdvantage,
                    Agreement::PartialAgreement,
                    0.8,
                ),
                // CMDistinguish is inadmissible for the dialogical mode
                // the accrue trajectory extracts.
                proposed_family: CanonicalMoveFamily::CMDistinguish,
                topic: None,
            },
            essence,
        )
    }

    #[test]
    fn sustained_violations_release_the_commitment() {
        let modulation = EssenceModulation::default();
        let window = modulation.violation_release_window;
        assert!(window > 1, "test needs a multi-turn window");
        let mut essence = commit_dialogical(&modulation);
        for turn in 0..window {
            let summary = violating_summary(&modulation, &mut essence, 100 + turn);
            assert!(summary.violation.is_some(), "turn {turn} must violate");
            assert_eq!(
                summary.released_commitment,
                turn + 1 == window,
                "release fires exactly on the window-reaching turn"
            );
        }
        assert!(
            matches!(essence, Essence::Uncommitted(_)),
            "sustained counter-evidence must release the commitment"
        );
        let Essence::Uncommitted(trajectory) = &essence else {
            unreachable!("checked above");
        };
        // Witnesses survive the release; angst is halved below the
        // commitment threshold, so no immediate recommit follows.
        assert!(!trajectory.witnesses.is_empty());
        assert!(trajectory.angst_level < modulation.angst_commitment_threshold);
        assert_eq!(trajectory.consecutive_violations, 0);
    }

    #[test]
    fn admissible_turn_decays_the_violation_counter() {
        let modulation = EssenceModulation::default();
        let window = modulation.violation_release_window;
        let mut essence = commit_dialogical(&modulation);
        let accrue = trace(
            ReconcileRule::RuleHolisticAdvantage,
            Agreement::PartialAgreement,
            0.8,
        );
        for turn in 0..window - 1 {
            assert!(violating_summary(&modulation, &mut essence, turn)
                .violation
                .is_some());
        }
        // ADR-0044 tuning: one admissible turn decays the counter by
        // `violation_decay_step` instead of zeroing it (7 → 6 with
        // defaults), so a mostly-violating trajectory still drifts toward
        // release instead of starting over.
        advance_essence(
            &modulation,
            EssenceAblation::Enabled,
            EssenceTurnInput {
                turn_ordinal: window,
                conatus: eroded_conatus(),
                field: &field_mid(),
                trace: &accrue,
                proposed_family: CanonicalMoveFamily::CMReflect,
                topic: None,
            },
            &mut essence,
        );
        let first = violating_summary(&modulation, &mut essence, 200);
        assert!(first.violation.is_some());
        assert!(
            !first.released_commitment,
            "decayed counter (7) must not release yet"
        );
        let second = violating_summary(&modulation, &mut essence, 201);
        assert!(second.violation.is_some());
        assert!(
            second.released_commitment,
            "decayed counter must reach the window one violation sooner"
        );
        assert!(matches!(essence, Essence::Uncommitted(_)));
    }

    #[test]
    fn violations_count_only_on_the_commitment_topic() {
        fn scoped_advance(
            modulation: &EssenceModulation,
            essence: &mut Essence,
            turn: usize,
            family: CanonicalMoveFamily,
            topic: Option<&str>,
        ) -> EssenceAdvanceTrace {
            advance_essence(
                modulation,
                EssenceAblation::Enabled,
                EssenceTurnInput {
                    turn_ordinal: turn,
                    conatus: eroded_conatus(),
                    field: &field_mid(),
                    trace: &trace(
                        ReconcileRule::RuleHolisticAdvantage,
                        Agreement::PartialAgreement,
                        0.8,
                    ),
                    proposed_family: family,
                    topic,
                },
                essence,
            )
        }
        let modulation = EssenceModulation::default();
        let window = modulation.violation_release_window;
        // Commit on "память": CMReflect is admissible for the Dialogical
        // mode a holistic-advantage trajectory extracts.
        let mut essence = empty_essence();
        for turn in 0..16 {
            scoped_advance(
                &modulation,
                &mut essence,
                turn,
                CanonicalMoveFamily::CMReflect,
                Some("память"),
            );
        }
        let Essence::Committed(_, commitment) = &essence else {
            panic!("expected a commitment after 16 accruing turns");
        };
        assert_eq!(commitment.topic.as_deref(), Some("память"));
        // Seven violations on another topic are recorded but never
        // counted: the counter must not move.
        for turn in 0..window - 1 {
            let summary = scoped_advance(
                &modulation,
                &mut essence,
                100 + turn,
                CanonicalMoveFamily::CMDistinguish,
                Some("внимание"),
            );
            assert!(
                summary.violation.is_some(),
                "cross-topic violation recorded"
            );
            assert!(
                !summary.released_commitment,
                "cross-topic violations must never release"
            );
        }
        assert!(matches!(essence, Essence::Committed(_, _)));
        // Same-topic violations count from zero: a full window releases.
        for turn in 0..window {
            let summary = scoped_advance(
                &modulation,
                &mut essence,
                200 + turn,
                CanonicalMoveFamily::CMDistinguish,
                Some("память"),
            );
            assert!(summary.violation.is_some());
            assert_eq!(
                summary.released_commitment,
                turn == window - 1,
                "release exactly on the window's last same-topic violation"
            );
        }
        assert!(matches!(essence, Essence::Uncommitted(_)));
    }

    #[test]
    fn unscoped_commitment_counts_every_topic() {
        // Pre-tuning commitments (topic None) stay universal: a scoped
        // turn still moves the counter. Old snapshots behave as before.
        let modulation = EssenceModulation::default();
        let window = modulation.violation_release_window;
        let mut essence = commit_dialogical(&modulation);
        for turn in 0..window {
            let summary = advance_essence(
                &modulation,
                EssenceAblation::Enabled,
                EssenceTurnInput {
                    turn_ordinal: turn,
                    conatus: eroded_conatus(),
                    field: &field_mid(),
                    trace: &trace(
                        ReconcileRule::RuleHolisticAdvantage,
                        Agreement::PartialAgreement,
                        0.8,
                    ),
                    proposed_family: CanonicalMoveFamily::CMDistinguish,
                    topic: Some("любая тема"),
                },
                &mut essence,
            );
            assert!(summary.violation.is_some());
            assert_eq!(summary.released_commitment, turn == window - 1);
        }
        assert!(matches!(essence, Essence::Uncommitted(_)));
    }

    #[test]
    fn commitment_budget_suppresses_recommit_after_churn() {
        let modulation = EssenceModulation {
            max_lifetime_commits: 1,
            ..EssenceModulation::default()
        };
        let accrue = trace(
            ReconcileRule::RuleHolisticAdvantage,
            Agreement::PartialAgreement,
            0.8,
        );
        // Spend the single-commit budget, then release it through a full
        // window of violations.
        let mut essence = empty_essence();
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
                    topic: None,
                },
                &mut essence,
            );
        }
        assert!(matches!(essence, Essence::Committed(_, _)));
        for turn in 0..modulation.violation_release_window {
            advance_essence(
                &modulation,
                EssenceAblation::Enabled,
                EssenceTurnInput {
                    turn_ordinal: 100 + turn,
                    conatus: eroded_conatus(),
                    field: &field_mid(),
                    trace: &accrue,
                    proposed_family: CanonicalMoveFamily::CMDistinguish,
                    topic: None,
                },
                &mut essence,
            );
        }
        assert!(matches!(essence, Essence::Uncommitted(_)));
        // Re-accrue past the threshold: the crossing is recorded but the
        // commit is suppressed — the budget is spent.
        let mut suppressed = false;
        for turn in 0..16 {
            let summary = advance_essence(
                &modulation,
                EssenceAblation::Enabled,
                EssenceTurnInput {
                    turn_ordinal: 200 + turn,
                    conatus: eroded_conatus(),
                    field: &field_mid(),
                    trace: &accrue,
                    proposed_family: CanonicalMoveFamily::CMReflect,
                    topic: None,
                },
                &mut essence,
            );
            assert!(
                matches!(essence, Essence::Uncommitted(_)),
                "spent budget must never recommit"
            );
            suppressed = suppressed || summary.budget_suppressed;
        }
        assert!(suppressed, "the re-crossing must testify to suppression");
    }

    #[test]
    fn nan_conatus_leaves_the_floor_and_erosion_intact() {
        let modulation = EssenceModulation::default();
        let mut trajectory = empty_trajectory();
        let nan_energy = crate::conatus::ConatusEnergy {
            scalar: f64::NAN,
            components: crate::conatus::ConatusComponents {
                morphology: 0.0,
                identity: 0.0,
                turns: 0.0,
                penalty: 0.0,
                self_divergence: 0.0,
            },
        };
        let held = trace(
            ReconcileRule::RuleHolisticAdvantage,
            Agreement::PartialAgreement,
            0.8,
        );
        witness(
            &modulation,
            1,
            nan_energy,
            &field_mid(),
            &held,
            &mut trajectory,
        );
        assert_eq!(
            trajectory.conatus_floor, 1.0,
            "NaN must not poison the floor"
        );
        assert_eq!(trajectory.witnesses.len(), 1, "the turn still witnesses");
        // Erosion stays live: sub-floor scalars afterwards still count.
        assert!(should_commit(&modulation, &trajectory).is_none());
    }

    #[test]
    fn release_does_not_recommit_immediately() {
        let modulation = EssenceModulation::default();
        let window = modulation.violation_release_window;
        let mut essence = commit_dialogical(&modulation);
        for turn in 0..window {
            violating_summary(&modulation, &mut essence, turn);
        }
        assert!(matches!(essence, Essence::Uncommitted(_)));
        // Same violating family right after the release, but with healthy
        // conatus and full agreement: angst was halved below the threshold
        // and now decays further, so no new trigger fires.
        let summary = advance_essence(
            &modulation,
            EssenceAblation::Enabled,
            EssenceTurnInput {
                turn_ordinal: window,
                conatus: healthy_conatus(),
                field: &field_mid(),
                trace: &trace(ReconcileRule::RuleAgreement, Agreement::FullAgreement, 0.0),
                proposed_family: CanonicalMoveFamily::CMDistinguish,
                topic: None,
            },
            &mut essence,
        );
        assert_eq!(summary.trigger, None);
        assert!(summary.committed.is_none());
    }

    #[test]
    fn sustained_erosion_recommits_after_release() {
        // Documented, not forbidden: if the conatus evidence stays
        // collapsed, the erosion trigger refires right after a release and
        // the commitment reforms. Persistent conditions, persistent
        // commitment — the hysteresis deadband covers the angst path,
        // not a world that never recovers.
        let modulation = EssenceModulation::default();
        let window = modulation.violation_release_window;
        let mut essence = commit_dialogical(&modulation);
        for turn in 0..window {
            violating_summary(&modulation, &mut essence, turn);
        }
        assert!(matches!(essence, Essence::Uncommitted(_)));
        let summary = violating_summary(&modulation, &mut essence, window);
        assert_eq!(summary.trigger, Some(CommitmentTrigger::ConatusErosion));
        assert!(summary.committed.is_some());
    }
}
