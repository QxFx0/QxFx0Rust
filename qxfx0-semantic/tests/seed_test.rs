use qxfx0_semantic::{seed_graph, verbalize_relation, COVERED_TOPICS};
use qxfx0_types::atom::AtomId;

#[test]
fn test_seed_graph_has_relations() {
    let graph = seed_graph();
    assert!(!graph.edges.is_empty(), "Seed graph should have relations");
    assert!(
        graph.edges.len() >= 11,
        "Should have at least 11 seed relations"
    );
}

#[test]
fn test_covered_topics_count() {
    assert_eq!(COVERED_TOPICS.len(), 30, "Should have 30 covered topics");
}

#[test]
fn test_svoboda_has_presupposes_and_limited_by() {
    let graph = seed_graph();
    let svoboda = AtomId::new("свобода");
    let rels = graph.relations_from(&svoboda);
    let has_presupposes = rels
        .iter()
        .any(|r| r.rel_type == qxfx0_types::RelationType::RelPresupposes);
    let has_limited_by = rels
        .iter()
        .any(|r| r.rel_type == qxfx0_types::RelationType::RelLimitedBy);

    assert!(has_presupposes, "свобода should have RelPresupposes");
    assert!(has_limited_by, "свобода should have RelLimitedBy");
}

#[test]
fn test_verbalize_svoboda_presupposes() {
    let graph = seed_graph();
    let svoboda = AtomId::new("свобода");
    let rels = graph.relations_from(&svoboda);
    let rel = rels
        .iter()
        .find(|r| r.rel_type == qxfx0_types::RelationType::RelPresupposes)
        .expect("should find RelPresupposes");

    let text = verbalize_relation(rel);
    assert!(text.contains("свобода"), "Should mention свобода");
    assert!(
        text.contains("предполагает"),
        "Should use verb предполагает"
    );
    assert!(text.contains("выбор"), "Should mention выбор");
}

#[test]
fn test_deterministic_graph() {
    // Graph should be identical on every construction
    let g1 = seed_graph();
    let g2 = seed_graph();
    assert_eq!(g1.edges.len(), g2.edges.len());
    assert_eq!(g1.atoms.len(), g2.atoms.len());
}

#[test]
fn test_relation_type_count() {
    assert_eq!(
        qxfx0_types::RelationType::ALL.len(),
        47,
        "Should have 47 relation types"
    );
}
