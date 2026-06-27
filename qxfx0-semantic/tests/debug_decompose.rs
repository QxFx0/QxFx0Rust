use qxfx0_semantic::{seed_graph, SenseDecomposer};

#[test]
fn debug_decompose_pamyat() {
    let graph = seed_graph();
    let vectors = SenseDecomposer::decompose("память и воспоминание", &graph);
    eprintln!("Vectors count: {}", vectors.len());
    for v in &vectors {
        eprintln!("  atom={} weight={}", v.atom_id.as_str(), v.weight);
    }
    assert!(!vectors.is_empty());
}
