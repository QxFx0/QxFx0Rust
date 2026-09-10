mod common;

use common::{assert_success, run, TestDir};

/// The ADR-0043 U5 promotion surface: present and honest on an empty
/// database. The full lifecycle (draft → approve → release → rollback) is
/// locked by the unit test against in-memory persistence; here we assert the
/// CLI wiring — the subcommands parse, `list` shows no active overlay, and
/// `draft` with a sleeping bridge stores nothing rather than crashing.
#[test]
fn promotion_commands_report_the_empty_boundary() {
    let dir = TestDir::new("promotion");
    let db = dir.join("promo.db");

    let help = run(&db, &["promotion", "--help"]);
    assert_success(&help);
    let help = String::from_utf8(help.stdout).unwrap();
    for verb in ["list", "draft", "approve", "release", "rollback"] {
        assert!(help.contains(verb), "promotion --help must list {verb}");
    }

    let list = run(&db, &["promotion", "list", "--json"]);
    assert_success(&list);
    let value: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    assert_eq!(value["active"], serde_json::Value::Null);
    assert!(value["overlays"].as_array().unwrap().is_empty());

    // A sleeping bridge has nothing promoted, so drafting stores no row.
    let draft = run(&db, &["promotion", "draft", "--json"]);
    assert_success(&draft);
    let value: serde_json::Value = serde_json::from_slice(&draft.stdout).unwrap();
    assert_eq!(value["predicates"], 0);

    // Rollback with nothing active is a clean no-op.
    let rollback = run(&db, &["promotion", "rollback", "--json"]);
    assert_success(&rollback);
    let value: serde_json::Value = serde_json::from_slice(&rollback.stdout).unwrap();
    assert_eq!(value["active"], serde_json::Value::Null);

    // The journal is still empty after a no-op draft.
    let recheck = run(&db, &["promotion", "list", "--json"]);
    let recheck: serde_json::Value = serde_json::from_slice(&recheck.stdout).unwrap();
    assert!(recheck["overlays"].as_array().map(Vec::is_empty) == Some(true));
}
