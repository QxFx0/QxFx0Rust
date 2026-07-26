use std::fs;

use qxfx0_self::perspective::{PerspectiveRegistry, PerspectiveRegistryConfig};
use qxfx0_types::PerspectiveMutation;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct ProjectionExpectation {
    scope: String,
    summary: String,
    version: u64,
    evidence_count: usize,
    counterargument_count: usize,
}

#[derive(Debug, Deserialize)]
struct ReferenceVector {
    config: PerspectiveRegistryConfig,
    mutations: Vec<PerspectiveMutation>,
    expected_active_scopes: Vec<String>,
    expected_projection: ProjectionExpectation,
}

#[test]
fn perspective_projection_v1_conforms_to_reference_vector() {
    let source = fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/perspective_projection_v1.json"
    ))
    .expect("reference vector must be present");
    let vector: ReferenceVector =
        serde_json::from_str(&source).expect("reference vector must parse");

    let registry = vector.mutations.iter().fold(
        PerspectiveRegistry::new(vector.config),
        |registry, mutation| registry.apply(mutation),
    );
    let active = registry.build_active_projections();
    assert_eq!(
        active
            .iter()
            .map(|projection| projection.scope.render())
            .collect::<Vec<_>>(),
        vector.expected_active_scopes
    );
    let projection = active
        .first()
        .expect("reference vector has an active projection");
    assert_eq!(projection.scope.render(), vector.expected_projection.scope);
    assert_eq!(projection.summary, vector.expected_projection.summary);
    assert_eq!(
        projection.perspective_version.0,
        vector.expected_projection.version
    );
    assert_eq!(
        projection.evidence_count,
        vector.expected_projection.evidence_count
    );
    assert_eq!(
        projection.counterargument_count,
        vector.expected_projection.counterargument_count
    );
}
