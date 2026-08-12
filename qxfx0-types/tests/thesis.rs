use qxfx0_types::{
    ConceptId, RelationId, Thesis, ThesisGraph, ThesisGraphError, ThesisId, ThesisKind,
    ThesisRelation, ThesisRelationKind,
};
use std::collections::BTreeMap;

fn thesis(id: &str) -> Thesis {
    Thesis {
        id: ThesisId::try_new(id).unwrap(),
        subject: ConceptId("concept.alpha".into()),
        predicate: RelationId::try_new("RelDependsOn").unwrap(),
        object: ConceptId("concept.beta".into()),
        kind: ThesisKind::InterpretiveClaim,
        qualifiers: BTreeMap::from([
            ("locale".into(), "ru".into()),
            ("scope".into(), "general".into()),
        ]),
        surface_text: Some("presentation only".into()),
        confidence_basis_points: Some(9_500),
        provenance: BTreeMap::from([("source".into(), "test".into())]),
        valid_from: Some("2026-01-01".into()),
        valid_to: None,
    }
}

#[test]
fn canonical_reference_vector_v1_is_stable_and_lowercase() {
    let value = thesis("thesis.reference");
    assert_eq!(
        value.canonical_digest().unwrap().to_string(),
        "f3c27b0f9515b79402d11cbc67504c7b89e8354e8dc32f61f2f598a474477c80"
    );
    assert_eq!(
        serde_json::to_string(&value.canonical_digest().unwrap()).unwrap(),
        format!("\"{}\"", value.canonical_digest().unwrap())
    );
}

#[test]
fn qualifier_input_order_does_not_affect_identity() {
    let left = thesis("left");
    let mut right = thesis("right");
    right.qualifiers = [("scope", "general"), ("locale", "ru")]
        .into_iter()
        .map(|(k, v)| (k.into(), v.into()))
        .collect();
    assert_eq!(
        left.canonical_digest().unwrap(),
        right.canonical_digest().unwrap()
    );
}

#[test]
fn semantic_fields_affect_identity_but_metadata_does_not() {
    let base = thesis("one");
    let mut metadata = thesis("different-id");
    metadata.surface_text = Some("other text".into());
    metadata.confidence_basis_points = Some(1);
    metadata.provenance = BTreeMap::from([("other".into(), "authority".into())]);
    metadata.valid_from = None;
    metadata.valid_to = Some("2099-01-01".into());
    assert_eq!(
        base.canonical_digest().unwrap(),
        metadata.canonical_digest().unwrap()
    );

    let mut semantic = base.clone();
    semantic.object = ConceptId("concept.gamma".into());
    assert_ne!(
        base.canonical_digest().unwrap(),
        semantic.canonical_digest().unwrap()
    );
}

#[test]
fn validation_is_strictly_bounded() {
    let mut value = thesis("bounded");
    value.confidence_basis_points = Some(10_001);
    assert!(value.validate().is_err());
    value.confidence_basis_points = None;
    value.qualifiers = (0..65).map(|i| (format!("k{i}"), "v".into())).collect();
    assert!(value.validate().is_err());
    assert!(ThesisId::try_new(" ").is_err());
}

#[test]
fn graph_validates_endpoints_self_edges_duplicates_order_and_revision_cycles() {
    let mut graph = ThesisGraph::default();
    for id in ["c", "a", "b"] {
        graph.insert_thesis(thesis(id)).unwrap();
    }
    let dangling = ThesisRelation {
        from: ThesisId::try_new("a").unwrap(),
        kind: ThesisRelationKind::Counters,
        to: ThesisId::try_new("missing").unwrap(),
    };
    assert!(matches!(
        graph.insert_relation(dangling),
        Err(ThesisGraphError::DanglingEndpoint(_))
    ));
    let self_edge = ThesisRelation {
        from: ThesisId::try_new("a").unwrap(),
        kind: ThesisRelationKind::Counters,
        to: ThesisId::try_new("a").unwrap(),
    };
    assert!(matches!(
        graph.insert_relation(self_edge),
        Err(ThesisGraphError::SelfEdge(_))
    ));

    let ab = ThesisRelation {
        from: ThesisId::try_new("a").unwrap(),
        kind: ThesisRelationKind::Revises,
        to: ThesisId::try_new("b").unwrap(),
    };
    assert!(graph.insert_relation(ab.clone()).unwrap());
    assert!(!graph.insert_relation(ab).unwrap());
    graph
        .insert_relation(ThesisRelation {
            from: ThesisId::try_new("b").unwrap(),
            kind: ThesisRelationKind::Supersedes,
            to: ThesisId::try_new("c").unwrap(),
        })
        .unwrap();
    assert!(matches!(
        graph.insert_relation(ThesisRelation {
            from: ThesisId::try_new("c").unwrap(),
            kind: ThesisRelationKind::Revises,
            to: ThesisId::try_new("a").unwrap(),
        }),
        Err(ThesisGraphError::RevisionCycle)
    ));
    assert_eq!(
        graph
            .theses()
            .keys()
            .map(ThesisId::as_str)
            .collect::<Vec<_>>(),
        ["a", "b", "c"]
    );
    graph.validate().unwrap();
}
