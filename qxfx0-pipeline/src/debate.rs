//! Pure projection from post-plan evidence into Debate Core v1 contracts.

use crate::turn_context::PlannedTurnContext;
use qxfx0_semantic::{ClaimRole, PlanSubject, PropositionMode};
use qxfx0_types::{
    ArgumentEdge, ArgumentEdgeKind, ArgumentNode, ArgumentNodeKind, CanonicalMoveFamily,
    DebateEvidenceRef, DebateMove, DebateObservationReceipt, DebateParticipant, LedgerEntry,
    PositionPolarity, RubricAssessment, RubricDimension, RubricScore,
};

pub(crate) fn observe(planned: &PlannedTurnContext) -> Result<DebateObservationReceipt, String> {
    let route = planned.routed();
    let outcome = planned.shadow_plan();
    let route_evidence = DebateEvidenceRef::RouteFamily(format!("{:?}", route.family()));
    let outcome_evidence = DebateEvidenceRef::PlanOutcome(outcome.kind().as_str().into());
    let topic_id = outcome
        .ready()
        .map(|plan| match plan.subject() {
            PlanSubject::Topic(id) => id.0.clone(),
            PlanSubject::Dialogue(_) => "dialogue".into(),
            PlanSubject::External(_) => "external_subject".into(),
        })
        .unwrap_or_else(|| {
            outcome
                .fallback()
                .and_then(|fallback| fallback.subject())
                .map(|subject| match subject {
                    qxfx0_semantic::FallbackSubject::KnownTopic(_) => "known_topic".into(),
                    qxfx0_semantic::FallbackSubject::UnresolvedTopic(_) => {
                        "unresolved_topic".into()
                    }
                    qxfx0_semantic::FallbackSubject::Dialogue(_) => "dialogue".into(),
                })
                .unwrap_or_else(|| "no_topic".into())
        });

    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut ledger = Vec::new();
    let mut node_evidence = Vec::new();
    let mut fact_evidence = Vec::new();
    if let Some(plan) = outcome.ready() {
        let thesis_id = plan
            .claims()
            .iter()
            .find(|claim| claim.role() == ClaimRole::Thesis)
            .map(|claim| claim.id().as_str().to_owned());
        for (sequence, claim) in plan.claims().iter().enumerate() {
            let node_id = claim.id().as_str().to_owned();
            let kind = node_kind(claim.role());
            let polarity = match claim.role() {
                ClaimRole::Thesis | ClaimRole::DialogueAct => PositionPolarity::Proposed,
                ClaimRole::Support | ClaimRole::Consequence => PositionPolarity::Supported,
                ClaimRole::Counterpoint => PositionPolarity::Qualified,
            };
            if let Some(thesis_id) = thesis_id.as_ref().filter(|id| **id != node_id) {
                let edge_kind = match claim.role() {
                    ClaimRole::Counterpoint => Some(ArgumentEdgeKind::Counters),
                    ClaimRole::Consequence => Some(ArgumentEdgeKind::Entails),
                    ClaimRole::Support => Some(ArgumentEdgeKind::Supports),
                    _ => None,
                };
                if let Some(kind) = edge_kind {
                    let (from, to) = if kind == ArgumentEdgeKind::Entails {
                        (thesis_id.clone(), node_id.clone())
                    } else {
                        (node_id.clone(), thesis_id.clone())
                    };
                    edges.push(ArgumentEdge { from, to, kind });
                }
            }
            if let Some(fact_id) = claim.fact_id() {
                fact_evidence.push(DebateEvidenceRef::Fact(fact_id.clone()));
            }
            node_evidence.push(DebateEvidenceRef::ArgumentNode(node_id.clone()));
            nodes.push(ArgumentNode {
                id: node_id.clone(),
                kind,
                participant: DebateParticipant::System,
                fact_id: claim.fact_id().cloned(),
            });
            ledger.push(LedgerEntry {
                sequence: sequence as u16,
                participant: DebateParticipant::System,
                node_id,
                polarity,
            });
        }
    }

    let clarity_evidence = node_evidence
        .first()
        .cloned()
        .into_iter()
        .chain(std::iter::once(outcome_evidence.clone()))
        .collect();
    let grounding_evidence = if fact_evidence.is_empty() {
        vec![outcome_evidence.clone()]
    } else {
        fact_evidence
    };
    let counter_evidence = edges
        .iter()
        .filter(|edge| edge.kind == ArgumentEdgeKind::Counters)
        .map(|edge| DebateEvidenceRef::ArgumentNode(edge.from.clone()))
        .collect::<Vec<_>>();
    let consequence_evidence = edges
        .iter()
        .filter(|edge| edge.kind == ArgumentEdgeKind::Entails)
        .map(|edge| DebateEvidenceRef::ArgumentNode(edge.to.clone()))
        .collect::<Vec<_>>();
    let rubric = vec![
        assessment(
            RubricDimension::ClaimClarity,
            if nodes.is_empty() { 0 } else { 10_000 },
            clarity_evidence,
        )?,
        assessment(
            RubricDimension::EvidenceGrounding,
            if nodes.iter().all(|node| node.fact_id.is_some()) && !nodes.is_empty() {
                10_000
            } else {
                0
            },
            grounding_evidence,
        )?,
        assessment(
            RubricDimension::CounterargumentCoverage,
            if counter_evidence.is_empty() {
                0
            } else {
                10_000
            },
            if counter_evidence.is_empty() {
                vec![route_evidence.clone()]
            } else {
                counter_evidence
            },
        )?,
        assessment(
            RubricDimension::ConsequenceCoverage,
            if consequence_evidence.is_empty() {
                0
            } else {
                10_000
            },
            if consequence_evidence.is_empty() {
                vec![outcome_evidence]
            } else {
                consequence_evidence
            },
        )?,
    ];

    DebateObservationReceipt::new(
        topic_id,
        debate_move(
            route.prepared().input().mode(),
            route.family(),
            route.prepared().input().is_challenge(),
        ),
        nodes,
        edges,
        ledger,
        rubric,
    )
    .map_err(|error| error.to_string())
}

fn assessment(
    dimension: RubricDimension,
    score: u16,
    evidence: Vec<DebateEvidenceRef>,
) -> Result<RubricAssessment, String> {
    Ok(RubricAssessment {
        dimension,
        score: RubricScore::from_basis_points(score).map_err(|error| error.to_string())?,
        evidence,
    })
}

fn node_kind(role: ClaimRole) -> ArgumentNodeKind {
    match role {
        ClaimRole::Thesis => ArgumentNodeKind::Thesis,
        ClaimRole::Support => ArgumentNodeKind::Support,
        ClaimRole::Counterpoint => ArgumentNodeKind::Counterpoint,
        ClaimRole::Consequence => ArgumentNodeKind::Consequence,
        ClaimRole::DialogueAct => ArgumentNodeKind::DialogueAct,
    }
}

fn debate_move(
    mode: PropositionMode,
    family: CanonicalMoveFamily,
    is_challenge: bool,
) -> DebateMove {
    if is_challenge || mode == PropositionMode::Challenge {
        return DebateMove::Challenge;
    }
    match family {
        CanonicalMoveFamily::CMDefine => DebateMove::Define,
        CanonicalMoveFamily::CMDistinguish => DebateMove::Distinguish,
        CanonicalMoveFamily::CMGround | CanonicalMoveFamily::CMAnchor => DebateMove::Ground,
        CanonicalMoveFamily::CMReflect | CanonicalMoveFamily::CMDeepen => DebateMove::Reflect,
        CanonicalMoveFamily::CMContact => DebateMove::Contact,
        CanonicalMoveFamily::CMConnect => DebateMove::Connect,
        CanonicalMoveFamily::CMConfront => DebateMove::Counter,
        CanonicalMoveFamily::CMClarify | CanonicalMoveFamily::CMRepair => DebateMove::Clarify,
        CanonicalMoveFamily::CMNextStep => DebateMove::InferConsequence,
        CanonicalMoveFamily::CMDescribe
        | CanonicalMoveFamily::CMPurpose
        | CanonicalMoveFamily::CMHypothesis => DebateMove::Assert,
    }
}
