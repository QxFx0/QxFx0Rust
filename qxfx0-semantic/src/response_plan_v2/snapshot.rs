//! Four-domain turn snapshots and replay verification (ADR-0034 §8).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::derivation::InferenceRuleId;
use super::selection::{
    ResponsePlanV2Mode, SelectionPolicy, SelectionReceipt, NUMERIC_SEMANTICS_VERSION,
    RANKING_VERSION,
};
use super::syn_tree::ResolvedSynNode;
use super::{RealizablePlan, RealizedSurface, REALIZATION_JOINER_VERSION};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthoritySnapshot {
    pub pack_set_digest: String,
    pub inference_rules_digest: String,
    pub assertion_policy_digest: String,
    pub fingerprint: String,
}

impl AuthoritySnapshot {
    pub fn new(
        pack_set_digest: impl Into<String>,
        assertion_policy_digest: impl Into<String>,
    ) -> Self {
        let mut value = Self {
            pack_set_digest: pack_set_digest.into(),
            inference_rules_digest: inference_rule_set_digest(),
            assertion_policy_digest: assertion_policy_digest.into(),
            fingerprint: String::new(),
        };
        value.fingerprint = fingerprint(b"qxfx0:authority-snapshot:v1", &value);
        value
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanningPolicySnapshot {
    pub budgets_digest: String,
    pub canonicalization_version: String,
    pub fingerprint: String,
}

impl PlanningPolicySnapshot {
    pub fn new(
        budgets_digest: impl Into<String>,
        canonicalization_version: impl Into<String>,
    ) -> Self {
        let mut value = Self {
            budgets_digest: budgets_digest.into(),
            canonicalization_version: canonicalization_version.into(),
            fingerprint: String::new(),
        };
        value.fingerprint = fingerprint(b"qxfx0:planning-snapshot:v1", &value);
        value
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RealizationSnapshot {
    pub valency_digest: String,
    pub grammar_digest: String,
    pub morphology_digest: String,
    pub morphology_depth_digest: String,
    pub joiner_version: String,
    pub fingerprint: String,
}

impl RealizationSnapshot {
    pub fn new(
        valency_digest: impl Into<String>,
        grammar_digest: impl Into<String>,
        morphology_digest: impl Into<String>,
        morphology_depth_digest: impl Into<String>,
    ) -> Self {
        let grammar_digest = domain_digest(
            b"qxfx0:realization-grammar:v2",
            &(grammar_digest.into(), REALIZATION_JOINER_VERSION),
        );
        let mut value = Self {
            valency_digest: valency_digest.into(),
            grammar_digest,
            morphology_digest: morphology_digest.into(),
            morphology_depth_digest: morphology_depth_digest.into(),
            joiner_version: REALIZATION_JOINER_VERSION.to_string(),
            fingerprint: String::new(),
        };
        value.fingerprint = fingerprint(b"qxfx0:realization-snapshot:v1", &value);
        value
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionPolicySnapshot {
    pub self_policy_digest: String,
    pub response_plan_v2_mode: ResponsePlanV2Mode,
    pub ranking_version: String,
    pub numeric_semantics_version: String,
    pub fingerprint: String,
}

impl SelectionPolicySnapshot {
    pub fn new(policy: SelectionPolicy) -> Self {
        let mut value = Self {
            self_policy_digest: policy.digest(),
            response_plan_v2_mode: policy.response_plan_v2_mode,
            ranking_version: RANKING_VERSION.to_string(),
            numeric_semantics_version: NUMERIC_SEMANTICS_VERSION.to_string(),
            fingerprint: String::new(),
        };
        value.fingerprint = fingerprint(b"qxfx0:selection-snapshot:v1", &value);
        value
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnContractSnapshot {
    pub authority: AuthoritySnapshot,
    pub planning: PlanningPolicySnapshot,
    pub realization: RealizationSnapshot,
    pub selection: SelectionPolicySnapshot,
    pub digest: String,
}

impl TurnContractSnapshot {
    pub fn new(
        authority: AuthoritySnapshot,
        planning: PlanningPolicySnapshot,
        realization: RealizationSnapshot,
        selection: SelectionPolicySnapshot,
    ) -> Self {
        let mut value = Self {
            authority,
            planning,
            realization,
            selection,
            digest: String::new(),
        };
        value.digest = fingerprint(b"qxfx0:turn-contract-snapshot:v1", &value);
        value
    }

    pub fn verify_integrity(&self) -> Result<(), SnapshotError> {
        verify_fingerprint(b"qxfx0:authority-snapshot:v1", &self.authority)?;
        verify_fingerprint(b"qxfx0:planning-snapshot:v1", &self.planning)?;
        verify_fingerprint(b"qxfx0:realization-snapshot:v1", &self.realization)?;
        verify_fingerprint(b"qxfx0:selection-snapshot:v1", &self.selection)?;
        let actual = fingerprint(b"qxfx0:turn-contract-snapshot:v1", self);
        if actual != self.digest {
            return Err(SnapshotError::IntegrityMismatch {
                domain: "turn_contract",
                expected: self.digest.clone(),
                actual,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayInputEnvelope {
    pub topic: String,
    pub logical_turn: u64,
    pub authority_as_of: Option<String>,
}

/// Fully selected realization material. Reproduction concatenates only these
/// captured values and never consults a process-global pack, lexicon or
/// morphology runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapturedRealizationNode {
    Clause {
        discourse_root_digest: String,
        canonical_path: String,
        subject_surface: String,
        head_surface: String,
        preposition: Option<String>,
        complement_surface: Option<String>,
    },
    FixedPhrase {
        discourse_root_digest: String,
        canonical_path: String,
        surface: String,
    },
}

impl CapturedRealizationNode {
    fn linearize(&self) -> String {
        match self {
            Self::Clause {
                subject_surface,
                head_surface,
                preposition,
                complement_surface,
                ..
            } => {
                let mut parts = vec![subject_surface.clone(), head_surface.clone()];
                if let Some(preposition) = preposition {
                    parts.push(preposition.clone());
                }
                if let Some(complement) = complement_surface {
                    parts.push(complement.clone());
                }
                parts.join(" ")
            }
            Self::FixedPhrase { surface, .. } => surface.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExactReplayBundle {
    pub schema_version: u32,
    pub input: ReplayInputEnvelope,
    pub contract_digest: String,
    pub candidate_merkle_root: String,
    pub nodes: Vec<CapturedRealizationNode>,
    pub expected_surface: RealizedSurface,
    pub bundle_digest: String,
}

impl ExactReplayBundle {
    pub fn new(
        input: ReplayInputEnvelope,
        contract_digest: impl Into<String>,
        candidate_merkle_root: impl Into<String>,
        nodes: Vec<CapturedRealizationNode>,
        expected_surface: RealizedSurface,
    ) -> Self {
        let mut value = Self {
            schema_version: 2,
            input,
            contract_digest: contract_digest.into(),
            candidate_merkle_root: candidate_merkle_root.into(),
            nodes,
            expected_surface,
            bundle_digest: String::new(),
        };
        value.bundle_digest = replay_bundle_fingerprint(&value);
        value
    }

    pub fn capture(
        input: ReplayInputEnvelope,
        contract: &TurnContractSnapshot,
        selection: &SelectionReceipt,
        plan: &RealizablePlan,
        surface: RealizedSurface,
    ) -> Self {
        let nodes = plan
            .resolved_syn_tree()
            .nodes()
            .iter()
            .map(|node| match node {
                ResolvedSynNode::Clause(clause) => CapturedRealizationNode::Clause {
                    discourse_root_digest: clause.occurrence.discourse_root_digest().to_string(),
                    canonical_path: clause.occurrence.canonical_path().to_string(),
                    subject_surface: clause.subject_surface.clone(),
                    head_surface: clause.head_surface.clone(),
                    preposition: clause.preposition.clone(),
                    complement_surface: clause.complement_surface.clone(),
                },
                ResolvedSynNode::FixedPhrase {
                    occurrence,
                    surface,
                } => CapturedRealizationNode::FixedPhrase {
                    discourse_root_digest: occurrence.discourse_root_digest().to_string(),
                    canonical_path: occurrence.canonical_path().to_string(),
                    surface: surface.clone(),
                },
            })
            .collect();
        Self::new(
            input,
            &contract.digest,
            &selection.candidate_merkle_root,
            nodes,
            surface,
        )
    }

    fn verify_integrity(&self) -> Result<(), SnapshotError> {
        let actual = replay_bundle_fingerprint(self);
        if self.schema_version != 2 || actual != self.bundle_digest {
            return Err(SnapshotError::IntegrityMismatch {
                domain: "exact_replay_bundle",
                expected: self.bundle_digest.clone(),
                actual,
            });
        }
        Ok(())
    }

    pub fn reproduce(&self) -> Result<RealizedSurface, SnapshotError> {
        self.verify_integrity()?;
        let clauses = self
            .nodes
            .iter()
            .map(CapturedRealizationNode::linearize)
            .collect::<Vec<_>>();
        let surface_digest = domain_digest(b"qxfx0:realized-surface:v1", &clauses);
        let reproduced = RealizedSurface {
            clauses,
            surface_digest,
            realization_snapshot_digest: self.expected_surface.realization_snapshot_digest.clone(),
            completeness_digest: self.expected_surface.completeness_digest.clone(),
        };
        if reproduced != self.expected_surface {
            return Err(SnapshotError::ReproducedSurfaceMismatch);
        }
        Ok(reproduced)
    }
}

/// Replay-stable V2 turn record. Local timing and host state are absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnRecord {
    pub schema_version: u32,
    pub contract: TurnContractSnapshot,
    pub selection: SelectionReceipt,
    pub binary_digest: String,
    pub exact_replay: ExactReplayBundle,
    pub stage_digest: String,
}

impl TurnRecord {
    pub fn new(
        contract: TurnContractSnapshot,
        selection: SelectionReceipt,
        binary_digest: impl Into<String>,
        exact_replay: ExactReplayBundle,
    ) -> Self {
        let mut value = Self {
            schema_version: 2,
            contract,
            selection,
            binary_digest: binary_digest.into(),
            exact_replay,
            stage_digest: String::new(),
        };
        value.stage_digest = fingerprint(b"qxfx0:v2-turn-record:v2", &value);
        value
    }

    fn verify_integrity(&self) -> Result<(), SnapshotError> {
        self.contract.verify_integrity()?;
        self.exact_replay.verify_integrity()?;
        if self.schema_version != 2
            || self.exact_replay.contract_digest != self.contract.digest
            || self.exact_replay.candidate_merkle_root != self.selection.candidate_merkle_root
        {
            return Err(SnapshotError::ReplayBundleMismatch);
        }
        if self.selection.policy_digest != self.contract.selection.self_policy_digest
            || self.selection.ranking_version != self.contract.selection.ranking_version
            || self.selection.numeric_semantics_version
                != self.contract.selection.numeric_semantics_version
        {
            return Err(SnapshotError::SelectionContractMismatch);
        }
        let actual = fingerprint(b"qxfx0:v2-turn-record:v2", self);
        if actual != self.stage_digest {
            return Err(SnapshotError::IntegrityMismatch {
                domain: "turn_record",
                expected: self.stage_digest.clone(),
                actual,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplayLevel {
    Integrity,
    Authority,
    Reproduction,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ReplayMaterials<'a> {
    pub authority: Option<&'a AuthoritySnapshot>,
    pub contract: Option<&'a TurnContractSnapshot>,
    pub binary_digest: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayVerification {
    pub level: ReplayLevel,
    pub turn_record_digest: String,
    pub reproduced_surface: Option<RealizedSurface>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SnapshotError {
    #[error("{domain} fingerprint mismatch: expected {expected}, actual {actual}")]
    IntegrityMismatch {
        domain: &'static str,
        expected: String,
        actual: String,
    },
    #[error("selection receipt does not match the selection snapshot")]
    SelectionContractMismatch,
    #[error("{level:?} replay materials are unavailable")]
    SnapshotUnavailable { level: ReplayLevel },
    #[error("authority snapshot mismatch")]
    AuthoritySnapshotMismatch,
    #[error("turn contract snapshot mismatch")]
    ContractSnapshotMismatch,
    #[error("planning policy snapshot mismatch")]
    PlanningPolicySnapshotMismatch,
    #[error("selection policy snapshot mismatch")]
    SelectionPolicySnapshotMismatch,
    #[error("reproduction binary mismatch")]
    BinaryMismatch,
    #[error("realization snapshot mismatch")]
    RealizationSnapshotMismatch,
    #[error("exact replay bundle does not match its turn record")]
    ReplayBundleMismatch,
    #[error("captured realization did not reproduce the expected surface")]
    ReproducedSurfaceMismatch,
}

pub fn verify_replay(
    record: &TurnRecord,
    level: ReplayLevel,
    materials: ReplayMaterials<'_>,
) -> Result<ReplayVerification, SnapshotError> {
    record.verify_integrity()?;
    if matches!(level, ReplayLevel::Authority | ReplayLevel::Reproduction) {
        let authority = materials
            .authority
            .ok_or(SnapshotError::SnapshotUnavailable { level })?;
        if authority != &record.contract.authority {
            return Err(SnapshotError::AuthoritySnapshotMismatch);
        }
    }
    let reproduced_surface = if level == ReplayLevel::Reproduction {
        let contract = materials
            .contract
            .ok_or(SnapshotError::SnapshotUnavailable { level })?;
        let binary_digest = materials
            .binary_digest
            .ok_or(SnapshotError::SnapshotUnavailable { level })?;
        if contract.authority != record.contract.authority {
            return Err(SnapshotError::AuthoritySnapshotMismatch);
        }
        if contract.planning != record.contract.planning {
            return Err(SnapshotError::PlanningPolicySnapshotMismatch);
        }
        if contract.realization != record.contract.realization {
            return Err(SnapshotError::RealizationSnapshotMismatch);
        }
        if contract.selection != record.contract.selection {
            return Err(SnapshotError::SelectionPolicySnapshotMismatch);
        }
        if contract.digest != record.contract.digest {
            return Err(SnapshotError::ContractSnapshotMismatch);
        }
        if binary_digest != record.binary_digest {
            return Err(SnapshotError::BinaryMismatch);
        }
        Some(record.exact_replay.reproduce()?)
    } else {
        None
    };
    Ok(ReplayVerification {
        level,
        turn_record_digest: record.stage_digest.clone(),
        reproduced_surface,
    })
}

pub fn inference_rule_set_digest() -> String {
    let rules = InferenceRuleId::ALL.map(InferenceRuleId::as_str);
    fingerprint(b"qxfx0:inference-rule-set:v1", &rules)
}

fn fingerprint<T: Serialize>(domain: &[u8], value: &T) -> String {
    let mut canonical = serde_json::to_value(value).expect("snapshot serializes");
    if let serde_json::Value::Object(fields) = &mut canonical {
        fields.remove("fingerprint");
        fields.remove("digest");
        fields.remove("stage_digest");
    }
    let encoded = serde_json::to_vec(&canonical).expect("canonical snapshot serializes");
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update((encoded.len() as u64).to_be_bytes());
    hasher.update(encoded);
    format!("{:x}", hasher.finalize())
}

fn replay_bundle_fingerprint(bundle: &ExactReplayBundle) -> String {
    let mut value = serde_json::to_value(bundle).expect("replay bundle serializes");
    value
        .as_object_mut()
        .expect("replay bundle is an object")
        .remove("bundle_digest");
    domain_digest(b"qxfx0:exact-replay-bundle:v2", &value)
}

fn domain_digest<T: Serialize>(domain: &[u8], value: &T) -> String {
    let encoded = serde_json::to_vec(value).expect("replay value serializes");
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update((encoded.len() as u64).to_be_bytes());
    hasher.update(encoded);
    format!("{:x}", hasher.finalize())
}

trait Fingerprinted: Serialize {
    fn domain_name(&self) -> &'static str;
    fn recorded_fingerprint(&self) -> &str;
}

macro_rules! fingerprinted {
    ($type:ty, $name:literal) => {
        impl Fingerprinted for $type {
            fn domain_name(&self) -> &'static str {
                $name
            }
            fn recorded_fingerprint(&self) -> &str {
                &self.fingerprint
            }
        }
    };
}

fingerprinted!(AuthoritySnapshot, "authority");
fingerprinted!(PlanningPolicySnapshot, "planning");
fingerprinted!(RealizationSnapshot, "realization");
fingerprinted!(SelectionPolicySnapshot, "selection");

fn verify_fingerprint<T: Fingerprinted>(domain: &[u8], value: &T) -> Result<(), SnapshotError> {
    let actual = fingerprint(domain, value);
    if actual != value.recorded_fingerprint() {
        return Err(SnapshotError::IntegrityMismatch {
            domain: value.domain_name(),
            expected: value.recorded_fingerprint().to_string(),
            actual,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::response_plan_v2::{
        preposition_allomorphs, valency_lexicon, AssertionPolicy, BasisPoints, SelfSelectionContext,
    };

    fn contract() -> TurnContractSnapshot {
        TurnContractSnapshot::new(
            AuthoritySnapshot::new("pack-digest", AssertionPolicy::v1().digest()),
            PlanningPolicySnapshot::new("budget-digest", "proposition-canon-v1"),
            RealizationSnapshot::new(
                valency_lexicon().fingerprint(),
                "clause-grammar-v1",
                qxfx0_morphology::get_runtime().lexemes_sha256(),
                preposition_allomorphs().fingerprint(),
            ),
            SelectionPolicySnapshot::new(SelectionPolicy::default()),
        )
    }

    fn record() -> TurnRecord {
        let contract = contract();
        let selection = SelectionReceipt {
            candidate_merkle_root: "candidate".into(),
            score: 42,
            context: SelfSelectionContext {
                conatus: BasisPoints::from_raw(12_000),
                salience: BasisPoints::from_raw(5_000),
                doubt: BasisPoints::from_raw(1_000),
            },
            policy_digest: contract.selection.self_policy_digest.clone(),
            ranking_version: contract.selection.ranking_version.clone(),
            numeric_semantics_version: contract.selection.numeric_semantics_version.clone(),
        };
        let clauses = vec!["истина зависит от разума".to_string()];
        let surface = RealizedSurface {
            surface_digest: domain_digest(b"qxfx0:realized-surface:v1", &clauses),
            clauses,
            realization_snapshot_digest: contract.realization.fingerprint.clone(),
            completeness_digest: "completeness".into(),
        };
        let bundle = ExactReplayBundle::new(
            ReplayInputEnvelope {
                topic: "истина".into(),
                logical_turn: 42,
                authority_as_of: None,
            },
            &contract.digest,
            &selection.candidate_merkle_root,
            vec![CapturedRealizationNode::Clause {
                discourse_root_digest: "root".into(),
                canonical_path: "0.thesis".into(),
                subject_surface: "истина".into(),
                head_surface: "зависит".into(),
                preposition: Some("от".into()),
                complement_surface: Some("разума".into()),
            }],
            surface,
        );
        TurnRecord::new(contract, selection, "binary-digest", bundle)
    }

    #[test]
    fn four_domains_have_independent_fingerprints() {
        let first = contract();
        let changed = TurnContractSnapshot::new(
            first.authority.clone(),
            PlanningPolicySnapshot::new("other-budget", "proposition-canon-v1"),
            first.realization.clone(),
            first.selection.clone(),
        );
        assert_eq!(first.authority.fingerprint, changed.authority.fingerprint);
        assert_ne!(first.planning.fingerprint, changed.planning.fingerprint);
        assert_ne!(first.digest, changed.digest);
    }

    #[test]
    fn snapshots_bind_v2_mode_and_joiner_version() {
        let policy = SelectionPolicy {
            response_plan_v2_mode: ResponsePlanV2Mode::Canary,
            ..SelectionPolicy::default()
        };
        let selection = SelectionPolicySnapshot::new(policy);
        assert_eq!(selection.response_plan_v2_mode, ResponsePlanV2Mode::Canary);

        let realization = RealizationSnapshot::new("valency", "grammar", "morph", "depth");
        assert_eq!(realization.joiner_version, REALIZATION_JOINER_VERSION);
        assert_ne!(realization.grammar_digest, "grammar");
    }

    #[test]
    fn integrity_needs_no_assets_and_detects_tampering() {
        let record = record();
        assert!(verify_replay(&record, ReplayLevel::Integrity, ReplayMaterials::default()).is_ok());

        let mut tampered = record;
        tampered.selection.score += 1;
        assert!(matches!(
            verify_replay(
                &tampered,
                ReplayLevel::Integrity,
                ReplayMaterials::default()
            ),
            Err(SnapshotError::IntegrityMismatch {
                domain: "turn_record",
                ..
            })
        ));
    }

    #[test]
    fn authority_and_reproduction_fail_closed_without_materials() {
        let record = record();
        assert!(matches!(
            verify_replay(&record, ReplayLevel::Authority, ReplayMaterials::default()),
            Err(SnapshotError::SnapshotUnavailable {
                level: ReplayLevel::Authority
            })
        ));
        assert!(matches!(
            verify_replay(
                &record,
                ReplayLevel::Reproduction,
                ReplayMaterials {
                    authority: Some(&record.contract.authority),
                    ..ReplayMaterials::default()
                }
            ),
            Err(SnapshotError::SnapshotUnavailable {
                level: ReplayLevel::Reproduction
            })
        ));
    }

    #[test]
    fn exact_assets_and_binary_reproduce_the_record() {
        let record = record();
        let verified = verify_replay(
            &record,
            ReplayLevel::Reproduction,
            ReplayMaterials {
                authority: Some(&record.contract.authority),
                contract: Some(&record.contract),
                binary_digest: Some("binary-digest"),
            },
        )
        .expect("reproduction");
        assert_eq!(verified.turn_record_digest, record.stage_digest);
        assert_eq!(
            verified
                .reproduced_surface
                .expect("reproduced surface")
                .clauses,
            vec!["истина зависит от разума"]
        );
    }

    #[test]
    fn reproduction_attributes_realization_and_binary_mismatches() {
        let record = record();
        let changed = TurnContractSnapshot::new(
            record.contract.authority.clone(),
            record.contract.planning.clone(),
            RealizationSnapshot::new("other", "grammar", "morph", "depth"),
            record.contract.selection.clone(),
        );
        assert!(matches!(
            verify_replay(
                &record,
                ReplayLevel::Reproduction,
                ReplayMaterials {
                    authority: Some(&record.contract.authority),
                    contract: Some(&changed),
                    binary_digest: Some("binary-digest"),
                }
            ),
            Err(SnapshotError::RealizationSnapshotMismatch)
        ));
        assert!(matches!(
            verify_replay(
                &record,
                ReplayLevel::Reproduction,
                ReplayMaterials {
                    authority: Some(&record.contract.authority),
                    contract: Some(&record.contract),
                    binary_digest: Some("other"),
                }
            ),
            Err(SnapshotError::BinaryMismatch)
        ));
    }

    #[test]
    fn exact_bundle_tampering_is_detected_before_reproduction() {
        let mut record = record();
        let CapturedRealizationNode::Clause { head_surface, .. } =
            &mut record.exact_replay.nodes[0]
        else {
            panic!("fixture is a clause")
        };
        *head_surface = "дрейфует".into();
        assert!(matches!(
            verify_replay(&record, ReplayLevel::Integrity, ReplayMaterials::default()),
            Err(SnapshotError::IntegrityMismatch {
                domain: "exact_replay_bundle",
                ..
            })
        ));
    }
}
