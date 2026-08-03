//! Certified realization boundary (ADR-0034 §7).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

use super::assertion::AssertionAuthorizedPlan;
use super::snapshot::{RealizationSnapshot, SnapshotError};
use super::syn_tree::{
    RealizationCompletenessCertificate, RealizationError, ResolvedSynTree, SynTree,
};
use super::valency::ValencyLexicon;
use qxfx0_morphology::MorphologyRuntime;

pub const REALIZATION_JOINER_VERSION: &str = "punctuated-space-v1";

/// Joins arbitrary clause surfaces without relying on a renderer or locale.
/// Empty clauses are omitted; existing terminal punctuation is preserved.
pub fn join_realized_clauses<I, S>(clauses: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    clauses
        .into_iter()
        .map(|clause| clause.as_ref().trim().to_string())
        .filter(|clause| !clause.is_empty())
        .map(|mut clause| {
            if !clause.ends_with(['.', '!', '?']) {
                clause.push('.');
            }
            clause
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RealizablePlan {
    authorized: AssertionAuthorizedPlan,
    resolved_syn_tree: ResolvedSynTree,
    realization_snapshot_digest: String,
    completeness_certificate: RealizationCompletenessCertificate,
}

impl RealizablePlan {
    pub fn authorized(&self) -> &AssertionAuthorizedPlan {
        &self.authorized
    }
    pub fn resolved_syn_tree(&self) -> &ResolvedSynTree {
        &self.resolved_syn_tree
    }
    pub fn realization_snapshot_digest(&self) -> &str {
        &self.realization_snapshot_digest
    }
    pub fn completeness_certificate(&self) -> &RealizationCompletenessCertificate {
        &self.completeness_certificate
    }
}

pub fn try_realize(
    authorized: AssertionAuthorizedPlan,
    syn_tree: &SynTree,
    snapshot: &RealizationSnapshot,
    lexicon: &ValencyLexicon,
    morphology: &MorphologyRuntime,
) -> Result<RealizablePlan, RealizationError> {
    let expected = authorized
        .certified()
        .candidate()
        .projected_claims()
        .into_iter()
        .map(|claim| occurrence_label(&claim.occurrence))
        .collect::<Vec<_>>();
    let actual = syn_tree
        .iter()
        .map(|(occurrence, _)| occurrence_label(occurrence))
        .collect::<Vec<_>>();
    let expected_set = expected.iter().collect::<BTreeSet<_>>();
    let actual_set = actual.iter().collect::<BTreeSet<_>>();
    if expected.len() != actual.len() || expected_set != actual_set {
        return Err(RealizationError::OccurrenceMismatch { expected, actual });
    }
    let resolved = super::syn_tree::resolve(syn_tree, lexicon, morphology)?;
    let certificate = resolved.certificate().clone();
    if certificate.valency_fingerprint != snapshot.valency_digest
        || certificate.morphology_sha256 != snapshot.morphology_digest
        || certificate.morphology_depth_fingerprint != snapshot.morphology_depth_digest
    {
        return Err(RealizationError::SnapshotMismatch {
            expected: snapshot.fingerprint.clone(),
            actual: realization_components_digest(&certificate),
        });
    }
    Ok(RealizablePlan {
        authorized,
        resolved_syn_tree: resolved,
        realization_snapshot_digest: snapshot.fingerprint.clone(),
        completeness_certificate: certificate,
    })
}

fn occurrence_label(occurrence: &super::discourse::DiscourseOccurrenceId) -> String {
    format!(
        "{}:{}",
        occurrence.discourse_root_digest(),
        occurrence.canonical_path()
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RealizedSurface {
    pub clauses: Vec<String>,
    pub surface_digest: String,
    pub realization_snapshot_digest: String,
    pub completeness_digest: String,
}

impl RealizedSurface {
    pub fn joined(&self) -> String {
        join_realized_clauses(&self.clauses)
    }
}

pub fn linearize(
    plan: &RealizablePlan,
    snapshot: &RealizationSnapshot,
) -> Result<RealizedSurface, SnapshotError> {
    if plan.realization_snapshot_digest != snapshot.fingerprint {
        return Err(SnapshotError::RealizationSnapshotMismatch);
    }
    let clauses = plan.resolved_syn_tree.linearize();
    let surface_digest = digest(b"qxfx0:realized-surface:v1", &clauses);
    let completeness_digest = digest(
        b"qxfx0:realization-completeness:v1",
        plan.resolved_syn_tree.certificate(),
    );
    Ok(RealizedSurface {
        clauses,
        surface_digest,
        realization_snapshot_digest: snapshot.fingerprint.clone(),
        completeness_digest,
    })
}

fn realization_components_digest(certificate: &RealizationCompletenessCertificate) -> String {
    format!(
        "{}:{}:{}",
        certificate.valency_fingerprint,
        certificate.morphology_sha256,
        certificate.morphology_depth_fingerprint
    )
}

fn digest<T: Serialize>(domain: &[u8], value: &T) -> String {
    let encoded = serde_json::to_vec(value).expect("surface serializes");
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update((encoded.len() as u64).to_be_bytes());
    hasher.update(encoded);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::response_plan_v2::{
        build_audited_topic, preposition_allomorphs, valency_lexicon, SynTree,
    };

    fn snapshot() -> RealizationSnapshot {
        RealizationSnapshot::new(
            valency_lexicon().fingerprint(),
            "audited-corpus-grammar-v1",
            qxfx0_morphology::get_runtime().lexemes_sha256(),
            preposition_allomorphs().fingerprint(),
        )
    }

    #[test]
    fn every_authorized_occurrence_requires_exactly_one_syntax_node() {
        let plan = build_audited_topic("свобода").expect("audited topic");
        let authorized = plan.authorized().clone();
        let full = plan.syn_tree(valency_lexicon()).expect("syntax adapter");

        assert!(matches!(
            try_realize(
                authorized.clone(),
                &SynTree::new(),
                &snapshot(),
                valency_lexicon(),
                qxfx0_morphology::get_runtime(),
            ),
            Err(RealizationError::OccurrenceMismatch { .. })
        ));

        let mut duplicated = full.clone();
        let (occurrence, node) = full.iter().next().expect("syntax node").clone();
        duplicated.push_node(occurrence, node);
        assert!(matches!(
            try_realize(
                authorized,
                &duplicated,
                &snapshot(),
                valency_lexicon(),
                qxfx0_morphology::get_runtime(),
            ),
            Err(RealizationError::OccurrenceMismatch { .. })
        ));
    }

    #[test]
    fn audited_plan_resolves_clause_and_fixed_claim_nodes() {
        let plan = build_audited_topic("свобода").expect("audited topic");
        let tree = plan.syn_tree(valency_lexicon()).expect("syntax adapter");
        let realized = try_realize(
            plan.into_authorized(),
            &tree,
            &snapshot(),
            valency_lexicon(),
            qxfx0_morphology::get_runtime(),
        )
        .expect("complete realization");

        assert_eq!(realized.completeness_certificate().clauses, 1);
        assert_eq!(realized.completeness_certificate().fixed_nodes, 2);
        assert_eq!(realized.resolved_syn_tree().linearize().len(), 3);
    }

    #[test]
    fn joiner_is_total_and_punctuation_is_deterministic() {
        assert_eq!(join_realized_clauses(Vec::<String>::new()), "");
        assert_eq!(
            join_realized_clauses([" first ", "", "second!", "third?"]),
            "first. second! third?"
        );
    }
}
