use qxfx0_semantic::{seed_graph, ConjugateComposer, SenseDecomposer};
use qxfx0_types::atom::AtomId;

#[test]
fn test_atom_pamyat_exists_in_graph() {
    let graph = seed_graph();
    let id = AtomId::new("память");
    assert!(
        graph.atoms.contains_key(&id),
        "Atom 'память' must exist in seed graph"
    );
}

#[test]
fn test_atom_vospominanie_exists_in_graph() {
    let graph = seed_graph();
    let id = AtomId::new("воспоминание");
    assert!(
        graph.atoms.contains_key(&id),
        "Atom 'воспоминание' must exist in seed graph"
    );
}

#[test]
fn test_decompose_single_pamyat() {
    let graph = seed_graph();
    let vectors = SenseDecomposer::decompose("память", &graph);
    assert!(
        !vectors.is_empty(),
        "Decompose 'память' should produce vectors"
    );
    assert!(
        vectors.iter().any(|v| v.atom_id.as_str() == "память"),
        "Decompose 'память' should find atom 'память', got: {:?}",
        vectors.iter().map(|v| v.atom_id.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn test_decompose_pamyat_i_vospominanie() {
    let graph = seed_graph();
    let vectors = SenseDecomposer::decompose("память и воспоминание", &graph);
    assert!(
        !vectors.is_empty(),
        "Decompose 'память и воспоминание' should produce vectors"
    );
    let has_pamyat = vectors.iter().any(|v| v.atom_id.as_str() == "память");
    let has_vospominanie = vectors.iter().any(|v| v.atom_id.as_str() == "воспоминание");
    assert!(
        has_pamyat,
        "Should find 'память', got: {:?}",
        vectors.iter().map(|v| v.atom_id.as_str()).collect::<Vec<_>>()
    );
    assert!(
        has_vospominanie,
        "Should find 'воспоминание', got: {:?}",
        vectors.iter().map(|v| v.atom_id.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn test_decompose_capitalized_pamyat() {
    let graph = seed_graph();
    let vectors = SenseDecomposer::decompose("Память", &graph);
    assert!(
        vectors.iter().any(|v| v.atom_id.as_str() == "память"),
        "Capitalized 'Память' should match atom 'память'"
    );
}

#[test]
fn test_conjugate_pamyat_i_vospominanie() {
    let graph = seed_graph();
    let vectors = SenseDecomposer::decompose("память и воспоминание", &graph);
    let surface = ConjugateComposer::compose(&graph, &vectors);
    assert!(
        !surface.text.is_empty(),
        "Conjugate response should not be empty"
    );
    // Response should mention память or воспоминание (the input atoms)
    assert!(
        surface.text.contains("память") || surface.text.contains("воспоминание"),
        "Response should mention 'память' or 'воспоминание', got: {}",
        surface.text
    );
}
