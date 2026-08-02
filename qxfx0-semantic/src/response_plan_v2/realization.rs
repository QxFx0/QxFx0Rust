//! Certified realization boundary (ADR-0034 §7).

use serde::Serialize;
use sha2::{Digest, Sha256};

use super::assertion::AssertionAuthorizedPlan;
use super::snapshot::{RealizationSnapshot, SnapshotError};
use super::syn_tree::{
    RealizationCompletenessCertificate, RealizationError, ResolvedSynTree, SynTree,
};
use super::valency::ValencyLexicon;
use qxfx0_morphology::MorphologyRuntime;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RealizedSurface {
    pub clauses: Vec<String>,
    pub surface_digest: String,
    pub realization_snapshot_digest: String,
    pub completeness_digest: String,
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
