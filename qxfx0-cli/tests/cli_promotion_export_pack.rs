//! Editorial-feed acceptance (ADR-0043 U5.4): `promotion export-pack`
//! is wired and fail-closed. The honest flow (draft → both evaluations
//! → approve → release → export with provenance) is locked by the CLI
//! library lifecycle test; here we assert the command surface and the
//! never-overwrite, never-create guarantees.

mod common;

use common::{assert_success, run, TestDir};

#[test]
fn export_pack_is_listed_and_fail_closed() {
    let dir = TestDir::new("promotion-export-pack");
    let db = dir.join("promo.db");

    let help = run(&db, &["promotion", "--help"]);
    assert_success(&help);
    assert!(
        String::from_utf8_lossy(&help.stdout).contains("export-pack"),
        "promotion --help must list export-pack"
    );

    // Missing database: fail closed, never created.
    let missing = dir.join("absent.db");
    let out = dir.join("feed.json");
    let output = run(
        &missing,
        &[
            "promotion",
            "export-pack",
            "ghost",
            "--out",
            out.to_str().unwrap(),
        ],
    );
    assert!(
        !output.status.success(),
        "missing database must fail closed"
    );
    assert!(!missing.exists(), "export-pack must not create a database");

    // Real database, missing overlay: fail closed with the version named,
    // and no feed file created.
    assert_success(&run(&db, &["sessions"]));
    let output = run(
        &db,
        &[
            "promotion",
            "export-pack",
            "ghost",
            "--out",
            out.to_str().unwrap(),
        ],
    );
    assert!(!output.status.success(), "missing overlay must fail closed");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("ghost"), "got: {stderr}");
    assert!(!out.exists(), "failed export must not create a file");
}
