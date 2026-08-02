//! Audited-corpus chain construction (ADR-0034 §10).
//!
//! The `response-plan-v2-audited-corpus` gate verifies semantic and authority
//! parity over all 30 audited topics: every stated claim of every topic must
//! traverse the whole certificate chain — admission, evidence, assertion —
//! and land on a `ClaimAuthority`. This module is the chain builder the gate
//! reads; the manifest locks the sources and the approved surfaces, and the
//! gate runs this builder per topic.
//!
//! The corpus states curated content only: every statement of a topic is
//! itself an audited fact, so no derivation steps are present. The derived
//! stratum (`DerivedCuratedConclusion`) is exercised by the assertion
//! boundary's own tests, not by this gate.

use std::collections::BTreeMap;

use crate::argued_topics::{argued_topic_registry, ArguedTopicRegistry};
use crate::fact_model::FactId;
use crate::knowledge_pack::active_pack_set;
use crate::response_plan::SemanticId;
use crate::response_plan_v2::admission::{AdmissionError, LeafAdmittedPlan};
use crate::response_plan_v2::assertion::{
    AssertionAuthorizedPlan, AssertionError, AssertionPolicy, ClaimAuthority,
};
use crate::response_plan_v2::candidate::{CandidateInvariantError, CandidateResponsePlan};
use crate::response_plan_v2::derivation::DerivationDag;
use crate::response_plan_v2::discourse::{ClaimId, DiscoursePlan, DiscourseTree};
use crate::response_plan_v2::evidence::{
    EvidenceCertifiedPlan, EvidenceError, EvidenceEvaluationContext,
};
use crate::response_plan_v2::proposition::{PropositionDagBuilder, PropositionNode};
use crate::response_plan_v2::realization::{linearize, try_realize, RealizedSurface};
use crate::response_plan_v2::selection::{
    select_candidate, CandidateSelectionSignals, SelectionCandidate, SelectionPolicy,
    SelectionReceipt, SelfSelectionContext,
};
use crate::response_plan_v2::snapshot::TurnContractSnapshot;
use crate::response_plan_v2::syn_tree::{Clause, NounPhrase, SynTree, VerbPhrase};
use crate::response_plan_v2::valency::{
    starts_with_word, Complement, ValencyError, ValencyLexicon,
};
use crate::response_plan_v2::{
    attempt_input_digest, enforce_work_budget, BoundedRejectedArtifact, BudgetPhase,
    BudgetResource, BudgetWorkItem, CertifiedPrefix, V2Attempt, V2BudgetPolicy, V2ExecutionResult,
    V2Failure, V2PreCandidateOutcome, V2Route,
};
use qxfx0_morphology::MorphologyRuntime;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AuditedCorpusError {
    #[error("startup: {0}")]
    Startup(String),
    #[error("candidate stratum: {0}")]
    Candidate(#[from] CandidateInvariantError),
    #[error("admission boundary: {0}")]
    Admission(#[from] AdmissionError),
    #[error("evidence boundary: {0}")]
    Evidence(#[from] EvidenceError),
    #[error("assertion boundary: {0}")]
    Assertion(#[from] AssertionError),
    #[error("topic '{0}' is not in the audited profile")]
    UnknownTopic(String),
}

impl AuditedCorpusError {
    pub fn is_unknown_topic(&self) -> bool {
        matches!(self, Self::UnknownTopic(_))
    }
}

/// One topic's full chain: every stated claim admitted, certified and
/// authorized under the V1 policy.
#[derive(Debug, Clone)]
pub struct AuditedTopicPlan {
    topic: String,
    authorized: AssertionAuthorizedPlan,
}

#[derive(Debug, Clone)]
pub struct AuditedV2Execution {
    pub result: V2ExecutionResult,
    pub selection: Option<SelectionReceipt>,
    pub realized: Option<RealizedSurface>,
    pub exact_replay: Option<super::snapshot::ExactReplayBundle>,
}

struct AuditedCandidate {
    topic: String,
    candidate: CandidateResponsePlan,
    bindings: BTreeMap<ClaimId, FactId>,
}

impl AuditedTopicPlan {
    pub fn topic(&self) -> &str {
        &self.topic
    }

    pub fn authorized(&self) -> &AssertionAuthorizedPlan {
        &self.authorized
    }

    pub fn certified(&self) -> &EvidenceCertifiedPlan {
        self.authorized.certified()
    }

    pub fn claims(&self) -> Vec<(ClaimId, FactId, ClaimAuthority)> {
        self.authorized
            .certified()
            .candidate()
            .projected_claims()
            .into_iter()
            .map(|claim| {
                let fact_id = self
                    .authorized
                    .certified()
                    .bindings()
                    .get(&claim.claim_id)
                    .expect("an authorized claim is bound")
                    .clone();
                let authority = self
                    .authorized
                    .authority_for(&claim.claim_id)
                    .expect("an authorized claim has an authority")
                    .clone();
                (claim.claim_id, fact_id, authority)
            })
            .collect()
    }
}

/// Build the whole chain for one audited topic: candidate over the topic's
/// own statement facts, admitted under the active pack set, certified under
/// the V1 temporal context, authorized under the V1 assertion policy.
pub fn build_audited_topic(topic: &str) -> Result<AuditedTopicPlan, AuditedCorpusError> {
    build_audited_topic_at(topic, EvidenceEvaluationContext::new(0, None))
}

/// Runtime form of the corpus adapter. The logical turn is supplied by the
/// persisted input envelope rather than hard-coded by the gate fixture.
pub fn build_audited_topic_at(
    topic: &str,
    evidence_context: EvidenceEvaluationContext,
) -> Result<AuditedTopicPlan, AuditedCorpusError> {
    let prepared = prepare_audited_candidate(topic)?;
    certify_audited_candidate(prepared, evidence_context)
}

fn prepare_audited_candidate(topic: &str) -> Result<AuditedCandidate, AuditedCorpusError> {
    let argued = argued_topic_registry()
        .map_err(|error| AuditedCorpusError::Startup(format!("registry unavailable: {error}")))?;
    let pack = active_pack_set();
    let entry = argued
        .get(topic)
        .ok_or_else(|| AuditedCorpusError::UnknownTopic(topic.to_string()))?;

    // Propositions from the statement facts' own triples, in discourse order:
    // thesis, counterpoint, optional consequence. The planner binds a claim to
    // the fact whose content the claim states.
    let mut builder = PropositionDagBuilder::new();
    let mut leaves = Vec::new();
    for statement in entry.statements() {
        let record = pack.facts().get(statement.fact_id()).ok_or_else(|| {
            AuditedCorpusError::Startup(format!(
                "audited fact '{}' is absent from the active pack",
                statement.fact_id().as_str()
            ))
        })?;
        let node = PropositionNode::Predicate {
            subject: SemanticId::try_new(record.subject.0.clone())
                .map_err(AuditedCorpusError::Startup)?,
            relation: SemanticId::try_new(record.relation.as_str())
                .map_err(AuditedCorpusError::Startup)?,
            object: SemanticId::try_new(record.object.0.clone())
                .map_err(AuditedCorpusError::Startup)?,
        };
        leaves.push(builder.insert(node));
    }
    let propositions = builder
        .build()
        .map_err(|error| AuditedCorpusError::Startup(error.to_string()))?;

    let sequence = match leaves.len() {
        2 => vec![
            DiscourseTree::Thesis(leaves[0].clone()),
            DiscourseTree::Counterpoint(leaves[1].clone()),
        ],
        _ => vec![
            DiscourseTree::Thesis(leaves[0].clone()),
            DiscourseTree::Counterpoint(leaves[1].clone()),
            DiscourseTree::Consequence(leaves[2].clone()),
        ],
    };
    let candidate = CandidateResponsePlan::try_new(
        propositions,
        DerivationDag::empty(),
        DiscoursePlan::try_new(DiscourseTree::Sequence(sequence))
            .map_err(|error| AuditedCorpusError::Startup(error.to_string()))?,
    )?;

    let mut bindings = BTreeMap::new();
    for (claim, statement) in candidate
        .projected_claims()
        .into_iter()
        .zip(entry.statements())
    {
        bindings.insert(claim.claim_id, statement.fact_id().clone());
    }
    Ok(AuditedCandidate {
        topic: topic.to_string(),
        candidate,
        bindings,
    })
}

fn certify_audited_candidate(
    prepared: AuditedCandidate,
    evidence_context: EvidenceEvaluationContext,
) -> Result<AuditedTopicPlan, AuditedCorpusError> {
    let argued = argued_topic_registry()
        .map_err(|error| AuditedCorpusError::Startup(format!("registry unavailable: {error}")))?;
    let pack = active_pack_set();
    let admitted =
        LeafAdmittedPlan::try_admit(prepared.candidate, prepared.bindings, pack, argued)?;
    let certified = EvidenceCertifiedPlan::try_certify(
        admitted,
        &evidence_context,
        pack.facts(),
        pack.fingerprint(),
    )?;
    let authorized =
        AssertionAuthorizedPlan::try_authorize(certified, &AssertionPolicy::v1(), pack.facts())?;

    Ok(AuditedTopicPlan {
        topic: prepared.topic,
        authorized,
    })
}

/// Execute the complete audited V2 chain under one captured turn contract.
/// Pre-candidate outcomes carry no prefix; every later failure carries exactly
/// the strongest certificate completed before that boundary failed.
#[allow(clippy::too_many_arguments)]
pub fn execute_audited_topic_at(
    topic: &str,
    evidence_context: EvidenceEvaluationContext,
    budgets: &V2BudgetPolicy,
    contract: &TurnContractSnapshot,
    selection_context: SelfSelectionContext,
    selection_policy: SelectionPolicy,
    lexicon: &ValencyLexicon,
    morphology: &MorphologyRuntime,
) -> AuditedV2Execution {
    if let Err(error) = contract.verify_integrity() {
        return pre_candidate(V2PreCandidateOutcome::Startup {
            reason: error.to_string(),
        });
    }
    if contract.authority.pack_set_digest != active_pack_set().fingerprint()
        || contract.authority.assertion_policy_digest != AssertionPolicy::v1().digest()
        || contract.planning.budgets_digest != budgets.digest()
        || contract.selection.self_policy_digest != selection_policy.digest()
        || contract.realization.valency_digest != lexicon.fingerprint()
        || contract.realization.morphology_digest != morphology.lexemes_sha256()
        || contract.realization.morphology_depth_digest
            != super::morphology_depth::preposition_allomorphs().fingerprint()
    {
        return pre_candidate(V2PreCandidateOutcome::Startup {
            reason: "captured turn contract does not match supplied deterministic assets".into(),
        });
    }
    let prepared = match prepare_audited_candidate(topic) {
        Ok(prepared) => prepared,
        Err(AuditedCorpusError::UnknownTopic(_)) => {
            return pre_candidate(V2PreCandidateOutcome::NotApplicable {
                route: V2Route::UnsupportedInput,
            });
        }
        Err(AuditedCorpusError::Candidate(error)) => {
            return pre_candidate(V2PreCandidateOutcome::Candidate {
                failure: error.to_string(),
            });
        }
        Err(AuditedCorpusError::Startup(reason)) => {
            return pre_candidate(V2PreCandidateOutcome::Startup { reason });
        }
        Err(error) => {
            return pre_candidate(V2PreCandidateOutcome::Startup {
                reason: error.to_string(),
            });
        }
    };
    let candidate = prepared.candidate;
    let input_digest = attempt_input_digest(&(
        topic,
        evidence_context.logical_turn,
        evidence_context.authority_as_of.as_deref(),
        contract.digest.as_str(),
    ));
    if let Err(rejection) = enforce_candidate_budget(
        &candidate,
        budgets,
        &contract.planning.fingerprint,
        &input_digest,
    ) {
        return rejected_truncated(
            CertifiedPrefix::Candidate(candidate),
            V2Failure::Budget(rejection.error),
            rejection.witness,
        );
    }

    let pack = active_pack_set();
    let argued =
        argued_topic_registry().expect("registry was available during candidate preparation");
    let candidate_prefix = candidate.clone();
    let admitted = match LeafAdmittedPlan::try_admit(candidate, prepared.bindings, pack, argued) {
        Ok(admitted) => admitted,
        Err(error) => {
            return rejected(
                CertifiedPrefix::Candidate(candidate_prefix),
                V2Failure::Admission(error),
            )
        }
    };
    let certified = match EvidenceCertifiedPlan::try_certify(
        admitted.clone(),
        &evidence_context,
        pack.facts(),
        pack.fingerprint(),
    ) {
        Ok(certified) => certified,
        Err(error) => {
            return rejected(
                CertifiedPrefix::Admitted(admitted),
                V2Failure::Evidence(error),
            )
        }
    };
    let authorized = match AssertionAuthorizedPlan::try_authorize(
        certified.clone(),
        &AssertionPolicy::v1(),
        pack.facts(),
    ) {
        Ok(authorized) => authorized,
        Err(error) => {
            return rejected(
                CertifiedPrefix::EvidenceCertified(certified),
                V2Failure::Assertion(error),
            )
        }
    };
    if let Err(rejection) = super::attempt::enforce_authorized_budget(
        &authorized,
        budgets,
        &contract.planning.fingerprint,
        &input_digest,
    ) {
        return rejected_truncated(
            CertifiedPrefix::AssertionAuthorized(authorized),
            V2Failure::Budget(rejection.error),
            rejection.witness,
        );
    }
    let tree = match (AuditedTopicPlan {
        topic: prepared.topic,
        authorized: authorized.clone(),
    })
    .syn_tree(lexicon)
    {
        Ok(tree) => tree,
        Err(error) => {
            return rejected(
                CertifiedPrefix::AssertionAuthorized(authorized),
                V2Failure::Realization(error.into()),
            )
        }
    };
    let syntax_work = tree
        .iter()
        .map(|(occurrence, _)| BudgetWorkItem {
            resource: BudgetResource::Clauses,
            id: format!(
                "{}:{}",
                occurrence.discourse_root_digest(),
                occurrence.canonical_path()
            ),
        })
        .collect::<Vec<_>>();
    if let Err(rejection) = enforce_work_budget(
        BudgetPhase::Realization,
        BudgetResource::Clauses,
        &syntax_work,
        budgets.clauses,
        &contract.planning.fingerprint,
        &input_digest,
    ) {
        return rejected_truncated(
            CertifiedPrefix::AssertionAuthorized(authorized),
            V2Failure::Budget(rejection.error),
            rejection.witness,
        );
    }
    let selected = select_candidate(
        vec![SelectionCandidate::new(
            authorized.certified().candidate().clone(),
            CandidateSelectionSignals::neutral(),
        )],
        selection_context,
        selection_policy,
    )
    .expect("one immutable candidate is selectable");
    let selection = selected.receipt().clone();
    let realizable = match try_realize(
        authorized.clone(),
        &tree,
        &contract.realization,
        lexicon,
        morphology,
    ) {
        Ok(realizable) => realizable,
        Err(error) => {
            return rejected(
                CertifiedPrefix::AssertionAuthorized(authorized),
                V2Failure::Realization(error),
            )
        }
    };
    let realized = match linearize(&realizable, &contract.realization) {
        Ok(surface) => surface,
        Err(error) => {
            return rejected(
                CertifiedPrefix::Realizable(Box::new(realizable)),
                V2Failure::Snapshot(error),
            )
        }
    };
    let byte_work = realized
        .clauses
        .iter()
        .flat_map(|clause| clause.as_bytes())
        .enumerate()
        .map(|(offset, byte)| BudgetWorkItem {
            resource: BudgetResource::Bytes,
            id: format!("{offset}:{byte:02x}"),
        })
        .collect::<Vec<_>>();
    if let Err(rejection) = enforce_work_budget(
        BudgetPhase::Realization,
        BudgetResource::Bytes,
        &byte_work,
        budgets.realized_bytes,
        &contract.planning.fingerprint,
        &input_digest,
    ) {
        return rejected_truncated(
            CertifiedPrefix::Realizable(Box::new(realizable)),
            V2Failure::Budget(rejection.error),
            rejection.witness,
        );
    }
    let exact_replay = super::snapshot::ExactReplayBundle::capture(
        super::snapshot::ReplayInputEnvelope {
            topic: topic.to_string(),
            logical_turn: evidence_context.logical_turn,
            authority_as_of: evidence_context.authority_as_of,
        },
        contract,
        &selection,
        &realizable,
        realized.clone(),
    );
    AuditedV2Execution {
        result: V2ExecutionResult::Attempt(V2Attempt::Realizable(Box::new(realizable))),
        selection: Some(selection),
        realized: Some(realized),
        exact_replay: Some(exact_replay),
    }
}

fn enforce_candidate_budget(
    candidate: &CandidateResponsePlan,
    policy: &V2BudgetPolicy,
    planning_policy_digest: &str,
    input_digest: &str,
) -> Result<(), Box<super::attempt::BudgetRejection>> {
    let checks = [
        (
            BudgetResource::Propositions,
            candidate
                .propositions()
                .iter()
                .map(|(id, _)| BudgetWorkItem {
                    resource: BudgetResource::Propositions,
                    id: id.as_str().to_string(),
                })
                .collect::<Vec<_>>(),
            policy.propositions,
        ),
        (
            BudgetResource::Derivations,
            candidate
                .derivations()
                .iter()
                .map(|(id, _)| BudgetWorkItem {
                    resource: BudgetResource::Derivations,
                    id: id.as_str().to_string(),
                })
                .collect::<Vec<_>>(),
            policy.derivations,
        ),
        (
            BudgetResource::DiscourseOccurrences,
            candidate
                .projected_claims()
                .into_iter()
                .map(|claim| BudgetWorkItem {
                    resource: BudgetResource::DiscourseOccurrences,
                    id: claim.claim_id.as_str().to_string(),
                })
                .collect::<Vec<_>>(),
            policy.discourse_occurrences,
        ),
    ];
    for (resource, work, limit) in checks {
        enforce_work_budget(
            BudgetPhase::Candidate,
            resource,
            &work,
            limit,
            planning_policy_digest,
            input_digest,
        )?;
    }
    Ok(())
}

fn pre_candidate(outcome: V2PreCandidateOutcome) -> AuditedV2Execution {
    AuditedV2Execution {
        result: V2ExecutionResult::PreCandidate(outcome),
        selection: None,
        realized: None,
        exact_replay: None,
    }
}

fn rejected(prefix: CertifiedPrefix, failure: V2Failure) -> AuditedV2Execution {
    AuditedV2Execution {
        result: V2ExecutionResult::Attempt(V2Attempt::Rejected {
            artifact: Box::new(BoundedRejectedArtifact::new(prefix)),
            failure,
        }),
        selection: None,
        realized: None,
        exact_replay: None,
    }
}

fn rejected_truncated(
    prefix: CertifiedPrefix,
    failure: V2Failure,
    witness: super::attempt::TruncationWitness,
) -> AuditedV2Execution {
    AuditedV2Execution {
        result: V2ExecutionResult::Attempt(V2Attempt::Rejected {
            artifact: Box::new(BoundedRejectedArtifact::truncated(prefix, witness)),
            failure,
        }),
        selection: None,
        realized: None,
        exact_replay: None,
    }
}

impl AuditedTopicPlan {
    pub fn into_authorized(self) -> AssertionAuthorizedPlan {
        self.authorized
    }

    /// Build one occurrence-addressed syntax node for every authorized claim.
    /// The audited thesis is compositional; the remaining approved corpus
    /// surfaces stay explicit fixed nodes until their own syntax is admitted.
    pub fn syn_tree(&self, lexicon: &ValencyLexicon) -> Result<SynTree, ValencyError> {
        let claims = self.authorized.certified().candidate().projected_claims();
        let argued = argued_topic_registry()
            .expect("audited registry is available")
            .get(&self.topic)
            .expect("audited topic remains available");
        let mut statements = argued.statements();
        let claim = claims.first().expect("audited topic has a thesis");
        statements.next().expect("audited topic has a thesis");
        let fact_id = self
            .authorized
            .certified()
            .bindings()
            .get(&claim.claim_id)
            .expect("thesis is bound");
        let record = active_pack_set()
            .facts()
            .get(fact_id)
            .expect("audited fact exists");
        let pack = active_pack_set();
        let lemma = |concept_id: &qxfx0_types::ConceptId| {
            pack.resolver()
                .records()
                .find(|entry| &entry.concept_id == concept_id)
                .map(|entry| entry.canonical_lemma.clone())
                .expect("audited fact concept has a canonical lemma")
        };
        let relation_id = argued
            .primary_proposition()
            .canonical_slots()
            .map(|(_, relation, _)| relation.as_str())
            .expect("audited primary proposition has canonical slots");
        let frame = lexicon.get(relation_id)?;
        let object = lemma(&record.object);
        let complement = match frame.complement() {
            Complement::None => None,
            Complement::Uninflected => Some(NounPhrase::fixed(object, None)),
            governing if object.contains(' ') => {
                let required = governing.required_case().expect("government names a case");
                if governing
                    .preposition()
                    .is_some_and(|preposition| starts_with_word(&object, preposition))
                {
                    Some(NounPhrase::fixed_with_preposition(object, required))
                } else {
                    Some(NounPhrase::fixed(object, Some(required)))
                }
            }
            _ => Some(NounPhrase::lexical(object)),
        };
        let mut tree = SynTree::new();
        tree.push(
            claim.occurrence.clone(),
            Clause::new(
                NounPhrase::lexical(lemma(&record.subject)),
                VerbPhrase::new(relation_id, complement),
            ),
        );
        for (claim, statement) in claims.into_iter().skip(1).zip(statements) {
            tree.push_fixed(claim.occurrence, statement.surface());
        }
        Ok(tree)
    }
}

/// Aggregate over the whole audited corpus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditedCorpusReport {
    pub topics: usize,
    pub statements: usize,
    pub curated: usize,
    pub derived_curated: usize,
}

/// Run the chain over every audited topic. Any topic that cannot be fully
/// authorized fails the corpus as a whole, together with the reasons.
pub fn audit_audited_corpus() -> Result<AuditedCorpusReport, Vec<(String, AuditedCorpusError)>> {
    let argued: &ArguedTopicRegistry = argued_topic_registry().map_err(|error| {
        vec![(
            "registry".to_string(),
            AuditedCorpusError::UnknownTopic(format!("registry unavailable: {error}")),
        )]
    })?;
    let mut failures = Vec::new();
    let mut report = AuditedCorpusReport {
        topics: 0,
        statements: 0,
        curated: 0,
        derived_curated: 0,
    };
    for topic in argued.topics() {
        match build_audited_topic(topic.topic().as_str()) {
            Ok(plan) => {
                report.topics += 1;
                for (_, _, authority) in plan.claims() {
                    report.statements += 1;
                    match authority {
                        ClaimAuthority::Curated { .. } => report.curated += 1,
                        ClaimAuthority::DerivedCuratedConclusion { .. } => {
                            report.derived_curated += 1
                        }
                    }
                }
            }
            Err(error) => failures.push((topic.topic().as_str().to_string(), error)),
        }
    }
    if failures.is_empty() {
        Ok(report)
    } else {
        Err(failures)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::response_plan_v2::{
        preposition_allomorphs, valency_lexicon, AuthoritySnapshot, PlanningPolicySnapshot,
        RealizationSnapshot, SelectionPolicySnapshot,
    };

    fn execution_contract(
        budgets: &V2BudgetPolicy,
        policy: SelectionPolicy,
    ) -> TurnContractSnapshot {
        TurnContractSnapshot::new(
            AuthoritySnapshot::new(
                active_pack_set().fingerprint(),
                AssertionPolicy::v1().digest(),
            ),
            PlanningPolicySnapshot::new(budgets.digest(), "proposition-canon-v1"),
            RealizationSnapshot::new(
                valency_lexicon().fingerprint(),
                "clause-grammar-v1",
                qxfx0_morphology::get_runtime().lexemes_sha256(),
                preposition_allomorphs().fingerprint(),
            ),
            SelectionPolicySnapshot::new(policy),
        )
    }

    fn execute(topic: &str, budgets: V2BudgetPolicy) -> AuditedV2Execution {
        let policy = SelectionPolicy::default();
        let contract = execution_contract(&budgets, policy);
        execute_audited_topic_at(
            topic,
            EvidenceEvaluationContext::new(1, None),
            &budgets,
            &contract,
            SelfSelectionContext::quantize(0.0, 0.0, 0.0),
            policy,
            valency_lexicon(),
            qxfx0_morphology::get_runtime(),
        )
    }

    #[test]
    fn the_corpus_closes_over_30_topics_and_69_claims() {
        let report = audit_audited_corpus().expect("whole corpus must authorize");
        assert_eq!(report.topics, 30);
        assert_eq!(report.statements, 69);
        assert_eq!(report.curated, 69);
        assert_eq!(report.derived_curated, 0);
    }

    #[test]
    fn an_unknown_topic_is_rejected_before_the_chain() {
        assert!(matches!(
            build_audited_topic("несуществующая_тема"),
            Err(AuditedCorpusError::UnknownTopic(_))
        ));
    }

    #[test]
    fn unified_chain_classifies_unsupported_input_without_a_prefix() {
        let execution = execute("несуществующая_тема", V2BudgetPolicy::default());
        assert!(matches!(
            execution.result,
            V2ExecutionResult::PreCandidate(V2PreCandidateOutcome::NotApplicable {
                route: V2Route::UnsupportedInput
            })
        ));
    }

    #[test]
    fn unified_chain_keeps_candidate_as_the_budget_prefix() {
        let execution = execute(
            "свобода",
            V2BudgetPolicy {
                propositions: 0,
                ..V2BudgetPolicy::default()
            },
        );
        assert!(matches!(
            execution.result,
            V2ExecutionResult::Attempt(V2Attempt::Rejected { artifact, failure: V2Failure::Budget(_) })
                if matches!(artifact.prefix(), CertifiedPrefix::Candidate(_))
        ));
    }

    #[test]
    fn unified_chain_returns_realizable_with_all_claim_surfaces() {
        let execution = execute("свобода", V2BudgetPolicy::default());
        assert!(matches!(
            execution.result,
            V2ExecutionResult::Attempt(V2Attempt::Realizable(_))
        ));
        let realized = execution.realized.expect("surface");
        assert_eq!(realized.clauses.len(), 3);
        assert_eq!(
            execution
                .exact_replay
                .expect("captured replay")
                .reproduce()
                .expect("asset-independent reproduction"),
            realized
        );
        assert!(execution.selection.is_some());
    }

    #[test]
    fn realization_budgets_preserve_the_strongest_prefix() {
        let clause_limited = execute(
            "свобода",
            V2BudgetPolicy {
                clauses: 2,
                ..V2BudgetPolicy::default()
            },
        );
        assert!(matches!(
            clause_limited.result,
            V2ExecutionResult::Attempt(V2Attempt::Rejected { artifact, failure: V2Failure::Budget(_) })
                if matches!(artifact.prefix(), CertifiedPrefix::AssertionAuthorized(_))
        ));

        let byte_limited = execute(
            "свобода",
            V2BudgetPolicy {
                realized_bytes: 1,
                ..V2BudgetPolicy::default()
            },
        );
        assert!(matches!(
            byte_limited.result,
            V2ExecutionResult::Attempt(V2Attempt::Rejected { artifact, failure: V2Failure::Budget(_) })
                if matches!(artifact.prefix(), CertifiedPrefix::Realizable(_))
        ));
    }

    #[test]
    fn every_claim_binds_the_statement_fact_it_states() {
        let argued = argued_topic_registry().unwrap();
        for topic in argued.topics() {
            let plan = build_audited_topic(topic.topic().as_str()).expect("topic chain");
            let expected: Vec<&FactId> = topic
                .statements()
                .map(|statement| statement.fact_id())
                .collect();
            let claims = plan.claims();
            let actual: Vec<&FactId> = claims.iter().map(|(_, fact_id, _)| fact_id).collect();
            assert_eq!(actual, expected, "topic '{}'", topic.topic().as_str());
        }
    }

    #[test]
    fn the_policy_digest_is_the_v1_policy_for_every_topic() {
        let argued = argued_topic_registry().unwrap();
        for topic in argued.topics() {
            let plan = build_audited_topic(topic.topic().as_str()).expect("topic chain");
            assert_eq!(
                plan.authorized().policy_digest(),
                AssertionPolicy::v1().digest()
            );
        }
    }
}
