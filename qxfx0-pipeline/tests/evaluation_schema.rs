use serde_json::Value;
use std::{collections::BTreeSet, fs, path::PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}
fn json(path: &str) -> Value {
    serde_json::from_slice(&fs::read(root().join(path)).unwrap()).unwrap()
}

#[test]
fn thesis_graph_corpus_v1_schema_and_coverage_are_frozen() {
    let corpus = json("data/eval/thesis-graph-v1/corpus.json");
    assert_eq!(corpus["schema_version"], 1);
    assert_eq!(corpus["corpus_id"], "thesis-graph-v1");
    let scenarios = corpus["scenarios"].as_array().unwrap();
    assert_eq!(scenarios.len(), 18);
    let packs = scenarios
        .iter()
        .map(|s| s["pack"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        packs,
        BTreeSet::from([
            "agency-responsibility-v1",
            "epistemology-truth-v1",
            "mind-memory-language-v1"
        ])
    );
    let categories = scenarios
        .iter()
        .map(|s| s["category"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        categories,
        BTreeSet::from([
            "explicit-counterargument",
            "true-contradiction",
            "qualification",
            "evidence-confidence-change",
            "revision-supersession-retraction",
            "cross-theme-link"
        ])
    );
    assert!(scenarios
        .iter()
        .all(|s| s["turns"].as_array().is_some_and(|x| x.len() == 3)));
    assert!(
        scenarios
            .iter()
            .filter(|s| s["adversarial"] == true)
            .count()
            >= 9
    );
    assert!(scenarios
        .iter()
        .all(|s| s["expected"]["user_text_must_not_be_authority"] == true));
}

#[test]
fn preregistration_names_all_hypotheses_and_modes() {
    let cfg = json("data/eval/thesis-graph-v1/preregistered-config.json");
    assert_eq!(cfg["frozen_before_run"], true);
    assert_eq!(cfg["replays_per_mode"], 2);
    assert_eq!(cfg["modes"], serde_json::json!(["disabled", "shadow"]));
    for h in [
        "H1_consistency",
        "H2_explainability",
        "H3_determinism_replay",
        "H4_safety_no_false_authority",
        "surface_effect",
    ] {
        assert!(cfg["hypotheses"].get(h).is_some(), "missing {h}");
    }
}
