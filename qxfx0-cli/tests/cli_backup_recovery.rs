mod common;

use common::{
    assert_rejected_without_secret, assert_success, doctor_json, run, state_digest, turn, TestDir,
};

#[test]
fn backup_preserves_sessions_and_restores_deterministic_continuation() {
    let dir = TestDir::new("backup-recovery");
    let source = dir.join("source.db");
    let backup = dir.join("backup.db");
    let restored = dir.join("restored.db");

    for (session, inputs) in [
        (
            "alpha",
            &["что такое свобода?", "как свобода связана с волей?"][..],
        ),
        ("beta", &["что такое память?"][..]),
    ] {
        for input in inputs {
            assert_success(&turn(&source, session, input));
        }
    }

    let sessions = run(&source, &["sessions"]);
    assert_success(&sessions);
    let sessions = String::from_utf8(sessions.stdout).unwrap();
    assert!(sessions.contains("alpha"));
    assert!(sessions.contains("beta"));
    assert!(doctor_json(&source)["healthy"].as_bool().unwrap());

    let created = run(&source, &["backup", backup.to_str().unwrap()]);
    assert_success(&created);
    let backup_bytes = std::fs::read(&backup).unwrap();
    assert!(!backup_bytes.is_empty());

    let secret = "SENSITIVE-BACKUP-MARKER-7d1c";
    let rejected = run(&source, &["backup", backup.to_str().unwrap()]);
    assert_rejected_without_secret(&rejected, secret);
    assert_eq!(std::fs::read(&backup).unwrap(), backup_bytes);

    std::fs::copy(&backup, &restored).unwrap();
    let restored_sessions = run(&restored, &["sessions"]);
    assert_success(&restored_sessions);
    assert_eq!(restored_sessions.stdout, run(&source, &["sessions"]).stdout);
    assert!(doctor_json(&restored)["healthy"].as_bool().unwrap());

    let next = "почему ответственность связана со свободой?";
    let source_next = turn(&source, "alpha", next);
    let restored_next = turn(&restored, "alpha", next);
    assert_success(&source_next);
    assert_success(&restored_next);
    assert_eq!(source_next.stdout, restored_next.stdout);
    assert_eq!(
        state_digest(&source, "alpha"),
        state_digest(&restored, "alpha")
    );

    let source_metrics = run(
        &source,
        &["metrics", "--json", "--max-response-ms", "60000"],
    );
    let restored_metrics = run(
        &restored,
        &["metrics", "--json", "--max-response-ms", "60000"],
    );
    assert_success(&source_metrics);
    assert_success(&restored_metrics);
    let source_metrics: serde_json::Value = serde_json::from_slice(&source_metrics.stdout).unwrap();
    let restored_metrics: serde_json::Value =
        serde_json::from_slice(&restored_metrics.stdout).unwrap();
    for key in ["doctor_healthy", "database_bytes", "response_probe_healthy"] {
        assert_eq!(source_metrics[key], restored_metrics[key], "metric {key}");
    }
}
