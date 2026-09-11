//! Runtime A/B acceptance (ADR-0043 U5): `promotion evaluate-runtime`
//! is wired, fail-closed on missing artifacts, never creates a database
//! on a mistyped path, and never mutates the operator database's
//! sessions. The honest measurement flow (snapshot → inject → render
//! both arms → verdict) is locked by the CLI library lifecycle test;
//! here we assert the command surface.

mod common;

use common::{assert_success, run, TestDir};

#[test]
fn evaluate_runtime_is_listed_and_fail_closed() {
    let dir = TestDir::new("promotion-runtime");
    let db = dir.join("promo.db");

    let help = run(&db, &["promotion", "--help"]);
    assert_success(&help);
    assert!(
        String::from_utf8_lossy(&help.stdout).contains("evaluate-runtime"),
        "promotion --help must list evaluate-runtime"
    );

    // Missing database: fail closed, never created.
    let missing = dir.join("absent.db");
    let output = run(&missing, &["promotion", "evaluate-runtime", "ghost"]);
    assert!(
        !output.status.success(),
        "missing database must fail closed"
    );
    assert!(
        !missing.exists(),
        "evaluate-runtime must not create a database"
    );

    // Real database, missing overlay: fail closed with the version named.
    assert_success(&run(&db, &["sessions"]));
    let output = run(&db, &["promotion", "evaluate-runtime", "ghost"]);
    assert!(!output.status.success(), "missing overlay must fail closed");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("ghost"), "got: {stderr}");
}

#[test]
fn evaluate_runtime_leaves_operator_sessions_untouched() {
    use std::fs;
    let dir = TestDir::new("promotion-runtime-clean");
    let db = dir.join("promo.db");
    assert_success(&run(
        &db,
        &["--session-id", "untouched", "turn", "что такое свобода?"],
    ));
    let before = fs::read(&db).expect("database file exists");

    // No overlay: the command fails before snapshotting, sessions intact.
    let output = run(&db, &["promotion", "evaluate-runtime", "ghost"]);
    assert!(!output.status.success());
    let after = fs::read(&db).expect("database file exists");
    assert_eq!(
        before, after,
        "failed trial must not touch the database file"
    );
}
