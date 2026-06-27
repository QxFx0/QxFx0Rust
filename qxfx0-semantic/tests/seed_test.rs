use qxfx0_semantic::{seed_graph, verbalize_relation, COVERED_TOPICS};
use qxfx0_types::atom::AtomId;
use qxfx0_types::RelationType;

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
    assert_eq!(COVERED_TOPICS.len(), 107, "Should have 107 covered topics");
}

#[test]
fn test_svoboda_has_presupposes_and_limited_by() {
    let graph = seed_graph();
    let svoboda = AtomId::new("свобода");
    let rels = graph.relations_from(&svoboda);
    let has_presupposes = rels
        .iter()
        .any(|r| r.rel_type == RelationType::RelPresupposes);
    let has_limited_by = rels
        .iter()
        .any(|r| r.rel_type == RelationType::RelLimitedBy);

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
        .find(|r| r.rel_type == RelationType::RelPresupposes)
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
        RelationType::ALL.len(),
        47,
        "Should have 47 relation types"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Memory cluster tests — память, воспоминание, помнить
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_memory_atoms_in_covered_topics() {
    assert!(
        COVERED_TOPICS.contains(&"память"),
        "память should be in COVERED_TOPICS"
    );
    assert!(
        COVERED_TOPICS.contains(&"воспоминание"),
        "воспоминание should be in COVERED_TOPICS"
    );
    assert!(
        COVERED_TOPICS.contains(&"помнить"),
        "помнить should be in COVERED_TOPICS"
    );
}

#[test]
fn test_memory_atoms_exist_in_graph() {
    let graph = seed_graph();
    for name in &["память", "воспоминание", "помнить"] {
        let id = AtomId::new(*name);
        assert!(
            graph.atoms.contains_key(&id),
            "Atom '{}' should exist in graph.atoms",
            name
        );
    }
}

#[test]
fn test_pamyat_has_proper_edges() {
    let graph = seed_graph();
    let pamyat = AtomId::new("память");
    let rels = graph.relations_from(&pamyat);

    // память requires сознание
    assert!(
        rels.iter().any(|r| r.rel_type == RelationType::RelRequires
            && r.to.as_str() == "сознание"),
        "память should require сознание"
    );
    // память includes воспоминание
    assert!(
        rels.iter().any(|r| r.rel_type == RelationType::RelIncludes
            && r.to.as_str() == "воспоминание"),
        "память should include воспоминание"
    );
    // память structures бытие
    assert!(
        rels.iter().any(|r| r.rel_type == RelationType::RelStructures
            && r.to.as_str() == "бытие"),
        "память should structure бытие"
    );
    // память expresses through язык
    assert!(
        rels.iter().any(|r| r.rel_type == RelationType::RelExpresses
            && r.to.as_str() == "язык"),
        "память should express through язык"
    );
    // память preserves история
    assert!(
        rels.iter().any(|r| r.rel_type == RelationType::RelPreserves
            && r.to.as_str() == "история"),
        "память should preserve история"
    );
}

#[test]
fn test_vospominanie_has_proper_edges() {
    let graph = seed_graph();
    let vosp = AtomId::new("воспоминание");
    let rels = graph.relations_from(&vosp);

    // воспоминание requires время
    assert!(
        rels.iter().any(|r| r.rel_type == RelationType::RelRequires
            && r.to.as_str() == "время"),
        "воспоминание should require время"
    );
    // воспоминание requires сознание
    assert!(
        rels.iter().any(|r| r.rel_type == RelationType::RelRequires
            && r.to.as_str() == "сознание"),
        "воспоминание should require сознание"
    );
    // воспоминание reconstructs память
    assert!(
        rels.iter().any(|r| r.rel_type == RelationType::RelReconstructs
            && r.to.as_str() == "память"),
        "воспоминание should reconstruct память"
    );
}

#[test]
fn test_pomnit_has_proper_edges() {
    let graph = seed_graph();
    let pomnit = AtomId::new("помнить");
    let rels = graph.relations_from(&pomnit);

    // помнить requires память
    assert!(
        rels.iter().any(|r| r.rel_type == RelationType::RelRequires
            && r.to.as_str() == "память"),
        "помнить should require память"
    );
    // помнить evokes воспоминание
    assert!(
        rels.iter().any(|r| r.rel_type == RelationType::RelEvokes
            && r.to.as_str() == "воспоминание"),
        "помнить should evoke воспоминание"
    );
    // помнить requires сознание
    assert!(
        rels.iter().any(|r| r.rel_type == RelationType::RelRequires
            && r.to.as_str() == "сознание"),
        "помнить should require сознание"
    );
    // помнить depends on время
    assert!(
        rels.iter().any(|r| r.rel_type == RelationType::RelDependsOn
            && r.to.as_str() == "время"),
        "помнить should depend on время"
    );
    // помнить requires самосознание
    assert!(
        rels.iter().any(|r| r.rel_type == RelationType::RelRequires
            && r.to.as_str() == "самосознание"),
        "помнить should require самосознание"
    );
}

#[test]
fn test_verbalize_uses_ru_original_for_genitive() {
    let graph = seed_graph();
    let svoboda = AtomId::new("свобода");
    let rels = graph.relations_from(&svoboda);
    let rel = rels.iter().find(|r| {
        r.rel_type == RelationType::RelRequires && r.to.as_str() == "сознание"
    }).expect("свобода should have RelRequires->сознание");
    let text = verbalize_relation(rel);
    assert_eq!(text, "свобода требует сознания", "verbalize_relation must use ru_original with correct genitive case");
}

#[test]
fn test_conjugate_compose_uses_correct_grammar() {
    use qxfx0_semantic::{ConjugateComposer, SenseDecomposer};
    let graph = seed_graph();
    let vectors = SenseDecomposer::decompose("свобода", &graph);
    let surface = ConjugateComposer::compose(&graph, &vectors);
    assert!(surface.text.contains("свобода требует сознания"),
        "Expected genitive 'сознания', got: {}", surface.text);
    assert!(surface.text.contains("свобода контрастирует с истиной"),
        "Expected instrumental 'истиной', got: {}", surface.text);
}

#[test]
fn test_verbalize_uses_ru_original_for_instrumental() {
    let graph = seed_graph();
    let svoboda = AtomId::new("свобода");
    let rels = graph.relations_from(&svoboda);
    let rel = rels.iter().find(|r| {
        r.rel_type == RelationType::RelContrastsWith && r.to.as_str() == "истина"
    }).expect("свобода should have RelContrastsWith->истина");
    let text = verbalize_relation(rel);
    assert_eq!(text, "свобода контрастирует с истиной", "verbalize_relation must use ru_original with correct instrumental case");
}

#[test]
fn test_all_edge_atoms_exist_in_graph() {
    // Every atom referenced in any edge (from or to) must exist in graph.atoms
    let graph = seed_graph();
    for rel in &graph.edges {
        assert!(
            graph.atoms.contains_key(&rel.from),
            "Edge from-atom '{}' missing from graph.atoms",
            rel.from.as_str()
        );
        assert!(
            graph.atoms.contains_key(&rel.to),
            "Edge to-atom '{}' missing from graph.atoms",
            rel.to.as_str()
        );
    }
}
