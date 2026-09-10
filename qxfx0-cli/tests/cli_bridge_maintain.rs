mod common;

use common::{assert_success, run, TestDir};

/// The ADR-0043 U4 maintenance surface: present, honest, and a no-op while
/// the bridge sleeps (no session carries a store).
#[test]
fn bridge_maintain_reports_the_sleeping_bridge() {
    let dir = TestDir::new("bridge-maintain");
    let db = dir.join("bridge.db");

    let help = run(&db, &["bridge-maintain", "--help"]);
    assert_success(&help);

    let report = run(&db, &["bridge-maintain", "--json"]);
    assert_success(&report);
    let value: serde_json::Value = serde_json::from_slice(&report.stdout).unwrap();
    assert_eq!(value["sessions_touched"], 0);
    assert!(value["sessions"].as_array().unwrap().is_empty());

    let plain = run(&db, &["bridge-maintain"]);
    assert_success(&plain);
    let stdout = String::from_utf8(plain.stdout).unwrap();
    assert!(stdout.contains("the bridge sleeps"), "{stdout}");
}
