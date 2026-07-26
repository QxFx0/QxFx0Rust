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
