//! Verification-only replay boundary. It never reads host assets or reruns a turn.

pub use qxfx0_semantic::response_plan_v2::{
    CapturedRealizationNode, ExactReplayBundle, ReplayInputEnvelope, ReplayLevel, ReplayMaterials,
    ReplayVerification, SnapshotError, TurnRecord,
};

pub fn verify_turn_record_replay(
    record: &TurnRecord,
    level: ReplayLevel,
    materials: ReplayMaterials<'_>,
) -> Result<ReplayVerification, SnapshotError> {
    qxfx0_semantic::response_plan_v2::verify_replay(record, level, materials)
}

#[cfg(test)]
mod tests {
    use super::*;
    use qxfx0_semantic::response_plan_v2::*;

    #[test]
    fn replay_api_fails_closed_without_materials() {
        let policy = SelectionPolicy::default();
        let contract = TurnContractSnapshot::new(
            AuthoritySnapshot::new("pack", AssertionPolicy::v1().digest()),
            PlanningPolicySnapshot::new("budget", "canon"),
            RealizationSnapshot::new("valency", "grammar", "morph", "depth"),
            SelectionPolicySnapshot::new(policy),
        );
        let record = TurnRecord::new(
            contract.clone(),
            SelectionReceipt {
                candidate_merkle_root: "candidate".into(),
                score: 0,
                context: SelfSelectionContext::quantize(0.0, 0.0, 0.0),
                policy_digest: contract.selection.self_policy_digest.clone(),
                ranking_version: contract.selection.ranking_version.clone(),
                numeric_semantics_version: contract.selection.numeric_semantics_version.clone(),
            },
            "binary",
            ExactReplayBundle::new(
                ReplayInputEnvelope {
                    topic: "topic".into(),
                    logical_turn: 1,
                    authority_as_of: None,
                },
                &contract.digest,
                "candidate",
                Vec::new(),
                RealizedSurface {
                    clauses: Vec::new(),
                    surface_digest:
                        "82a36c98076f2b9f81c1a8b3c38ad42aa0a5b76f90b24f53bbfe7a02925f455d".into(),
                    realization_snapshot_digest: contract.realization.fingerprint.clone(),
                    completeness_digest: "empty".into(),
                },
            ),
        );
        assert!(matches!(
            verify_turn_record_replay(
                &record,
                ReplayLevel::Reproduction,
                ReplayMaterials::default()
            ),
            Err(SnapshotError::SnapshotUnavailable { .. })
        ));
    }
}
