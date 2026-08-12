use qxfx0_semantic::{EvidenceRegistry, EvidenceRegistryError, EVIDENCE_POLICY_V1};
use qxfx0_types::*;
use std::collections::BTreeMap;
fn record(id: &str, trust: TrustClass, strength: u16) -> EvidenceRecord {
    EvidenceRecord {
        id: EvidenceId::try_new(id).unwrap(),
        source_id: SourceId::try_new("source").unwrap(),
        kind: EvidenceKind::Observation,
        trust_class: trust,
        source_revision: "r1".into(),
        content_digest: EvidenceDigest::from_hex(
            "2222222222222222222222222222222222222222222222222222222222222222",
        )
        .unwrap(),
        strength_basis_points: BasisPoints::try_new(strength).unwrap(),
    }
}
fn fixture(records: Vec<EvidenceRecord>) -> (EvidenceRegistry, ThesisId, ThesisDigest) {
    let tid = ThesisId::try_new("thesis.a").unwrap();
    let td = ThesisDigest::from_bytes([3; 32]);
    let known = BTreeMap::from([(tid.clone(), td)]);
    let rows = records
        .iter()
        .cloned()
        .map(|r| {
            let d = r.canonical_digest().unwrap();
            (r, d)
        })
        .collect::<Vec<_>>();
    let links = records
        .into_iter()
        .map(|r| ThesisEvidenceLink {
            thesis_id: tid.clone(),
            evidence_id: r.id,
            role: ThesisEvidenceRole::Supports,
        })
        .collect::<Vec<_>>();
    (
        EvidenceRegistry::load(rows, links, &known).unwrap(),
        tid,
        td,
    )
}
#[test]
fn order_invariance_repeatability_and_trust_boundary() {
    let curated = record("curated", TrustClass::CuratedEmbedded, 7000);
    let generated = record("generated", TrustClass::GeneratedObservation, 10000);
    let (a, t, d) = fixture(vec![curated.clone(), generated.clone()]);
    let (b, _, _) = fixture(vec![generated, curated]);
    let id = AssessmentId::try_new("assessment").unwrap();
    let x = a.assess(id.clone(), &t, d, EVIDENCE_POLICY_V1).unwrap();
    let y = b.assess(id, &t, d, EVIDENCE_POLICY_V1).unwrap();
    assert_eq!(x, y);
    assert_eq!(x.confidence_basis_points.get(), 10000);
    assert_eq!(x.authority_evidence_count, 1);
}
#[test]
fn tampering_dangling_duplicate_and_policy_fail_closed() {
    let r = record("e", TrustClass::CuratedEmbedded, 5000);
    let tid = ThesisId::try_new("t").unwrap();
    let known = BTreeMap::from([(tid.clone(), ThesisDigest::from_bytes([1; 32]))]);
    assert!(matches!(
        EvidenceRegistry::load(
            [(r.clone(), EvidenceDigest::from_bytes([0; 32]))],
            [],
            &known
        ),
        Err(EvidenceRegistryError::DigestMismatch(_))
    ));
    let d = r.canonical_digest().unwrap();
    let link = ThesisEvidenceLink {
        thesis_id: tid.clone(),
        evidence_id: r.id.clone(),
        role: ThesisEvidenceRole::Supports,
    };
    assert!(matches!(
        EvidenceRegistry::load([(r.clone(), d)], [link.clone(), link], &known),
        Err(EvidenceRegistryError::DuplicateLink)
    ));
    let registry = EvidenceRegistry::load([(r, d)], [], &known).unwrap();
    assert!(matches!(
        registry.assess(
            AssessmentId::try_new("a").unwrap(),
            &tid,
            ThesisDigest::from_bytes([1; 32]),
            2
        ),
        Err(EvidenceRegistryError::UnsupportedPolicy(2))
    ));
}
