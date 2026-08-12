use qxfx0_types::*;
fn record(id: &str, trust: TrustClass, strength: u16) -> EvidenceRecord {
    EvidenceRecord {
        id: EvidenceId::try_new(id).unwrap(),
        source_id: SourceId::try_new("source.reference").unwrap(),
        kind: EvidenceKind::CuratedReference,
        trust_class: trust,
        source_revision: "revision-1".into(),
        content_digest: EvidenceDigest::from_hex(
            "1111111111111111111111111111111111111111111111111111111111111111",
        )
        .unwrap(),
        strength_basis_points: BasisPoints::try_new(strength).unwrap(),
    }
}
#[test]
fn canonical_digest_is_repeatable_and_semantically_sensitive() {
    let a = record("evidence.a", TrustClass::CuratedEmbedded, 8000);
    assert_eq!(a.canonical_digest().unwrap(), a.canonical_digest().unwrap());
    let mut b = a.clone();
    b.source_revision = "revision-2".into();
    assert_ne!(a.canonical_digest().unwrap(), b.canonical_digest().unwrap());
}
#[test]
fn sorted_set_is_order_invariant() {
    let a = record("a", TrustClass::CuratedEmbedded, 8000)
        .canonical_digest()
        .unwrap();
    let b = record("b", TrustClass::CuratedEmbedded, 6000)
        .canonical_digest()
        .unwrap();
    assert_eq!(
        sorted_evidence_set_digest([a, b]).unwrap(),
        sorted_evidence_set_digest([b, a]).unwrap()
    );
}
#[test]
fn identifiers_basis_points_and_digest_are_bounded() {
    assert!(EvidenceId::try_new(" ").is_err());
    assert!(EvidenceId::try_new("x".repeat(257)).is_err());
    assert!(BasisPoints::try_new(10001).is_err());
    assert!(EvidenceDigest::from_hex(&"A".repeat(64)).is_err());
}

#[test]
fn duplicate_set_members_fail_closed_and_reference_vector_is_stable() {
    let value = record("evidence.reference", TrustClass::CuratedEmbedded, 8000);
    let digest = value.canonical_digest().unwrap();
    assert_eq!(
        digest.to_string(),
        "e11c4f8c9e4291ce763a0a0014b88e3203872f0501396d7510e3294992200300"
    );
    assert_eq!(
        sorted_evidence_set_digest([digest]).unwrap().to_string(),
        "88bd29495332c2974ba64cb77aa74cc75540004294d54fff02a56210384a6eee"
    );
    assert!(matches!(
        sorted_evidence_set_digest([digest, digest]),
        Err(EvidenceTypeError::DuplicateEvidenceDigest)
    ));
}
