//! ResponsePlan V2 authority bookkeeping: receipts, digests, trace records.

use crate::execution_trace;
use crate::turn_context::RoutedTurnContext;
use crate::turn_types::response_plan_v2_canary_digest;
use crate::turn_types::{
    AuthorityDecisionReceipt, ResponsePlanV2Authority, ResponsePlanV2Mode,
    RESPONSE_PLAN_V2_CANARY_ALLOWLIST,
};
use qxfx0_types::*;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::time::Duration;

#[derive(Debug, Serialize)]
struct ResponsePlanV2Artifact {
    schema: &'static str,
    contract: qxfx0_plan_v2::TurnContractSnapshot,
    record: Option<qxfx0_plan_v2::TurnRecord>,
    result: qxfx0_plan_v2::V2ExecutionResult,
    realized: Option<qxfx0_plan_v2::RealizedSurface>,
    fallback: qxfx0_plan_v2::FallbackAction,
    authority_outcome: qxfx0_plan_v2::V2AuthorityOutcome,
}

/// SHA-256 of the running executable. The binary never changes while the
/// process lives, so the digest is computed once and cached; hashing
/// megabytes on every traced turn was pure per-turn I/O cost.
pub fn current_binary_digest() -> Result<String, String> {
    static BINARY_DIGEST: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    if let Some(cached) = BINARY_DIGEST.get() {
        return Ok(cached.clone());
    }
    let path = std::env::current_exe().map_err(|error| error.to_string())?;
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    // Failure stays uncached: an unreadable binary is reported on every use
    // instead of being remembered as a permanent condition.
    let digest = format!("{:x}", hasher.finalize());
    Ok(BINARY_DIGEST.get_or_init(|| digest).clone())
}

fn response_plan_v2_is_eligible(mode: ResponsePlanV2Mode, topic: &str) -> bool {
    match mode {
        ResponsePlanV2Mode::Off => false,
        ResponsePlanV2Mode::Shadow => true,
        ResponsePlanV2Mode::Canary => RESPONSE_PLAN_V2_CANARY_ALLOWLIST.contains(&topic),
        ResponsePlanV2Mode::AuditedAuthority => qxfx0_semantic::argued_topic_registry()
            .ok()
            .is_some_and(|registry| registry.get(topic).is_some()),
    }
}

pub(crate) fn record_response_plan_v2(
    mut trace: Option<&mut execution_trace::PipelineTrace>,
    routed: &RoutedTurnContext,
    logical_turn: u64,
    requested_mode: ResponsePlanV2Mode,
    authority: ResponsePlanV2Authority,
) -> Option<AuthorityDecisionReceipt> {
    use qxfx0_plan_v2::{
        execute_audited_topic_at, AssertionPolicy, AuthoritySnapshot, PlanningPolicySnapshot,
        RealizationSnapshot, SelectionPolicy, SelectionPolicySnapshot, SelfSelectionContext,
        TurnContractSnapshot, TurnRecord, V2BudgetPolicy,
    };

    let topic = routed.prepared().input().subject();
    let canary_eligible = RESPONSE_PLAN_V2_CANARY_ALLOWLIST.contains(&topic);
    let authority_intent_eligible = routed.family() == CanonicalMoveFamily::CMDefine;
    let eligible = response_plan_v2_is_eligible(requested_mode, topic)
        && (authority != ResponsePlanV2Authority::Canary || authority_intent_eligible);
    let effective_mode = if eligible {
        requested_mode
    } else {
        ResponsePlanV2Mode::Off
    };
    let scope_downgrade_count = usize::from(requested_mode != effective_mode);
    let downgrade_reason = if scope_downgrade_count == 1 {
        "topic_outside_rollout_scope"
    } else {
        "none"
    };
    let canary_digest = response_plan_v2_canary_digest();
    if effective_mode == ResponsePlanV2Mode::Off {
        let metadata = BTreeMap::from([
            ("requested_mode".into(), format!("{requested_mode:?}")),
            ("effective_mode".into(), "Off".into()),
            ("canary_eligible".into(), canary_eligible.to_string()),
            (
                "authority_intent_eligible".into(),
                authority_intent_eligible.to_string(),
            ),
            ("canary_digest".into(), canary_digest),
            ("attempted".into(), "false".into()),
            ("completed".into(), "false".into()),
            ("downgrade_count".into(), scope_downgrade_count.to_string()),
            ("downgrade_reason".into(), downgrade_reason.into()),
            ("semantic_parity".into(), "false".into()),
            ("authority_parity".into(), "false".into()),
            ("realization_parity".into(), "false".into()),
            ("replay_parity".into(), "false".into()),
            ("authority_outcome".into(), "not_attempted".into()),
            ("authority_outcome_digest".into(), "none".into()),
            ("authority_surface_digest".into(), "none".into()),
            ("claim_identity_digest".into(), "none".into()),
            ("fact_binding_digest".into(), "none".into()),
            ("claim_authority_digest".into(), "none".into()),
            ("v1_authoritative".into(), "true".into()),
            ("v1_fallback_used".into(), "false".into()),
        ]);
        let digest = execution_trace::calculate_stable_digest(&metadata)
            .unwrap_or_else(|error| format!("digest-error:{error}"));
        if let Some(trace) = trace.as_deref_mut() {
            trace.record_step(
                "response_plan_v2",
                digest.clone(),
                digest,
                Duration::ZERO,
                metadata,
            );
        }
        return None;
    }

    let policy = SelectionPolicy {
        response_plan_v2_mode: requested_mode,
        ..SelectionPolicy::default()
    };
    let budgets = V2BudgetPolicy::default();
    let contract = TurnContractSnapshot::new(
        AuthoritySnapshot::new(
            qxfx0_semantic::active_pack_set().fingerprint(),
            AssertionPolicy::v1().digest(),
        ),
        PlanningPolicySnapshot::new(budgets.digest(), "proposition-canon-v1"),
        RealizationSnapshot::new(
            qxfx0_plan_v2::valency_lexicon().fingerprint(),
            "clause-grammar-v1",
            qxfx0_morphology::get_runtime().lexemes_sha256(),
            qxfx0_plan_v2::preposition_allomorphs().fingerprint(),
        ),
        SelectionPolicySnapshot::new(policy),
    );
    if contract.verify_integrity().is_err() {
        let metadata = BTreeMap::from([
            ("requested_mode".into(), format!("{requested_mode:?}")),
            ("effective_mode".into(), "Off".into()),
            ("canary_eligible".into(), canary_eligible.to_string()),
            (
                "authority_intent_eligible".into(),
                authority_intent_eligible.to_string(),
            ),
            ("canary_digest".into(), canary_digest),
            ("attempted".into(), "true".into()),
            ("completed".into(), "false".into()),
            (
                "downgrade_count".into(),
                (scope_downgrade_count + 1).to_string(),
            ),
            ("downgrade_reason".into(), "snapshot_unavailable".into()),
            ("semantic_parity".into(), "false".into()),
            ("authority_parity".into(), "false".into()),
            ("realization_parity".into(), "false".into()),
            ("replay_parity".into(), "false".into()),
            ("authority_outcome".into(), "typed_non_declarative".into()),
            ("authority_outcome_digest".into(), "none".into()),
            ("authority_surface_digest".into(), "none".into()),
            ("claim_identity_digest".into(), "none".into()),
            ("fact_binding_digest".into(), "none".into()),
            ("claim_authority_digest".into(), "none".into()),
            ("v1_authoritative".into(), "true".into()),
            ("v1_fallback_used".into(), "false".into()),
        ]);
        let digest = execution_trace::calculate_stable_digest(&metadata)
            .unwrap_or_else(|error| format!("digest-error:{error}"));
        if let Some(trace) = trace.as_deref_mut() {
            trace.record_step(
                "response_plan_v2",
                digest.clone(),
                digest,
                Duration::ZERO,
                metadata,
            );
        }
        return None;
    }

    let context = SelfSelectionContext::quantize(
        routed.prepared().conatus_energy(),
        routed.prepared().salience(),
        0.0,
    );
    let execution = execute_audited_topic_at(
        routed.prepared().input().subject(),
        qxfx0_plan_v2::EvidenceEvaluationContext::new(logical_turn, None),
        &budgets,
        &contract,
        context,
        policy,
        qxfx0_plan_v2::valency_lexicon(),
        qxfx0_morphology::get_runtime(),
    );
    let record =
        execution
            .selection
            .zip(execution.exact_replay)
            .and_then(|(selection, exact_replay)| {
                let binary_digest = match current_binary_digest() {
                    Ok(digest) => digest,
                    Err(error) => {
                        tracing::warn!("V2 turn record dropped: binary digest failed: {error}");
                        return None;
                    }
                };
                Some(TurnRecord::new(
                    contract.clone(),
                    selection,
                    binary_digest,
                    exact_replay,
                ))
            });
    let result = execution.result;
    let realized_surface = execution.realized;
    let fallback = qxfx0_plan_v2::fallback_action_for_result(&result);
    let expected_source_digest = qxfx0_plan_v2::audited_surface_source_digest(topic)
        .unwrap_or_else(|error| {
            tracing::warn!("audited surface digest unavailable for '{topic}': {error}");
            String::new()
        });
    let authority_outcome = match realized_surface.clone() {
        Some(surface) => qxfx0_plan_v2::authority_outcome(
            topic,
            qxfx0_plan_v2::AuthoritySurfaceStrategy::Compositional,
            Ok(surface),
            &expected_source_digest,
        ),
        None if fallback == qxfx0_plan_v2::FallbackAction::AuditedV1Renderer => {
            qxfx0_plan_v2::authority_outcome(
                topic,
                qxfx0_plan_v2::AuthoritySurfaceStrategy::Compositional,
                Err(format!("V2 realization failed: {result:?}")),
                &expected_source_digest,
            )
        }
        None => qxfx0_plan_v2::V2AuthorityOutcome::TypedNonDeclarative {
            reason: format!("no V2 realized surface: {result:?}"),
        },
    };
    let (
        claim_identity_digest,
        fact_binding_digest,
        claim_authority_digest,
        semantic_parity,
        authority_parity,
    ) = match &result {
        qxfx0_plan_v2::V2ExecutionResult::Attempt(qxfx0_plan_v2::V2Attempt::Realizable(plan)) => {
            let authorized = plan.authorized();
            let projected = authorized.certified().candidate().projected_claims();
            let claim_identity_digest = execution_trace::calculate_stable_digest(&projected)
                .unwrap_or_else(|error| format!("digest-error:{error}"));
            let fact_binding_digest =
                execution_trace::calculate_stable_digest(authorized.certified().bindings())
                    .unwrap_or_else(|error| format!("digest-error:{error}"));
            let claim_authority_digest =
                execution_trace::calculate_stable_digest(authorized.authorities())
                    .unwrap_or_else(|error| format!("digest-error:{error}"));
            let expected_facts = qxfx0_semantic::argued_topic_registry()
                .ok()
                .and_then(|registry| registry.get(topic))
                .map(|entry| {
                    entry
                        .statements()
                        .map(|statement| statement.fact_id())
                        .collect::<Vec<_>>()
                });
            let semantic_parity = expected_facts.as_ref().is_some_and(|expected| {
                projected.len() == expected.len()
                    && projected
                        .iter()
                        .zip(expected)
                        .all(|(claim, expected_fact)| {
                            authorized.certified().bindings().get(&claim.claim_id)
                                == Some(*expected_fact)
                        })
            });
            let authority_parity = semantic_parity
                && projected
                    .iter()
                    .all(|claim| authorized.authority_for(&claim.claim_id).is_some());
            (
                claim_identity_digest,
                fact_binding_digest,
                claim_authority_digest,
                semantic_parity,
                authority_parity,
            )
        }
        _ => ("none".into(), "none".into(), "none".into(), false, false),
    };
    let replay_bundle_digest = record
        .as_ref()
        .map(|record| record.exact_replay.bundle_digest.clone());
    let artifact = ResponsePlanV2Artifact {
        schema: "qxfx0.response-plan-v2.shadow.v1",
        contract,
        record,
        result,
        realized: realized_surface,
        fallback,
        authority_outcome,
    };
    let input_digest = execution_trace::calculate_stable_digest(&(
        routed.prepared().input().subject(),
        logical_turn,
        artifact.contract.digest.as_str(),
    ))
    .unwrap_or_else(|error| format!("digest-error:{error}"));
    let output_digest = execution_trace::calculate_stable_digest(&artifact)
        .unwrap_or_else(|error| format!("digest-error:{error}"));
    let receipt = AuthorityDecisionReceipt {
        topic: topic.to_string(),
        requested_mode,
        effective_mode,
        authority,
        outcome: artifact.authority_outcome.clone(),
        output_digest: artifact
            .authority_outcome
            .output()
            .map(|output| output.surface_digest.clone()),
        artifact_digest: output_digest.clone(),
        contract_digest: artifact.contract.digest.clone(),
        replay_bundle_digest,
        guard_classification: "pending".into(),
    };
    let execution_downgrade = !matches!(
        &artifact.result,
        qxfx0_plan_v2::V2ExecutionResult::Attempt(qxfx0_plan_v2::V2Attempt::Realizable(_))
    );
    let authority_kind = artifact.authority_outcome.kind();
    let authority_downgrade = matches!(
        &artifact.authority_outcome,
        qxfx0_plan_v2::V2AuthorityOutcome::RealizationDowngrade { .. }
    );
    let downgrade_count =
        scope_downgrade_count + usize::from(execution_downgrade) + usize::from(authority_downgrade);
    let downgrade_reason = if execution_downgrade {
        if authority_downgrade {
            "realization_downgrade"
        } else {
            "v2_execution_failure"
        }
    } else {
        downgrade_reason
    };
    let realization_parity = matches!(
        &artifact.authority_outcome,
        qxfx0_plan_v2::V2AuthorityOutcome::Compositional { output }
            if !output.clauses.is_empty()
    );
    let replay_parity = artifact.record.is_some();
    let mut metadata = BTreeMap::from([
        ("requested_mode".into(), format!("{requested_mode:?}")),
        ("effective_mode".into(), format!("{effective_mode:?}")),
        ("canary_eligible".into(), canary_eligible.to_string()),
        (
            "authority_intent_eligible".into(),
            authority_intent_eligible.to_string(),
        ),
        ("canary_digest".into(), canary_digest),
        ("attempted".into(), "true".into()),
        ("completed".into(), "true".into()),
        ("downgrade_count".into(), downgrade_count.to_string()),
        ("downgrade_reason".into(), downgrade_reason.into()),
        (
            "v1_authoritative".into(),
            (!receipt.can_emit_v2()).to_string(),
        ),
        ("v1_fallback_used".into(), "false".into()),
    ]);
    metadata.extend([
        ("contract_digest".into(), artifact.contract.digest.clone()),
        ("replay_integrity".into(), "verified-by-construction".into()),
        ("semantic_parity".into(), semantic_parity.to_string()),
        ("authority_parity".into(), authority_parity.to_string()),
        ("realization_parity".into(), realization_parity.to_string()),
        ("replay_parity".into(), replay_parity.to_string()),
        (
            "attestation_presentation_surface_signed".into(),
            "false".into(),
        ),
        ("claim_identity_digest".into(), claim_identity_digest),
        ("fact_binding_digest".into(), fact_binding_digest),
        ("claim_authority_digest".into(), claim_authority_digest),
        ("authority_outcome".into(), authority_kind.into()),
        (
            "authority_outcome_digest".into(),
            execution_trace::calculate_stable_digest(&artifact.authority_outcome)
                .unwrap_or_else(|error| format!("digest-error:{error}")),
        ),
        (
            "authority_surface_digest".into(),
            artifact
                .authority_outcome
                .output()
                .map(|output| output.surface_digest.clone())
                .unwrap_or_else(|| "none".into()),
        ),
        (
            "authority_source_digest".into(),
            artifact
                .authority_outcome
                .source_digest()
                .unwrap_or("none")
                .into(),
        ),
        (
            "replay_bundle_digest".into(),
            receipt
                .replay_bundle_digest
                .clone()
                .unwrap_or_else(|| "none".into()),
        ),
        (
            "legacy_graph_v2_declarative_fallback".into(),
            "false".into(),
        ),
    ]);
    if let Some(trace) = trace {
        let _ = trace.set_authority_receipt(&receipt);
        trace.record_step(
            "response_plan_v2",
            input_digest,
            output_digest,
            Duration::ZERO,
            metadata,
        );
    }
    Some(receipt)
}
