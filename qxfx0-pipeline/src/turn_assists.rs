//! Turn assists: clarification routing and shadow evidence recorders
//! (fact-grounded, doubt, anomaly). Called by the orchestrator in `lib.rs`.

use crate::execution_trace;
use crate::fact_grounded;
use crate::turn_types::{
    AnomalyShadowMode, ClarificationMode, DoubtShadowMode, SameTopicSuppressionMode,
};
use qxfx0_types::system_state::*;
use std::collections::BTreeMap;
use std::time::Duration;

pub(crate) fn record_fact_grounded_trace(
    trace: &mut execution_trace::PipelineTrace,
    rollout: fact_grounded::FactGroundedRollout,
    state: &SystemState,
    blocked: bool,
    receipt: Option<&fact_grounded::RenderedPlanReceipt>,
    outcome: &Result<
        Option<fact_grounded::FactGroundedFinalize>,
        fact_grounded::FactGroundedCompositionError,
    >,
) {
    let input_digest = execution_trace::calculate_stable_digest(&(
        rollout,
        blocked,
        receipt.map(fact_grounded::RenderedPlanReceipt::response_digest),
    ))
    .unwrap_or_else(|error| format!("digest-error:{error}"));
    let mut metadata = BTreeMap::from([
        ("rollout".into(), format!("{rollout:?}").to_lowercase()),
        ("blocked".into(), blocked.to_string()),
        ("receipt_present".into(), receipt.is_some().to_string()),
    ]);
    if let Some(receipt) = receipt {
        metadata.insert(
            "topic".into(),
            receipt.binding().stance_topic().as_str().into(),
        );
        metadata.insert(
            "concept_id".into(),
            receipt.binding().concept_id().0.clone(),
        );
        metadata.insert(
            "thesis_fact_id".into(),
            receipt.binding().thesis_fact_id().0.clone(),
        );
    }
    let status = match outcome {
        Ok(Some(fact_grounded::FactGroundedFinalize::Applied(update))) => {
            metadata.insert("episodes_added".into(), update.episodes_added.to_string());
            "applied"
        }
        Ok(Some(fact_grounded::FactGroundedFinalize::Observed { claim_count, .. })) => {
            metadata.insert("claim_count".into(), claim_count.to_string());
            "observed"
        }
        Ok(Some(fact_grounded::FactGroundedFinalize::Skipped(_))) => "skipped",
        Ok(None) if blocked => "blocked",
        Ok(None) => "no_audited_plan_receipt",
        Err(error) => {
            metadata.insert("error".into(), error.to_string());
            "rejected"
        }
    };
    metadata.insert("status".into(), status.into());
    let output_digest = execution_trace::calculate_stable_digest(&(state, &metadata))
        .unwrap_or_else(|error| format!("digest-error:{error}"));
    trace.record_step(
        "fact_grounded_finalize",
        input_digest,
        output_digest,
        Duration::ZERO,
        metadata,
    );
}

/// Record pure doubt/episodic evidence after topic normalization. The local
/// store is constructed from an already-persisted confirmed decision only;
/// it is neither retained nor applied to routing.
pub(crate) fn record_doubt_shadow(
    trace: &mut execution_trace::PipelineTrace,
    doubt_shadow: DoubtShadowMode,
    state: &SystemState,
    proposition: &qxfx0_semantic::ParsedProposition,
) {
    let input_digest = execution_trace::calculate_stable_digest(&(state, proposition))
        .unwrap_or_else(|error| format!("digest-error:{error}"));
    let config = qxfx0_self::doubt::EpisodicConfig::default();
    let mut metadata = BTreeMap::from([
        (
            "doubt_shadow_enabled".into(),
            doubt_shadow.enabled().to_string(),
        ),
        ("doubt_recall_count".into(), "0".into()),
        (
            "doubt_episodic_capacity".into(),
            config.capacity.to_string(),
        ),
        ("doubt_recall_limit".into(), config.recall_limit.to_string()),
    ]);

    if doubt_shadow.enabled() {
        let mut store = qxfx0_self::doubt::BoundedEpisodicStore::new(config);
        let confirmed_previous_decision =
            state.last_turn_decision.as_ref().is_some_and(|decision| {
                matches!(
                    decision.guard_status,
                    GuardStatus::Allowed | GuardStatus::InvariantWarn(_)
                )
            });
        if confirmed_previous_decision {
            if let Some(topic) = state.dialogue.last_topic.clone() {
                store = store.record(qxfx0_types::EpisodicEvent {
                    id: state.dialogue.turn_count as u64,
                    turn: state.dialogue.turn_count as u64,
                    kind: qxfx0_types::EpisodicKind::SystemDecision,
                    topic: Some(topic),
                });
            }
        }

        let driver = qxfx0_types::DoubtDriver::Other;
        let score = qxfx0_self::doubt::compute_doubt(qxfx0_types::DoubtInput {
            confidence: state.semantic.field.confidence,
            driver,
        });
        let recalled = store.recall(state.dialogue.turn_count as u64, Some(&proposition.subject));
        let proposed = qxfx0_self::doubt::route_for_doubt(
            score,
            qxfx0_self::doubt::DoubtPolicy::default(),
            &recalled,
        );
        metadata.extend([
            ("doubt_score".into(), score.value().to_string()),
            ("doubt_driver".into(), format!("{driver:?}").to_lowercase()),
            ("doubt_recall_count".into(), recalled.len().to_string()),
            (
                "doubt_proposed_route".into(),
                doubt_route_name(proposed).into(),
            ),
            ("doubt_reason".into(), "observation_only".into()),
        ]);
    } else {
        metadata.extend([
            ("doubt_score".into(), "not_evaluated".into()),
            ("doubt_driver".into(), "not_evaluated".into()),
            ("doubt_proposed_route".into(), "not_evaluated".into()),
            ("doubt_reason".into(), "disabled".into()),
        ]);
    }

    let output_digest = execution_trace::calculate_stable_digest(&metadata)
        .unwrap_or_else(|error| format!("digest-error:{error}"));
    trace.record_step(
        "doubt_shadow",
        input_digest,
        output_digest,
        Duration::ZERO,
        metadata,
    );
}

const ANOMALY_SHADOW_LEDGER_CAPACITY: usize = 1;

/// Record a typed recovery proposal after normalization without applying it.
///
/// Temporal evidence compares persisted typed system decisions with one local,
/// explicit affirmed candidate for the current turn. The candidate is never
/// retained here; this remains a trace-only recovery proposal.
pub(crate) fn record_anomaly_shadow(
    trace: &mut execution_trace::PipelineTrace,
    anomaly_shadow: AnomalyShadowMode,
    state: &SystemState,
    proposition: &qxfx0_semantic::ParsedProposition,
    is_challenge: bool,
    subject_authority: crate::SubjectAuthority,
) {
    let input_digest = execution_trace::calculate_stable_digest(&(state, proposition))
        .unwrap_or_else(|error| format!("digest-error:{error}"));
    let mut metadata = BTreeMap::from([
        (
            "anomaly_shadow_enabled".into(),
            anomaly_shadow.enabled().to_string(),
        ),
        (
            "anomaly_ledger_capacity".into(),
            ANOMALY_SHADOW_LEDGER_CAPACITY.to_string(),
        ),
        ("anomaly_temporal_evidence".into(), "disabled".into()),
        ("anomaly_temporal_history_count".into(), "0".into()),
    ]);

    if anomaly_shadow.enabled() {
        let mut ledger =
            qxfx0_self::anomaly::AnomalyRecoveryLedger::new(ANOMALY_SHADOW_LEDGER_CAPACITY);
        let observed_turn = state.dialogue.turn_count.saturating_add(1);
        // ADR-0044 M3: the anomaly evidence reads the live layer.
        let view = crate::essence_view::essence_view(state, subject_authority);
        let self_reference = qxfx0_self::anomaly::AnomalyEvidence::SelfReference {
            turn: observed_turn,
            subject: proposition.subject.clone(),
            angst: view.angst,
            witness_count: view.witness_count,
        };
        let anti_conatus = qxfx0_self::anomaly::AnomalyEvidence::AntiConatus {
            turn: observed_turn,
            stance_confidence: state.semantic.field.confidence,
            stance_consistent: !is_challenge,
            angst: view.angst,
            conatus: view.last_conatus,
        };
        let temporal = qxfx0_types::stance::StanceTopic::new(proposition.subject.clone())
            .ok()
            .and_then(|topic| {
                let current = qxfx0_types::stance::StanceObservation {
                    turn: observed_turn,
                    topic,
                    polarity: qxfx0_types::stance::StancePolarity::Affirmed,
                    source: qxfx0_types::stance::StanceSource::SystemDecision,
                };
                qxfx0_types::stance::detect_temporal_contradiction(
                    &state.semantic.stance_provenance,
                    &current,
                )
            });
        metadata.extend([
            (
                "anomaly_temporal_evidence".into(),
                "typed_persisted_provenance".into(),
            ),
            (
                "anomaly_temporal_history_count".into(),
                state.semantic.stance_provenance.len().to_string(),
            ),
        ]);
        let decision = qxfx0_self::anomaly::detect_anomaly(self_reference)
            .or_else(|| qxfx0_self::anomaly::detect_anomaly(anti_conatus))
            .or_else(|| {
                temporal.and_then(|contradiction| {
                    qxfx0_self::anomaly::detect_anomaly(contradiction.to_anomaly_evidence())
                })
            });

        if let Some(decision) = decision {
            let outcome = ledger.record(decision, input_digest.clone());
            let (replay_outcome, recovery) = match outcome {
                qxfx0_self::anomaly::AnomalyReplayOutcome::Proposed(trace) => ("proposed", trace),
                qxfx0_self::anomaly::AnomalyReplayOutcome::NoStateTransition(trace) => {
                    ("no_state_transition", trace)
                }
            };
            metadata.extend([
                (
                    "anomaly_proposed_kind".into(),
                    anomaly_kind_name(recovery.kind).into(),
                ),
                (
                    "anomaly_strategy".into(),
                    anomaly_strategy_name(recovery.strategy).into(),
                ),
                (
                    "anomaly_result".into(),
                    anomaly_result_name(recovery.result).into(),
                ),
                ("anomaly_idempotency_key".into(), recovery.idempotency_key),
                ("anomaly_replay_outcome".into(), replay_outcome.into()),
                ("anomaly_ledger_len".into(), ledger.len().to_string()),
                ("anomaly_reason".into(), "observation_only".into()),
            ]);
        } else {
            metadata.extend([
                ("anomaly_proposed_kind".into(), "not_detected".into()),
                ("anomaly_strategy".into(), "not_applicable".into()),
                ("anomaly_result".into(), "not_applicable".into()),
                ("anomaly_idempotency_key".into(), "not_applicable".into()),
                ("anomaly_replay_outcome".into(), "not_applicable".into()),
                ("anomaly_ledger_len".into(), ledger.len().to_string()),
                ("anomaly_reason".into(), "no_admitted_evidence".into()),
            ]);
        }
    } else {
        metadata.extend([
            ("anomaly_proposed_kind".into(), "not_evaluated".into()),
            ("anomaly_strategy".into(), "not_evaluated".into()),
            ("anomaly_result".into(), "not_evaluated".into()),
            ("anomaly_idempotency_key".into(), "not_evaluated".into()),
            ("anomaly_replay_outcome".into(), "not_evaluated".into()),
            ("anomaly_ledger_len".into(), "0".into()),
            ("anomaly_reason".into(), "disabled".into()),
        ]);
    }

    let output_digest = execution_trace::calculate_stable_digest(&metadata)
        .unwrap_or_else(|error| format!("digest-error:{error}"));
    trace.record_step(
        "anomaly_shadow",
        input_digest,
        output_digest,
        Duration::ZERO,
        metadata,
    );
}

const fn anomaly_kind_name(kind: qxfx0_self::anomaly::AnomalyKind) -> &'static str {
    match kind {
        qxfx0_self::anomaly::AnomalyKind::SelfReferentialCollapse => "self_referential_collapse",
        qxfx0_self::anomaly::AnomalyKind::Temporal => "temporal",
        qxfx0_self::anomaly::AnomalyKind::Unclassifiable => "unclassifiable",
        qxfx0_self::anomaly::AnomalyKind::AntiConatus => "anti_conatus",
    }
}

const fn anomaly_strategy_name(
    strategy: qxfx0_self::anomaly::AnomalyRecoveryStrategy,
) -> &'static str {
    match strategy {
        qxfx0_self::anomaly::AnomalyRecoveryStrategy::ResetEssence => "reset_essence",
        qxfx0_self::anomaly::AnomalyRecoveryStrategy::RestrictRoute => "restrict_route",
        qxfx0_self::anomaly::AnomalyRecoveryStrategy::RequestRevision => "request_revision",
    }
}

const fn anomaly_result_name(result: qxfx0_self::anomaly::AnomalyRecoveryResult) -> &'static str {
    match result {
        qxfx0_self::anomaly::AnomalyRecoveryResult::EssenceReset => "essence_reset",
        qxfx0_self::anomaly::AnomalyRecoveryResult::RouteRestricted => "route_restricted",
        qxfx0_self::anomaly::AnomalyRecoveryResult::RevisionRequested => "revision_requested",
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ClarificationDecision {
    pub(crate) proposed: Option<qxfx0_types::DoubtRoute>,
    pub(crate) score: Option<qxfx0_types::DoubtScore>,
    pub(crate) applied: bool,
    pub(crate) suppression_eligible: bool,
    pub(crate) suppression_applied: bool,
    pub(crate) recall_count: usize,
}

pub(crate) fn clarification_decision(
    clarification: ClarificationMode,
    suppression: SameTopicSuppressionMode,
    state: &SystemState,
    proposition: &qxfx0_semantic::ParsedProposition,
) -> ClarificationDecision {
    if !clarification.observes() || !clarification_mode_is_eligible(proposition.mode) {
        return ClarificationDecision {
            proposed: None,
            score: None,
            applied: false,
            suppression_eligible: false,
            suppression_applied: false,
            recall_count: 0,
        };
    }
    let score = qxfx0_self::doubt::compute_doubt(qxfx0_types::DoubtInput {
        confidence: state.semantic.field.confidence,
        driver: qxfx0_types::DoubtDriver::Other,
    });
    let proposed =
        qxfx0_self::doubt::route_for_doubt(score, qxfx0_self::doubt::DoubtPolicy::default(), &[]);
    let recalled = if suppression.observes() && proposed == qxfx0_types::DoubtRoute::Clarify {
        immediate_confirmed_same_topic(state, &proposition.subject)
    } else {
        Vec::new()
    };
    let suppression_route = qxfx0_self::doubt::route_for_doubt(
        score,
        qxfx0_self::doubt::DoubtPolicy::default(),
        &recalled,
    );
    let suppression_eligible =
        suppression_route == qxfx0_types::DoubtRoute::SuppressedByRecentDecision;
    let suppression_applied =
        clarification.applies() && suppression.applies() && suppression_eligible;
    ClarificationDecision {
        proposed: Some(proposed),
        score: Some(score),
        applied: clarification.applies()
            && proposed == qxfx0_types::DoubtRoute::Clarify
            && !suppression_applied,
        suppression_eligible,
        suppression_applied,
        recall_count: recalled.len(),
    }
}

pub(crate) fn immediate_confirmed_same_topic(
    state: &SystemState,
    topic: &str,
) -> Vec<qxfx0_types::EpisodicEvent> {
    let confirmed = state.last_turn_decision.as_ref().is_some_and(|decision| {
        matches!(
            decision.guard_status,
            GuardStatus::Allowed | GuardStatus::InvariantWarn(_)
        )
    });
    let Some(previous_topic) = confirmed
        .then(|| state.dialogue.last_topic.clone())
        .flatten()
    else {
        return Vec::new();
    };
    let store =
        qxfx0_self::doubt::BoundedEpisodicStore::default().record(qxfx0_types::EpisodicEvent {
            id: state.dialogue.turn_count as u64,
            turn: state.dialogue.turn_count as u64,
            kind: qxfx0_types::EpisodicKind::SystemDecision,
            topic: Some(previous_topic),
        });
    store.recall(state.dialogue.turn_count as u64, Some(topic))
}

const fn clarification_mode_is_eligible(mode: qxfx0_semantic::PropositionMode) -> bool {
    matches!(
        mode,
        qxfx0_semantic::PropositionMode::Define
            | qxfx0_semantic::PropositionMode::Assert
            | qxfx0_semantic::PropositionMode::Connect
            | qxfx0_semantic::PropositionMode::Reflect
    )
}

pub(crate) fn record_clarification_route(
    trace: &mut execution_trace::PipelineTrace,
    clarification: ClarificationMode,
    state: &SystemState,
    proposition: &qxfx0_semantic::ParsedProposition,
    decision: ClarificationDecision,
) {
    let input_digest = execution_trace::calculate_stable_digest(&(state, proposition))
        .unwrap_or_else(|error| format!("digest-error:{error}"));
    let proposed = decision
        .proposed
        .map(doubt_route_name)
        .unwrap_or("not_evaluated");
    let metadata = BTreeMap::from([
        (
            "clarification_enabled".into(),
            clarification.observes().to_string(),
        ),
        (
            "clarification_mode".into(),
            match clarification {
                ClarificationMode::Disabled => "disabled",
                ClarificationMode::TraceOnly => "trace_only",
                ClarificationMode::LimitedEnabled => "limited_enabled",
            }
            .into(),
        ),
        (
            "clarification_score".into(),
            decision
                .score
                .map(|score| score.value().to_string())
                .unwrap_or_else(|| "not_evaluated".into()),
        ),
        ("clarification_proposed_route".into(), proposed.into()),
        ("clarification_applied".into(), decision.applied.to_string()),
        (
            "clarification_reason".into(),
            if decision.applied {
                "low_confidence"
            } else if clarification.observes() {
                "observation_only"
            } else {
                "disabled"
            }
            .into(),
        ),
    ]);
    let output_digest = execution_trace::calculate_stable_digest(&metadata)
        .unwrap_or_else(|error| format!("digest-error:{error}"));
    trace.record_step(
        "clarification_route",
        input_digest,
        output_digest,
        Duration::ZERO,
        metadata,
    );
}

pub(crate) fn record_same_topic_suppression(
    trace: &mut execution_trace::PipelineTrace,
    suppression: SameTopicSuppressionMode,
    state: &SystemState,
    proposition: &qxfx0_semantic::ParsedProposition,
    decision: ClarificationDecision,
) {
    let input_digest = execution_trace::calculate_stable_digest(&(state, proposition))
        .unwrap_or_else(|error| format!("digest-error:{error}"));
    let metadata = BTreeMap::from([
        (
            "same_topic_suppression_enabled".into(),
            suppression.observes().to_string(),
        ),
        (
            "same_topic_suppression_mode".into(),
            match suppression {
                SameTopicSuppressionMode::Disabled => "disabled",
                SameTopicSuppressionMode::TraceOnly => "trace_only",
                SameTopicSuppressionMode::LimitedEnabled => "limited_enabled",
            }
            .into(),
        ),
        (
            "same_topic_suppression_recall_count".into(),
            decision.recall_count.to_string(),
        ),
        (
            "same_topic_suppression_eligible".into(),
            decision.suppression_eligible.to_string(),
        ),
        (
            "same_topic_suppression_applied".into(),
            decision.suppression_applied.to_string(),
        ),
        (
            "same_topic_suppression_actual_route".into(),
            if decision.suppression_applied {
                "retain_current"
            } else {
                "unchanged"
            }
            .into(),
        ),
        (
            "same_topic_suppression_reason".into(),
            if decision.suppression_eligible {
                "immediate_confirmed_same_topic"
            } else if suppression.observes() {
                "not_eligible"
            } else {
                "disabled"
            }
            .into(),
        ),
    ]);
    let output_digest = execution_trace::calculate_stable_digest(&metadata)
        .unwrap_or_else(|error| format!("digest-error:{error}"));
    trace.record_step(
        "same_topic_suppression",
        input_digest,
        output_digest,
        Duration::ZERO,
        metadata,
    );
}

const fn doubt_route_name(route: qxfx0_types::DoubtRoute) -> &'static str {
    match route {
        qxfx0_types::DoubtRoute::RetainCurrent => "retain_current",
        qxfx0_types::DoubtRoute::Clarify => "clarify",
        qxfx0_types::DoubtRoute::SuppressedByRecentDecision => "suppressed_by_recent_decision",
    }
}
