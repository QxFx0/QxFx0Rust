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
use crate::response_plan_v2::syn_tree::{Clause, NounPhrase, SynTree, VerbPhrase};
use crate::response_plan_v2::valency::{
    starts_with_word, Complement, ValencyError, ValencyLexicon,
};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AuditedCorpusError {
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
    let argued = argued_topic_registry().map_err(|error| {
        AuditedCorpusError::UnknownTopic(format!("registry unavailable: {error}"))
    })?;
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
        let record = pack
            .facts()
            .get(statement.fact_id())
            .expect("an audited statement fact is present in the pack");
        let node = PropositionNode::Predicate {
            subject: SemanticId::try_new(record.subject.0.clone())
                .expect("statement subject is a semantic id"),
            relation: SemanticId::try_new(record.relation.as_str())
                .expect("statement relation is a semantic id"),
            object: SemanticId::try_new(record.object.0.clone())
                .expect("statement object is a semantic id"),
        };
        leaves.push(builder.insert(node));
    }
    let propositions = builder
        .build()
        .expect("statement propositions are well-formed");

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
        DiscoursePlan::try_new(DiscourseTree::Sequence(sequence)).expect("corpus discourse"),
    )?;

    let mut bindings = BTreeMap::new();
    for (claim, statement) in candidate
        .projected_claims()
        .into_iter()
        .zip(entry.statements())
    {
        bindings.insert(claim.claim_id, statement.fact_id().clone());
    }
    let admitted = LeafAdmittedPlan::try_admit(candidate, bindings, pack, argued)?;
    let certified = EvidenceCertifiedPlan::try_certify(
        admitted,
        &evidence_context,
        pack.facts(),
        pack.fingerprint(),
    )?;
    let authorized =
        AssertionAuthorizedPlan::try_authorize(certified, &AssertionPolicy::v1(), pack.facts())?;

    Ok(AuditedTopicPlan {
        topic: topic.to_string(),
        authorized,
    })
}

impl AuditedTopicPlan {
    pub fn into_authorized(self) -> AssertionAuthorizedPlan {
        self.authorized
    }

    /// Build the thesis syntax adapter without making syntax part of the
    /// semantic certificate. The lexicon and morphology remain late-bound.
    pub fn thesis_syn_tree(&self, lexicon: &ValencyLexicon) -> Result<SynTree, ValencyError> {
        let claim = self
            .authorized
            .certified()
            .candidate()
            .projected_claims()
            .into_iter()
            .next()
            .expect("audited topic has a thesis");
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
        let frame = lexicon.get(record.relation.as_str())?;
        let object = record.object.0.clone();
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
            claim.occurrence,
            Clause::new(
                NounPhrase::lexical(record.subject.0.clone()),
                VerbPhrase::new(record.relation.as_str(), complement),
            ),
        );
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
