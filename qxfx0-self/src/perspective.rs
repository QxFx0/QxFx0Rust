//! Pure, bounded perspective registry and immutable projection builder.
//!
//! This is a conformance slice of the Haskell `PerspectiveRegistry` model.
//! It deliberately has no persistence or renderer integration: callers apply
//! explicit mutations and may only hand a [`PerspectiveProjection`] onward.

use std::collections::BTreeMap;

use qxfx0_types::{
    CautionLevel, ConfidenceBand, NormativeProfileId, PerspectiveDecision, PerspectiveId,
    PerspectiveMutation, PerspectiveProjection, PerspectiveScope, PerspectiveStatus,
    PerspectiveVersion,
};
use sha2::{Digest, Sha256};

const MAX_SUMMARY_CHARS: usize = 180;
const MAX_PUBLIC_REFERENCES: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct PerspectiveRegistryConfig {
    pub max_active_perspectives: usize,
    pub max_revisions_per_scope: usize,
    pub max_inactive_versions: usize,
}

impl Default for PerspectiveRegistryConfig {
    fn default() -> Self {
        Self {
            max_active_perspectives: 16,
            max_revisions_per_scope: 12,
            max_inactive_versions: 8,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct EndorsedPerspective {
    id: PerspectiveId,
    version: PerspectiveVersion,
    scope: PerspectiveScope,
    thesis: String,
    orientation: String,
    confidence: f64,
    normative_profile_id: NormativeProfileId,
    normative_profile_version: u64,
    status: PerspectiveStatus,
    evidence: Vec<String>,
    counterarguments: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RevisionRecord {
    from_version: Option<PerspectiveVersion>,
    to_version: PerspectiveVersion,
    decision: PerspectiveDecision,
}

#[derive(Debug, Clone, PartialEq)]
struct PerspectiveThread {
    id: PerspectiveId,
    active_version: Option<PerspectiveVersion>,
    versions: Vec<EndorsedPerspective>,
    revisions: Vec<RevisionRecord>,
    status: PerspectiveStatus,
    last_updated_turn: u64,
}

/// A deterministic, bounded registry. Its raw threads are private on purpose:
/// presentation must use `build_projection` / `build_active_projections`.
#[derive(Debug, Clone, PartialEq)]
pub struct PerspectiveRegistry {
    config: PerspectiveRegistryConfig,
    threads: BTreeMap<PerspectiveScope, PerspectiveThread>,
    next_perspective_ordinal: u64,
    next_version_ordinal: u64,
    last_updated_turn: u64,
}

impl Default for PerspectiveRegistry {
    fn default() -> Self {
        Self::new(PerspectiveRegistryConfig::default())
    }
}

impl PerspectiveRegistry {
    pub fn new(config: PerspectiveRegistryConfig) -> Self {
        Self {
            config,
            threads: BTreeMap::new(),
            next_perspective_ordinal: 1,
            next_version_ordinal: 1,
            last_updated_turn: 0,
        }
    }

    pub fn last_updated_turn(&self) -> u64 {
        self.last_updated_turn
    }

    pub fn thread_count(&self) -> usize {
        self.threads.len()
    }

    /// Applies an explicit decision without external effects.
    pub fn apply(mut self, mutation: &PerspectiveMutation) -> Self {
        self.last_updated_turn = mutation.turn;
        match mutation.decision {
            PerspectiveDecision::ObserveOnly => self,
            PerspectiveDecision::Quarantine | PerspectiveDecision::AcceptBounded => {
                self.upsert_contested(mutation)
            }
            PerspectiveDecision::PromoteEndorsed | PerspectiveDecision::ReviseActive => {
                self.upsert_active(mutation)
            }
            PerspectiveDecision::SuspendActive => self.suspend(mutation),
            PerspectiveDecision::RollbackPrior => self.rollback(mutation),
        }
    }

    /// Builds an immutable DTO from the active endorsed version only.
    pub fn build_projection(&self, scope: &PerspectiveScope) -> Option<PerspectiveProjection> {
        let thread = self.threads.get(scope)?;
        let active = thread.active_version?;
        let endorsed = thread.versions.iter().find(|version| {
            version.version == active && version.status == PerspectiveStatus::Active
        })?;

        Some(PerspectiveProjection {
            scope: endorsed.scope.clone(),
            summary: truncate_chars(&endorsed.thesis, MAX_SUMMARY_CHARS),
            orientation: endorsed.orientation.clone(),
            confidence_band: confidence_band(endorsed.confidence),
            caution_level: caution_level(endorsed),
            contested: matches!(
                endorsed.status,
                PerspectiveStatus::Contested | PerspectiveStatus::Suspended
            ),
            perspective_version: endorsed.version,
            normative_profile_id: endorsed.normative_profile_id.clone(),
            normative_profile_version: endorsed.normative_profile_version,
            evidence_count: endorsed.evidence.len(),
            counterargument_count: endorsed.counterarguments.len(),
            explanation_handle: format!(
                "{}:v{}:np{}",
                endorsed.id.0, endorsed.version.0, endorsed.normative_profile_version
            ),
        })
    }

    /// Active projections are newest-first and bounded by the registry cap.
    pub fn build_active_projections(&self) -> Vec<PerspectiveProjection> {
        let mut scopes: Vec<_> = self
            .threads
            .iter()
            .filter(|(_, thread)| thread.active_version.is_some())
            .map(|(scope, thread)| (scope, thread.last_updated_turn))
            .collect();
        scopes.sort_by(|(left_scope, left_turn), (right_scope, right_turn)| {
            right_turn
                .cmp(left_turn)
                .then_with(|| left_scope.cmp(right_scope))
        });
        scopes
            .into_iter()
            .take(self.config.max_active_perspectives)
            .filter_map(|(scope, _)| self.build_projection(scope))
            .collect()
    }

    /// Canonical digest over the replay-visible lineage, independent of memory
    /// addresses and map insertion order.
    pub fn replay_digest(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(format!("last:{}|", self.last_updated_turn));
        for (scope, thread) in &self.threads {
            hasher.update(scope.render());
            hasher.update(format!(
                "|id:{}|active:{:?}|turn:{}|",
                thread.id.0, thread.active_version, thread.last_updated_turn
            ));
            for version in &thread.versions {
                hasher.update(format!(
                    "v:{}|{:?}|{}|{}|{:.17}|{}|{}|{:?}|{:?}|",
                    version.version.0,
                    version.status,
                    version.thesis,
                    version.orientation,
                    version.confidence,
                    version.normative_profile_id.0,
                    version.normative_profile_version,
                    version.evidence,
                    version.counterarguments
                ));
            }
            for revision in &thread.revisions {
                hasher.update(format!(
                    "r:{:?}>{}|{:?}|",
                    revision.from_version, revision.to_version.0, revision.decision
                ));
            }
        }
        format!("sha256:{:x}", hasher.finalize())
    }

    fn upsert_active(mut self, mutation: &PerspectiveMutation) -> Self {
        let existing = self.threads.remove(&mutation.scope);
        let (id, prior_active, mut versions, mut revisions) = match existing {
            Some(thread) => (
                thread.id,
                thread.active_version,
                thread.versions,
                thread.revisions,
            ),
            None => {
                let id = PerspectiveId(format!("perspective-{}", self.next_perspective_ordinal));
                self.next_perspective_ordinal += 1;
                (id, None, Vec::new(), Vec::new())
            }
        };
        for version in &mut versions {
            if version.status == PerspectiveStatus::Active {
                version.status = PerspectiveStatus::Revised;
            }
        }
        let version = PerspectiveVersion(self.next_version_ordinal);
        self.next_version_ordinal += 1;
        versions.insert(
            0,
            EndorsedPerspective {
                id: id.clone(),
                version,
                scope: mutation.scope.clone(),
                thesis: mutation.thesis.clone(),
                orientation: mutation.orientation.clone(),
                confidence: mutation.confidence.clamp(0.0, 1.0),
                normative_profile_id: mutation.normative_profile_id.clone(),
                normative_profile_version: mutation.normative_profile_version,
                status: PerspectiveStatus::Active,
                evidence: mutation
                    .evidence
                    .iter()
                    .take(MAX_PUBLIC_REFERENCES)
                    .cloned()
                    .collect(),
                counterarguments: mutation
                    .counterarguments
                    .iter()
                    .take(MAX_PUBLIC_REFERENCES)
                    .cloned()
                    .collect(),
            },
        );
        revisions.insert(
            0,
            RevisionRecord {
                from_version: prior_active,
                to_version: version,
                decision: mutation.decision,
            },
        );
        revisions.truncate(self.config.max_revisions_per_scope);
        bound_versions(&mut versions, self.config.max_inactive_versions);
        self.threads.insert(
            mutation.scope.clone(),
            PerspectiveThread {
                id,
                active_version: Some(version),
                versions,
                revisions,
                status: PerspectiveStatus::Active,
                last_updated_turn: mutation.turn,
            },
        );
        self.enforce_active_cap()
    }

    fn upsert_contested(mut self, mutation: &PerspectiveMutation) -> Self {
        let existing = self.threads.remove(&mutation.scope);
        let (id, active_version, mut versions, mut revisions) = match existing {
            Some(thread) => (
                thread.id,
                thread.active_version,
                thread.versions,
                thread.revisions,
            ),
            None => {
                let id = PerspectiveId(format!("perspective-{}", self.next_perspective_ordinal));
                self.next_perspective_ordinal += 1;
                (id, None, Vec::new(), Vec::new())
            }
        };
        let version = PerspectiveVersion(self.next_version_ordinal);
        self.next_version_ordinal += 1;
        versions.insert(
            0,
            EndorsedPerspective {
                id: id.clone(),
                version,
                scope: mutation.scope.clone(),
                thesis: mutation.thesis.clone(),
                orientation: mutation.orientation.clone(),
                confidence: mutation.confidence.clamp(0.0, 1.0),
                normative_profile_id: mutation.normative_profile_id.clone(),
                normative_profile_version: mutation.normative_profile_version,
                status: PerspectiveStatus::Contested,
                evidence: mutation
                    .evidence
                    .iter()
                    .take(MAX_PUBLIC_REFERENCES)
                    .cloned()
                    .collect(),
                counterarguments: mutation
                    .counterarguments
                    .iter()
                    .take(MAX_PUBLIC_REFERENCES)
                    .cloned()
                    .collect(),
            },
        );
        revisions.insert(
            0,
            RevisionRecord {
                from_version: active_version,
                to_version: version,
                decision: mutation.decision,
            },
        );
        revisions.truncate(self.config.max_revisions_per_scope);
        bound_versions(&mut versions, self.config.max_inactive_versions);
        self.threads.insert(
            mutation.scope.clone(),
            PerspectiveThread {
                id,
                active_version,
                versions,
                revisions,
                status: PerspectiveStatus::Contested,
                last_updated_turn: mutation.turn,
            },
        );
        self.enforce_active_cap()
    }

    fn suspend(mut self, mutation: &PerspectiveMutation) -> Self {
        let Some(thread) = self.threads.get_mut(&mutation.scope) else {
            return self;
        };
        let active = thread.active_version.take();
        for version in &mut thread.versions {
            if Some(version.version) == active {
                version.status = PerspectiveStatus::Suspended;
            }
        }
        if let Some(version) = active {
            thread.revisions.insert(
                0,
                RevisionRecord {
                    from_version: Some(version),
                    to_version: version,
                    decision: mutation.decision,
                },
            );
            thread
                .revisions
                .truncate(self.config.max_revisions_per_scope);
        }
        thread.status = PerspectiveStatus::Suspended;
        thread.last_updated_turn = mutation.turn;
        self
    }

    fn rollback(mut self, mutation: &PerspectiveMutation) -> Self {
        let Some(thread) = self.threads.get_mut(&mutation.scope) else {
            return self;
        };
        let prior_active = thread.active_version;
        let replacement = thread
            .versions
            .iter()
            .find(|version| {
                Some(version.version) != prior_active
                    && matches!(
                        version.status,
                        PerspectiveStatus::Revised | PerspectiveStatus::Contested
                    )
            })
            .map(|version| version.version);
        for version in &mut thread.versions {
            if Some(version.version) == prior_active {
                version.status = PerspectiveStatus::Withdrawn;
            }
            if Some(version.version) == replacement {
                version.status = PerspectiveStatus::Active;
            }
        }
        thread.active_version = replacement;
        thread.status = if replacement.is_some() {
            PerspectiveStatus::Active
        } else {
            PerspectiveStatus::Withdrawn
        };
        thread.last_updated_turn = mutation.turn;
        if let Some(to_version) = replacement {
            thread.revisions.insert(
                0,
                RevisionRecord {
                    from_version: prior_active,
                    to_version,
                    decision: mutation.decision,
                },
            );
            thread
                .revisions
                .truncate(self.config.max_revisions_per_scope);
        }
        self
    }

    fn enforce_active_cap(mut self) -> Self {
        let mut active: Vec<_> = self
            .threads
            .iter()
            .filter_map(|(scope, thread)| {
                thread
                    .active_version
                    .map(|_| (scope.clone(), thread.last_updated_turn))
            })
            .collect();
        active.sort_by(|(left_scope, left_turn), (right_scope, right_turn)| {
            right_turn
                .cmp(left_turn)
                .then_with(|| left_scope.cmp(right_scope))
        });
        for (scope, _) in active.into_iter().skip(self.config.max_active_perspectives) {
            let thread = self
                .threads
                .get_mut(&scope)
                .expect("scope was collected from threads");
            let active_version = thread.active_version.take();
            for version in &mut thread.versions {
                if Some(version.version) == active_version {
                    version.status = PerspectiveStatus::Suspended;
                }
            }
            thread.status = PerspectiveStatus::Suspended;
        }
        self
    }
}

fn bound_versions(versions: &mut Vec<EndorsedPerspective>, max_inactive: usize) {
    let mut retained_inactive = 0;
    versions.retain(|version| {
        if version.status == PerspectiveStatus::Active {
            true
        } else {
            retained_inactive += 1;
            retained_inactive <= max_inactive
        }
    });
}

fn confidence_band(confidence: f64) -> ConfidenceBand {
    if confidence >= 0.80 {
        ConfidenceBand::High
    } else if confidence >= 0.60 {
        ConfidenceBand::Medium
    } else if confidence >= 0.40 {
        ConfidenceBand::Low
    } else {
        ConfidenceBand::Minimal
    }
}

fn caution_level(endorsed: &EndorsedPerspective) -> CautionLevel {
    if matches!(
        endorsed.status,
        PerspectiveStatus::Contested | PerspectiveStatus::Suspended
    ) {
        CautionLevel::High
    } else if endorsed.confidence < 0.60 || !endorsed.counterarguments.is_empty() {
        CautionLevel::Medium
    } else {
        CautionLevel::Low
    }
}

fn truncate_chars(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope(name: &str) -> PerspectiveScope {
        PerspectiveScope::Topic(name.into())
    }

    fn mutation(
        turn: u64,
        scope: PerspectiveScope,
        decision: PerspectiveDecision,
        thesis: &str,
    ) -> PerspectiveMutation {
        PerspectiveMutation {
            turn,
            scope,
            decision,
            thesis: thesis.into(),
            orientation: "reflective".into(),
            confidence: 0.72,
            normative_profile_id: NormativeProfileId("default".into()),
            normative_profile_version: 1,
            evidence: vec![
                "knowledge:bounded agency".into(),
                "dialogue:stable topic".into(),
            ],
            counterarguments: vec![],
        }
    }

    #[test]
    fn reference_vector_observe_only_does_not_allocate_lineage() {
        let registry = PerspectiveRegistry::default().apply(&mutation(
            9,
            scope("freedom"),
            PerspectiveDecision::ObserveOnly,
            "observed only",
        ));
        assert_eq!(registry.thread_count(), 0);
        assert_eq!(registry.last_updated_turn(), 9);
        assert!(registry.build_projection(&scope("freedom")).is_none());
    }

    #[test]
    fn reference_vector_promotion_and_revision_keep_single_active_version() {
        let registry = PerspectiveRegistry::default()
            .apply(&mutation(
                1,
                scope("freedom"),
                PerspectiveDecision::PromoteEndorsed,
                "initial thesis",
            ))
            .apply(&mutation(
                2,
                scope("freedom"),
                PerspectiveDecision::ReviseActive,
                "revised thesis",
            ));
        let projection = registry
            .build_projection(&scope("freedom"))
            .expect("active projection");
        assert_eq!(projection.summary, "revised thesis");
        assert_eq!(projection.perspective_version, PerspectiveVersion(2));
        assert_eq!(projection.explanation_handle, "perspective-1:v2:np1");
    }

    #[test]
    fn reference_vector_quarantine_retains_lineage_without_projection() {
        let registry = PerspectiveRegistry::default().apply(&mutation(
            1,
            scope("freedom"),
            PerspectiveDecision::Quarantine,
            "unresolved freedom thesis",
        ));
        assert_eq!(registry.thread_count(), 1);
        assert!(registry.build_projection(&scope("freedom")).is_none());
        assert!(registry.build_active_projections().is_empty());
    }

    #[test]
    fn reference_vector_active_cap_suspends_older_scope_without_deleting_lineage() {
        let registry = PerspectiveRegistry::new(PerspectiveRegistryConfig {
            max_active_perspectives: 1,
            ..Default::default()
        })
        .apply(&mutation(
            1,
            scope("freedom"),
            PerspectiveDecision::PromoteEndorsed,
            "freedom",
        ))
        .apply(&mutation(
            2,
            scope("responsibility"),
            PerspectiveDecision::PromoteEndorsed,
            "responsibility",
        ));
        assert_eq!(registry.thread_count(), 2);
        assert!(registry.build_projection(&scope("freedom")).is_none());
        assert_eq!(
            registry
                .build_active_projections()
                .into_iter()
                .map(|p| p.scope)
                .collect::<Vec<_>>(),
            vec![scope("responsibility")]
        );
    }

    #[test]
    fn projection_caps_public_surface_and_never_exposes_raw_references() {
        let mut rich = mutation(
            1,
            scope("freedom"),
            PerspectiveDecision::PromoteEndorsed,
            &"x".repeat(220),
        );
        rich.evidence = (0..12).map(|index| format!("evidence-{index}")).collect();
        rich.counterarguments = (0..12).map(|index| format!("counter-{index}")).collect();
        let projection = PerspectiveRegistry::default()
            .apply(&rich)
            .build_projection(&scope("freedom"))
            .unwrap();
        assert_eq!(projection.summary.chars().count(), MAX_SUMMARY_CHARS);
        assert_eq!(projection.evidence_count, MAX_PUBLIC_REFERENCES);
        assert_eq!(projection.counterargument_count, MAX_PUBLIC_REFERENCES);
    }

    #[test]
    fn replay_digest_is_stable_for_equivalent_mutation_vectors() {
        let vector = [
            mutation(
                1,
                scope("freedom"),
                PerspectiveDecision::PromoteEndorsed,
                "initial",
            ),
            mutation(
                2,
                scope("freedom"),
                PerspectiveDecision::ReviseActive,
                "revision",
            ),
            mutation(
                3,
                scope("responsibility"),
                PerspectiveDecision::PromoteEndorsed,
                "responsibility",
            ),
        ];
        let first = vector
            .iter()
            .fold(PerspectiveRegistry::default(), |registry, item| {
                registry.apply(item)
            });
        let second = vector
            .iter()
            .fold(PerspectiveRegistry::default(), |registry, item| {
                registry.apply(item)
            });
        assert_eq!(first.replay_digest(), second.replay_digest());
        assert_eq!(
            first.build_active_projections(),
            second.build_active_projections()
        );
    }
}
