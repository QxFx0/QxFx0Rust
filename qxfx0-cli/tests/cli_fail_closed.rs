mod common;

use common::{
    assert_rejected_without_secret, assert_success, doctor_json, raw_session_rows, run,
    state_digest, turn, TestDir,
};

const SESSION: &str = "baseline";
const SECRET: &str = "SENSITIVE-CLI-MARKER-4a8e";

fn assert_healthy_and_unchanged(db: &std::path::Path, digest: &str) {
    assert_eq!(state_digest(db, SESSION), digest);
    assert!(doctor_json(db)["healthy"].as_bool().unwrap());
}

#[test]
fn invalid_session_ids_fail_without_mutating_a_valid_database() {
    let dir = TestDir::new("invalid-sessions");
    let db = dir.join("state.db");
    assert_success(&turn(&db, SESSION, "что такое свобода?"));
    let baseline = state_digest(&db, SESSION);

    let too_long = "x".repeat(129);
    for invalid in ["", "   ", "line\nbreak", too_long.as_str()] {
        let output = turn(&db, invalid, SECRET);
        assert_rejected_without_secret(&output, SECRET);
        assert_healthy_and_unchanged(&db, &baseline);
    }
}

#[test]
fn corrupted_session_json_is_rejected_without_further_mutation() {
    let dir = TestDir::new("corrupt-session");
    let db = dir.join("state.db");
    assert_success(&turn(&db, SESSION, "что такое память?"));

    let connection = rusqlite::Connection::open(&db).unwrap();
    connection
        .execute(
            "UPDATE runtime_sessions SET state_json = ?1 WHERE id = ?2",
            ["{not-valid-json}", SESSION],
        )
        .unwrap();
    drop(connection);
    let corrupted = raw_session_rows(&db, SESSION);

    let output = turn(&db, SESSION, SECRET);
    assert_rejected_without_secret(&output, SECRET);
    assert_eq!(raw_session_rows(&db, SESSION), corrupted);

    let doctor = run(&db, &["doctor", "--json"]);
    assert_rejected_without_secret(&doctor, SECRET);
    let report: serde_json::Value = serde_json::from_slice(&doctor.stdout).unwrap();
    assert_eq!(report["healthy"], false);
}

#[test]
fn existing_artifact_destinations_and_strict_metrics_fail_closed() {
    let dir = TestDir::new("destinations-metrics");
    let db = dir.join("state.db");
    assert_success(&turn(&db, SESSION, "что такое сознание?"));
    let baseline = state_digest(&db, SESSION);

    let backup = dir.join("existing-backup.db");
    let trace = dir.join("existing-trace.jsonl");
    let sentinel = b"do-not-overwrite";
    std::fs::write(&backup, sentinel).unwrap();
    std::fs::write(&trace, sentinel).unwrap();

    let backup_failure = run(&db, &["backup", backup.to_str().unwrap()]);
    assert_rejected_without_secret(&backup_failure, SECRET);
    assert_eq!(std::fs::read(&backup).unwrap(), sentinel);
    assert_healthy_and_unchanged(&db, &baseline);

    let trace_failure = run(
        &db,
        &[
            "--session-id",
            SESSION,
            "turn",
            SECRET,
            "--doubt-shadow-trace-jsonl",
            trace.to_str().unwrap(),
        ],
    );
    assert_rejected_without_secret(&trace_failure, SECRET);
    assert_eq!(std::fs::read(&trace).unwrap(), sentinel);
    assert_healthy_and_unchanged(&db, &baseline);

    let metrics_failure = run(
        &db,
        &[
            "metrics",
            "--json",
            "--max-db-bytes",
            "1",
            "--max-response-ms",
            "60000",
        ],
    );
    assert_rejected_without_secret(&metrics_failure, SECRET);
    let metrics: serde_json::Value = serde_json::from_slice(&metrics_failure.stdout).unwrap();
    assert_eq!(metrics["doctor_healthy"], true);
    assert!(metrics["database_bytes"].as_u64().unwrap() > 1);
    assert_healthy_and_unchanged(&db, &baseline);
}
