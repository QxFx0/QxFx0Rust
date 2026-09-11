//! Flip-proposal acceptance (ADR-0043 U6.2): `flip-draft` fails closed
//! on missing inputs, drafts honestly over real FELT exports with a live
//! B2 re-run, `flip-verify` recomputes without inputs, and tampered
//! proposals fail closed. The draft never flips anything.

mod common;

use common::{assert_success, run, turn, TestDir};
use std::fs;

fn felt_export(db: &std::path::Path, session: &str, out: &std::path::Path) {
    assert_success(&turn(db, session, "что такое память?"));
    assert_success(&turn(db, session, "что такое внимание?"));
    let output = run(
        db,
        &[
            "--session-id",
            session,
            "felt-export",
            "--out",
            out.to_str().unwrap(),
        ],
    );
    assert_success(&output);
}

fn tiny_prompts(dir: &TestDir) -> std::path::PathBuf {
    let path = dir.join("prompts.tsv");
    fs::write(&path, "что такое свобода?\nчто такое память?\n").expect("write prompts");
    path
}

fn draft(
    db: &std::path::Path,
    felts: &[&std::path::Path],
    prompts: &std::path::Path,
    out: &std::path::Path,
) -> std::process::Output {
    let mut args: Vec<String> = vec!["flip-draft".to_string()];
    for felt in felts {
        args.push("--felt".to_string());
        args.push(felt.to_str().unwrap().to_string());
    }
    args.push("--prompts".to_string());
    args.push(prompts.to_str().unwrap().to_string());
    args.push("--out".to_string());
    args.push(out.to_str().unwrap().to_string());
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run(db, &arg_refs)
}

#[test]
fn flip_draft_is_fail_closed_on_missing_inputs() {
    let dir = TestDir::new("flip-missing");
    let db = dir.join("flip.db");
    assert_success(&run(&db, &["sessions"]));
    let out = dir.join("proposal.json");

    let missing_felt = dir.join("absent.md");
    let prompts = tiny_prompts(&dir);
    let output = draft(&db, &[&missing_felt], &prompts, &out);
    assert!(
        !output.status.success(),
        "missing FELT file must fail closed"
    );

    let felt = dir.join("real.md");
    fs::write(&felt, "не манифест").expect("write fake felt");
    let missing_prompts = dir.join("absent.tsv");
    let output = draft(&db, &[&felt], &missing_prompts, &out);
    assert!(!output.status.success(), "missing prompts must fail closed");

    let empty_prompts = dir.join("empty.tsv");
    fs::write(&empty_prompts, "# только комментарий\n").expect("write empty prompts");
    let output = draft(&db, &[&felt], &empty_prompts, &out);
    assert!(!output.status.success(), "empty prompts must fail closed");
    assert!(String::from_utf8_lossy(&output.stderr).contains("ни одного промпта"));
}

#[test]
fn thin_coverage_drafts_honestly_not_ready() {
    let dir = TestDir::new("flip-thin");
    let db = dir.join("flip.db");
    let felt_a = dir.join("a.md");
    let felt_b = dir.join("b.md");
    felt_export(&db, "flip-a", &felt_a);
    felt_export(&db, "flip-b", &felt_b);
    let prompts = tiny_prompts(&dir);
    let out = dir.join("proposal.json");

    // Two thin sessions over two prompts: a live B2 re-run, real
    // verification — and an honest not-ready (2 < 5 sessions).
    let output = draft(&db, &[&felt_a, &felt_b], &prompts, &out);
    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("не готово"), "got: {stdout}");
    let proposal = fs::read_to_string(&out).expect("proposal written");
    assert!(proposal.contains("\"ready\": false"), "got: {proposal}");

    let verify = run(&db, &["flip-verify", out.to_str().unwrap()]);
    assert_success(&verify);
    let stdout = String::from_utf8_lossy(&verify.stdout);
    assert!(stdout.contains("не готово"), "got: {stdout}");
}

#[test]
fn flipped_readiness_after_draft_fails_verification() {
    let dir = TestDir::new("flip-tamper");
    let db = dir.join("flip.db");
    let felt_a = dir.join("a.md");
    let felt_b = dir.join("b.md");
    felt_export(&db, "flip-a", &felt_a);
    felt_export(&db, "flip-b", &felt_b);
    let prompts = tiny_prompts(&dir);
    let out = dir.join("proposal.json");
    assert_success(&draft(&db, &[&felt_a, &felt_b], &prompts, &out));

    let proposal = fs::read_to_string(&out).expect("proposal written");
    let evil = proposal.replacen("\"ready\": false", "\"ready\": true", 1);
    let evil_path = dir.join("evil.json");
    fs::write(&evil_path, evil).expect("write tampered proposal");
    let output = run(&db, &["flip-verify", evil_path.to_str().unwrap()]);
    assert!(
        !output.status.success(),
        "flipped readiness must fail closed"
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("готовность не совпадает"));
}

#[test]
fn flip_draft_never_overwrites_and_verify_rejects_garbage() {
    let dir = TestDir::new("flip-clobber");
    let db = dir.join("flip.db");
    let felt_a = dir.join("a.md");
    let felt_b = dir.join("b.md");
    felt_export(&db, "flip-a", &felt_a);
    felt_export(&db, "flip-b", &felt_b);
    let prompts = tiny_prompts(&dir);
    let out = dir.join("proposal.json");
    assert_success(&draft(&db, &[&felt_a, &felt_b], &prompts, &out));
    let output = draft(&db, &[&felt_a, &felt_b], &prompts, &out);
    assert!(
        !output.status.success(),
        "existing proposal must never be overwritten"
    );

    let garbage = dir.join("garbage.json");
    fs::write(&garbage, "{oops").expect("write garbage");
    let output = run(&db, &["flip-verify", garbage.to_str().unwrap()]);
    assert!(!output.status.success(), "garbage must fail closed");
}
