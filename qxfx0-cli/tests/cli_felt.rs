//! FELT dual-journal evidence acceptance (ADR-0043 U6): `felt-export`
//! never creates a database on a mistyped path and never refuses a thin
//! session, `felt-verify` re-evaluates the six gates without a database,
//! signatures follow the diary shape, and tampered verdicts fail closed.

mod common;

use common::{assert_success, run, turn, TestDir};
use std::fs;

fn drive_two_turns(db: &std::path::Path, session: &str) {
    assert_success(&turn(db, session, "что такое память?"));
    assert_success(&turn(db, session, "что такое внимание?"));
}

fn export(
    db: &std::path::Path,
    session: &str,
    out: &std::path::Path,
    extra: &[&str],
) -> std::process::Output {
    let mut args: Vec<&str> = vec!["--session-id", session, "felt-export", "--out"];
    let out_str = out.to_str().expect("temp path is utf-8");
    args.push(out_str);
    args.extend_from_slice(extra);
    run(db, &args)
}

#[test]
fn felt_export_is_fail_closed_on_missing_artifacts() {
    let dir = TestDir::new("felt-missing");
    let missing_db = dir.join("absent.db");
    let out = dir.join("felt.md");
    let output = export(&missing_db, "s", &out, &[]);
    assert!(
        !output.status.success(),
        "missing database must fail closed"
    );
    assert!(
        !missing_db.exists(),
        "felt-export must not create a database"
    );

    let db = dir.join("real.db");
    assert_success(&run(&db, &["sessions"]));
    let output = export(&db, "ghost", &out, &[]);
    assert!(!output.status.success(), "missing session must fail closed");
    assert!(String::from_utf8_lossy(&output.stderr).contains("ghost"));
}

#[test]
fn thin_session_exports_and_verifies_as_not_proven() {
    let dir = TestDir::new("felt-thin");
    let db = dir.join("felt.db");
    drive_two_turns(&db, "thin");
    let out = dir.join("thin.md");
    let output = export(&db, "thin", &out, &[]);
    assert_success(&output);
    let markdown = fs::read_to_string(&out).expect("export file exists");
    assert!(
        markdown.contains("не доказан"),
        "thin session must testify not-proven"
    );
    assert!(
        markdown.contains("```felt-manifest"),
        "manifest block embedded"
    );

    let verify = run(&db, &["felt-verify", out.to_str().unwrap()]);
    assert_success(&verify);
    let stdout = String::from_utf8_lossy(&verify.stdout);
    assert!(
        stdout.contains("не пройдены"),
        "gates honestly fail: {stdout}"
    );
    assert!(stdout.contains("подпись отсутствует"), "got: {stdout}");
}

#[test]
fn signature_paths_match_the_diary_shape() {
    let dir = TestDir::new("felt-signed");
    let db = dir.join("felt.db");
    drive_two_turns(&db, "signed");
    let out = dir.join("signed.md");
    assert_success(&export(&db, "signed", &out, &["--passphrase", "secret"]));
    let path = out.to_str().unwrap();

    let without = run(&db, &["felt-verify", path]);
    assert!(
        !without.status.success(),
        "signed export needs its passphrase"
    );
    assert!(String::from_utf8_lossy(&without.stderr).contains("--passphrase"));

    let wrong = run(&db, &["felt-verify", path, "--passphrase", "wrong"]);
    assert!(!wrong.status.success(), "wrong passphrase must fail closed");

    let right = run(&db, &["felt-verify", path, "--passphrase", "secret"]);
    assert_success(&right);
    assert!(String::from_utf8_lossy(&right.stdout).contains("проверена"));
}

#[test]
fn flipped_verdict_after_export_fails_verification() {
    let dir = TestDir::new("felt-tamper");
    let db = dir.join("felt.db");
    drive_two_turns(&db, "tamper");
    let out = dir.join("tamper.md");
    assert_success(&export(&db, "tamper", &out, &[]));
    let markdown = fs::read_to_string(&out).expect("export file exists");
    assert!(markdown.contains("\"verdict_passed\": false"));
    let evil = markdown.replacen("\"verdict_passed\": false", "\"verdict_passed\": true", 1);
    let evil_path = dir.join("evil.md");
    fs::write(&evil_path, evil).expect("write tampered file");
    let output = run(&db, &["felt-verify", evil_path.to_str().unwrap()]);
    assert!(!output.status.success(), "flipped verdict must fail closed");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("вердикт не совпадает"), "got: {stderr}");
}

#[test]
fn felt_export_never_overwrites() {
    let dir = TestDir::new("felt-clobber");
    let db = dir.join("felt.db");
    drive_two_turns(&db, "clobber");
    let out = dir.join("felt.md");
    assert_success(&export(&db, "clobber", &out, &[]));
    let output = export(&db, "clobber", &out, &[]);
    assert!(
        !output.status.success(),
        "existing evidence must never be overwritten"
    );
}
