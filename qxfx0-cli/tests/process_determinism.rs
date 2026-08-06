use std::path::Path;
use std::process::{Command, Output};

fn run_turn(binary: &str, db: &Path, session: &str, text: &str) -> Output {
    Command::new(binary)
        .args([
            "--db",
            db.to_str().unwrap(),
            "--session-id",
            session,
            "turn",
            text,
        ])
        .output()
        .expect("spawn fresh qxfx0 process")
}

fn cleanup(path: &Path) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
}

#[test]
fn fresh_process_replay_is_deterministic() {
    let binary = env!("CARGO_BIN_EXE_qxfx0");
    let base = std::env::temp_dir();
    let pid = std::process::id();
    let first_db = base.join(format!("qxfx0-process-replay-{pid}-a.db"));
    let second_db = base.join(format!("qxfx0-process-replay-{pid}-b.db"));
    cleanup(&first_db);
    cleanup(&second_db);

    let inputs = ["что такое свобода?", "я купил дом", "почему небо голубое?"];
    let session = "fresh-process-replay";
    let mut first_outputs = Vec::new();
    let mut second_outputs = Vec::new();

    for text in inputs {
        let first = run_turn(binary, &first_db, session, text);
        let second = run_turn(binary, &second_db, session, text);
        assert!(
            first.status.success(),
            "first process failed: {}",
            String::from_utf8_lossy(&first.stderr)
        );
        assert!(
            second.status.success(),
            "second process failed: {}",
            String::from_utf8_lossy(&second.stderr)
        );
        first_outputs.push(first.stdout);
        second_outputs.push(second.stdout);
    }

    assert_eq!(first_outputs, second_outputs);
    let first = qxfx0_persistence::Persistence::open(first_db.to_str().unwrap())
        .unwrap()
        .load_state(session)
        .unwrap()
        .unwrap();
    let second = qxfx0_persistence::Persistence::open(second_db.to_str().unwrap())
        .unwrap()
        .load_state(session)
        .unwrap()
        .unwrap();
    assert_eq!(
        qxfx0_pipeline::execution_trace::calculate_stable_digest(&first).unwrap(),
        qxfx0_pipeline::execution_trace::calculate_stable_digest(&second).unwrap()
    );

    cleanup(&first_db);
    cleanup(&second_db);
}

#[test]
fn debate_core_cli_writes_private_deterministic_trace_without_changing_a_turn() {
    let binary = env!("CARGO_BIN_EXE_qxfx0");
    let base = std::env::temp_dir();
    let pid = std::process::id();
    let standard_db = base.join(format!("qxfx0-debate-cli-{pid}-standard.db"));
    let observed_db = base.join(format!("qxfx0-debate-cli-{pid}-observed.db"));
    let rejected_db = base.join(format!("qxfx0-debate-cli-{pid}-rejected.db"));
    let conflict_db = base.join(format!("qxfx0-debate-cli-{pid}-conflict.db"));
    let trace = base.join(format!("qxfx0-debate-cli-{pid}.jsonl"));
    let conflict_trace = base.join(format!("qxfx0-debate-cli-{pid}-conflict.jsonl"));
    cleanup(&standard_db);
    cleanup(&observed_db);
    cleanup(&rejected_db);
    cleanup(&conflict_db);
    let _ = std::fs::remove_file(&trace);
    let _ = std::fs::remove_file(&conflict_trace);

    let session = "debate-cli-session";
    let text = "что такое свобода?";
    let standard = run_turn(binary, &standard_db, session, text);
    let observed = Command::new(binary)
        .args([
            "--db",
            observed_db.to_str().unwrap(),
            "--session-id",
            session,
            "turn",
            text,
            "--debate-core-trace-jsonl",
            trace.to_str().unwrap(),
        ])
        .output()
        .expect("spawn debate core turn");
    assert!(standard.status.success());
    assert!(
        observed.status.success(),
        "debate core command failed: {}",
        String::from_utf8_lossy(&observed.stderr)
    );
    assert_eq!(standard.stdout, observed.stdout);
    let standard_state = qxfx0_persistence::Persistence::open(standard_db.to_str().unwrap())
        .unwrap()
        .load_state(session)
        .unwrap()
        .unwrap();
    let observed_state = qxfx0_persistence::Persistence::open(observed_db.to_str().unwrap())
        .unwrap()
        .load_state(session)
        .unwrap()
        .unwrap();
    assert_eq!(
        qxfx0_pipeline::execution_trace::calculate_stable_digest(&standard_state).unwrap(),
        qxfx0_pipeline::execution_trace::calculate_stable_digest(&observed_state).unwrap()
    );

    let encoded = std::fs::read_to_string(&trace).unwrap();
    assert!(!encoded.contains(text));
    assert!(!encoded.contains(String::from_utf8_lossy(&observed.stdout).trim()));
    let value: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(value["schema"], "qxfx0.debate-core-trace.v1");
    assert_eq!(value["receipt"]["topic_id"], "свобода");
    assert_eq!(value["receipt"]["move_type"], "define");
    assert!(value.get("trace").is_none());

    let unknown_trace = base.join(format!("qxfx0-debate-cli-{pid}-unknown.jsonl"));
    let repeated_unknown_trace =
        base.join(format!("qxfx0-debate-cli-{pid}-unknown-repeated.jsonl"));
    let unknown_db = base.join(format!("qxfx0-debate-cli-{pid}-unknown.db"));
    cleanup(&unknown_db);
    let _ = std::fs::remove_file(&unknown_trace);
    let _ = std::fs::remove_file(&repeated_unknown_trace);
    let unknown_text = "что такое кванточайник?";
    let unknown = Command::new(binary)
        .args([
            "--db",
            unknown_db.to_str().unwrap(),
            "--session-id",
            "debate-cli-unknown",
            "turn",
            unknown_text,
            "--debate-core-trace-jsonl",
            unknown_trace.to_str().unwrap(),
        ])
        .output()
        .expect("spawn unknown-topic debate core turn");
    assert!(unknown.status.success());
    let unknown_encoded = std::fs::read_to_string(&unknown_trace).unwrap();
    assert!(!unknown_encoded.contains(unknown_text));
    assert!(!unknown_encoded.contains("кванточайник"));
    assert!(!unknown_encoded.contains("debate-cli-unknown"));
    let repeated_unknown = Command::new(binary)
        .args([
            "--db",
            unknown_db.to_str().unwrap(),
            "--session-id",
            "debate-cli-unknown",
            "turn",
            unknown_text,
            "--debate-core-trace-jsonl",
            repeated_unknown_trace.to_str().unwrap(),
        ])
        .output()
        .expect("spawn repeated unknown-topic debate core turn");
    assert!(repeated_unknown.status.success());
    let repeated_encoded = std::fs::read_to_string(&repeated_unknown_trace).unwrap();
    // Dialogue state may advance; this second turn verifies that redaction remains intact.
    assert!(!repeated_encoded.contains(unknown_text));
    assert!(!repeated_encoded.contains("кванточайник"));
    assert!(!repeated_encoded.contains("debate-cli-unknown"));

    let rejected = Command::new(binary)
        .args([
            "--db",
            rejected_db.to_str().unwrap(),
            "--session-id",
            session,
            "turn",
            text,
            "--debate-core-trace-jsonl",
            trace.to_str().unwrap(),
        ])
        .output()
        .expect("spawn rejected debate core turn");
    assert!(!rejected.status.success());
    assert!(!rejected_db.exists());

    let conflict = Command::new(binary)
        .args([
            "--db",
            conflict_db.to_str().unwrap(),
            "--session-id",
            session,
            "turn",
            text,
            "--debate-core-trace-jsonl",
            conflict_trace.to_str().unwrap(),
            "--record-stance-provenance",
        ])
        .output()
        .expect("spawn conflicting debate core turn");
    assert!(!conflict.status.success());
    assert!(!conflict_db.exists());

    cleanup(&standard_db);
    cleanup(&observed_db);
    cleanup(&rejected_db);
    cleanup(&conflict_db);
    cleanup(&unknown_db);
    let _ = std::fs::remove_file(&trace);
    let _ = std::fs::remove_file(&conflict_trace);
    let _ = std::fs::remove_file(&unknown_trace);
    let _ = std::fs::remove_file(&repeated_unknown_trace);
}

#[test]
fn user_argument_cli_writes_private_receipt_without_changing_a_turn() {
    let binary = env!("CARGO_BIN_EXE_qxfx0");
    let base = std::env::temp_dir();
    let pid = std::process::id();
    let standard_db = base.join(format!("qxfx0-user-argument-{pid}-standard.db"));
    let observed_db = base.join(format!("qxfx0-user-argument-{pid}-observed.db"));
    let rejected_db = base.join(format!("qxfx0-user-argument-{pid}-rejected.db"));
    let privacy_db = base.join(format!("qxfx0-user-argument-{pid}-privacy.db"));
    let trace = base.join(format!("qxfx0-user-argument-{pid}.jsonl"));
    let privacy_trace = base.join(format!("qxfx0-user-argument-{pid}-privacy.jsonl"));
    let repeated_trace = base.join(format!("qxfx0-user-argument-{pid}-repeated.jsonl"));
    for path in [&standard_db, &observed_db, &rejected_db, &privacy_db] {
        cleanup(path);
    }
    for path in [&trace, &privacy_trace, &repeated_trace] {
        let _ = std::fs::remove_file(path);
    }

    let session = "user-argument-cli-session";
    let text = "Свобода требует ответственности, потому что выбор имеет последствия.";
    let standard = run_turn(binary, &standard_db, session, text);
    let observed = Command::new(binary)
        .args([
            "--db",
            observed_db.to_str().unwrap(),
            "--session-id",
            session,
            "turn",
            text,
            "--user-argument-trace-jsonl",
            trace.to_str().unwrap(),
        ])
        .output()
        .expect("spawn user argument turn");
    assert!(standard.status.success());
    assert!(
        observed.status.success(),
        "user argument command failed: {}",
        String::from_utf8_lossy(&observed.stderr)
    );
    assert_eq!(standard.stdout, observed.stdout);

    let standard_state = qxfx0_persistence::Persistence::open(standard_db.to_str().unwrap())
        .unwrap()
        .load_state(session)
        .unwrap()
        .unwrap();
    let observed_state = qxfx0_persistence::Persistence::open(observed_db.to_str().unwrap())
        .unwrap()
        .load_state(session)
        .unwrap()
        .unwrap();
    assert_eq!(
        qxfx0_pipeline::execution_trace::calculate_stable_digest(&standard_state).unwrap(),
        qxfx0_pipeline::execution_trace::calculate_stable_digest(&observed_state).unwrap()
    );

    let encoded = std::fs::read_to_string(&trace).unwrap();
    assert!(!encoded.contains(text));
    assert!(!encoded.contains(session));
    assert!(!encoded.contains(String::from_utf8_lossy(&observed.stdout).trim()));
    let value: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(value["schema"], "qxfx0.user-argument-parse-trace.v1");
    assert_eq!(value["receipt"]["disposition"], "parsed");
    assert_eq!(value["receipt"]["nodes"].as_array().unwrap().len(), 2);
    assert!(value.get("trace").is_none());

    let verified = Command::new(binary)
        .args(["verify-user-argument-trace", trace.to_str().unwrap()])
        .output()
        .expect("verify user argument trace");
    assert!(
        verified.status.success(),
        "verification failed: {}",
        String::from_utf8_lossy(&verified.stderr)
    );

    let unknown_text = "что такое кванточайник?";
    let first_unknown = Command::new(binary)
        .args([
            "--db",
            privacy_db.to_str().unwrap(),
            "--session-id",
            "user-argument-private",
            "turn",
            unknown_text,
            "--user-argument-trace-jsonl",
            privacy_trace.to_str().unwrap(),
        ])
        .output()
        .expect("spawn unmatched user argument turn");
    assert!(first_unknown.status.success());
    let first_encoded = std::fs::read_to_string(&privacy_trace).unwrap();
    assert!(!first_encoded.to_lowercase().contains("кванточайник"));
    assert!(!first_encoded.contains("user-argument-private"));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&first_encoded).unwrap()["receipt"]
            ["disposition"],
        "abstained"
    );

    let repeated_unknown = Command::new(binary)
        .args([
            "--db",
            privacy_db.to_str().unwrap(),
            "--session-id",
            "user-argument-private",
            "turn",
            unknown_text,
            "--user-argument-trace-jsonl",
            repeated_trace.to_str().unwrap(),
        ])
        .output()
        .expect("spawn repeated unmatched user argument turn");
    assert!(repeated_unknown.status.success());
    let repeated_encoded = std::fs::read_to_string(&repeated_trace).unwrap();
    assert!(!repeated_encoded.to_lowercase().contains("кванточайник"));
    assert!(!repeated_encoded.contains("user-argument-private"));

    let rejected = Command::new(binary)
        .args([
            "--db",
            rejected_db.to_str().unwrap(),
            "--session-id",
            session,
            "turn",
            text,
            "--user-argument-trace-jsonl",
            trace.to_str().unwrap(),
        ])
        .output()
        .expect("spawn rejected user argument turn");
    assert!(!rejected.status.success());
    assert!(
        !rejected_db.exists(),
        "DB must not be opened after sink failure"
    );

    for path in [&standard_db, &observed_db, &rejected_db, &privacy_db] {
        cleanup(path);
    }
    for path in [&trace, &privacy_trace, &repeated_trace] {
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn doubt_shadow_cli_writes_external_trace_without_changing_a_turn() {
    let binary = env!("CARGO_BIN_EXE_qxfx0");
    let base = std::env::temp_dir();
    let pid = std::process::id();
    let standard_db = base.join(format!("qxfx0-doubt-cli-{pid}-standard.db"));
    let shadow_db = base.join(format!("qxfx0-doubt-cli-{pid}-shadow.db"));
    let rejected_db = base.join(format!("qxfx0-doubt-cli-{pid}-rejected.db"));
    let trace = base.join(format!("qxfx0-doubt-cli-{pid}.jsonl"));
    cleanup(&standard_db);
    cleanup(&shadow_db);
    cleanup(&rejected_db);
    let _ = std::fs::remove_file(&trace);

    let session = "doubt-cli-session";
    let text = "что такое свобода?";
    let standard = run_turn(binary, &standard_db, session, text);
    let shadow = Command::new(binary)
        .args([
            "--db",
            shadow_db.to_str().unwrap(),
            "--session-id",
            session,
            "turn",
            text,
            "--doubt-shadow-trace-jsonl",
            trace.to_str().unwrap(),
        ])
        .output()
        .expect("spawn doubt shadow turn");
    assert!(standard.status.success());
    assert!(
        shadow.status.success(),
        "doubt shadow command failed: {}",
        String::from_utf8_lossy(&shadow.stderr)
    );
    assert_eq!(standard.stdout, shadow.stdout);

    let standard_state = qxfx0_persistence::Persistence::open(standard_db.to_str().unwrap())
        .unwrap()
        .load_state(session)
        .unwrap()
        .unwrap();
    let shadow_state = qxfx0_persistence::Persistence::open(shadow_db.to_str().unwrap())
        .unwrap()
        .load_state(session)
        .unwrap()
        .unwrap();
    assert_eq!(
        qxfx0_pipeline::execution_trace::calculate_stable_digest(&standard_state).unwrap(),
        qxfx0_pipeline::execution_trace::calculate_stable_digest(&shadow_state).unwrap()
    );
    let trace_value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&trace).unwrap()).unwrap();
    assert_eq!(trace_value["schema"], "qxfx0.doubt-shadow-trace.v1");
    assert!(trace_value["trace"]["steps"]
        .as_array()
        .unwrap()
        .iter()
        .any(|step| step["stage"] == "doubt_shadow"));

    // The sink is validated before opening the database, so an existing trace
    // file fails without creating or modifying a session DB.
    let rejected = Command::new(binary)
        .args([
            "--db",
            rejected_db.to_str().unwrap(),
            "--session-id",
            session,
            "turn",
            text,
            "--doubt-shadow-trace-jsonl",
            trace.to_str().unwrap(),
        ])
        .output()
        .expect("spawn rejected doubt shadow turn");
    assert!(!rejected.status.success());
    assert!(
        !rejected_db.exists(),
        "DB must not be opened after sink failure"
    );

    cleanup(&standard_db);
    cleanup(&shadow_db);
    cleanup(&rejected_db);
    let _ = std::fs::remove_file(&trace);
}

#[test]
fn anomaly_shadow_cli_writes_external_trace_without_changing_a_turn() {
    let binary = env!("CARGO_BIN_EXE_qxfx0");
    let base = std::env::temp_dir();
    let pid = std::process::id();
    let standard_db = base.join(format!("qxfx0-anomaly-cli-{pid}-standard.db"));
    let shadow_db = base.join(format!("qxfx0-anomaly-cli-{pid}-shadow.db"));
    let rejected_db = base.join(format!("qxfx0-anomaly-cli-{pid}-rejected.db"));
    let trace = base.join(format!("qxfx0-anomaly-cli-{pid}.jsonl"));
    let diagnostics = base.join(format!("qxfx0-anomaly-cli-{pid}-diagnostics.jsonl"));
    cleanup(&standard_db);
    cleanup(&shadow_db);
    cleanup(&rejected_db);
    let _ = std::fs::remove_file(&trace);
    let _ = std::fs::remove_file(&diagnostics);

    let session = "anomaly-cli-session";
    let text = "что такое я?";
    let standard = run_turn(binary, &standard_db, session, text);
    let shadow = Command::new(binary)
        .args([
            "--db",
            shadow_db.to_str().unwrap(),
            "--session-id",
            session,
            "turn",
            text,
            "--anomaly-shadow-trace-jsonl",
            trace.to_str().unwrap(),
            "--diagnostics-jsonl",
            diagnostics.to_str().unwrap(),
        ])
        .output()
        .expect("spawn anomaly shadow turn");
    assert!(standard.status.success());
    assert!(
        shadow.status.success(),
        "anomaly shadow command failed: {}",
        String::from_utf8_lossy(&shadow.stderr)
    );
    assert_eq!(standard.stdout, shadow.stdout);

    let standard_state = qxfx0_persistence::Persistence::open(standard_db.to_str().unwrap())
        .unwrap()
        .load_state(session)
        .unwrap()
        .unwrap();
    let shadow_state = qxfx0_persistence::Persistence::open(shadow_db.to_str().unwrap())
        .unwrap()
        .load_state(session)
        .unwrap()
        .unwrap();
    assert_eq!(
        qxfx0_pipeline::execution_trace::calculate_stable_digest(&standard_state).unwrap(),
        qxfx0_pipeline::execution_trace::calculate_stable_digest(&shadow_state).unwrap()
    );
    let trace_value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&trace).unwrap()).unwrap();
    assert_eq!(trace_value["schema"], "qxfx0.anomaly-shadow-trace.v1");
    assert!(trace_value["trace"]["steps"]
        .as_array()
        .unwrap()
        .iter()
        .any(|step| step["stage"] == "anomaly_shadow"));
    let diagnostics_value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&diagnostics).unwrap()).unwrap();
    assert_eq!(diagnostics_value["schema"], "qxfx0.turn-diagnostics.v1");

    // The external sink is validated before opening the database, so an
    // existing artifact fails without creating or modifying a session DB.
    let rejected = Command::new(binary)
        .args([
            "--db",
            rejected_db.to_str().unwrap(),
            "--session-id",
            session,
            "turn",
            text,
            "--anomaly-shadow-trace-jsonl",
            trace.to_str().unwrap(),
        ])
        .output()
        .expect("spawn rejected anomaly shadow turn");
    assert!(!rejected.status.success());
    assert!(
        !rejected_db.exists(),
        "DB must not be opened after sink failure"
    );

    cleanup(&standard_db);
    cleanup(&shadow_db);
    cleanup(&rejected_db);
    let _ = std::fs::remove_file(&trace);
    let _ = std::fs::remove_file(&diagnostics);
}

#[test]
fn cognitive_pilot_trace_is_opt_in_and_validated_before_db_open() {
    let binary = env!("CARGO_BIN_EXE_qxfx0");
    let base = std::env::temp_dir();
    let pid = std::process::id();
    let standard_db = base.join(format!("qxfx0-cognitive-{pid}-standard.db"));
    let pilot_db = base.join(format!("qxfx0-cognitive-{pid}-pilot.db"));
    let rejected_db = base.join(format!("qxfx0-cognitive-{pid}-rejected.db"));
    let trace = base.join(format!("qxfx0-cognitive-{pid}.jsonl"));
    let diagnostics = base.join(format!("qxfx0-cognitive-{pid}-diagnostics.jsonl"));
    cleanup(&standard_db);
    cleanup(&pilot_db);
    cleanup(&rejected_db);
    let _ = std::fs::remove_file(&trace);
    let _ = std::fs::remove_file(&diagnostics);

    let session = "cognitive-pilot";
    let text = "что такое свобода?";
    let standard = run_turn(binary, &standard_db, session, text);
    let pilot = Command::new(binary)
        .args([
            "--db",
            pilot_db.to_str().unwrap(),
            "--session-id",
            session,
            "turn",
            text,
            "--cognitive-pilot-trace-jsonl",
            trace.to_str().unwrap(),
            "--diagnostics-jsonl",
            diagnostics.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(standard.status.success());
    assert!(
        pilot.status.success(),
        "{}",
        String::from_utf8_lossy(&pilot.stderr)
    );
    assert_eq!(standard.stdout, pilot.stdout);
    let standard_state = qxfx0_persistence::Persistence::open(standard_db.to_str().unwrap())
        .unwrap()
        .load_state(session)
        .unwrap()
        .unwrap();
    let pilot_state = qxfx0_persistence::Persistence::open(pilot_db.to_str().unwrap())
        .unwrap()
        .load_state(session)
        .unwrap()
        .unwrap();
    assert_eq!(
        qxfx0_pipeline::execution_trace::calculate_stable_digest(&standard_state).unwrap(),
        qxfx0_pipeline::execution_trace::calculate_stable_digest(&pilot_state).unwrap()
    );
    let record: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&trace).unwrap()).unwrap();
    assert_eq!(record["schema"], "qxfx0.cognitive-pilot-trace.v1");
    assert!(record["trace"]["steps"]
        .as_array()
        .unwrap()
        .iter()
        .any(|step| step["stage"] == "same_topic_suppression"));
    let diagnostic_record: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&diagnostics).unwrap()).unwrap();
    assert_eq!(diagnostic_record["schema"], "qxfx0.turn-diagnostics.v1");
    assert!(diagnostic_record["db_open_ms"].is_u64());
    assert!(diagnostic_record["cli_process_ms"].is_u64());

    let rejected = Command::new(binary)
        .args([
            "--db",
            rejected_db.to_str().unwrap(),
            "turn",
            text,
            "--enable-clarification",
        ])
        .output()
        .unwrap();
    assert!(!rejected.status.success());
    assert!(!rejected_db.exists());
    cleanup(&standard_db);
    cleanup(&pilot_db);
    cleanup(&rejected_db);
    let _ = std::fs::remove_file(&trace);
    let _ = std::fs::remove_file(&diagnostics);
}
