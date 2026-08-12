mod common;

use common::{assert_success, doctor_json, session_turn_count, state_digest, turn, TestDir};
use std::path::Path;

const WORKLOAD: [&str; 12] = [
    "Что такое истина?",
    "Как связаны свобода и ответственность?",
    "Что означает человеческое достоинство?",
    "Как память влияет на личность?",
    "В чём различие знания и убеждения?",
    "Как надежда связана с действием?",
    "Что делает решение справедливым?",
    "Как язык формирует понимание?",
    "Почему доверие требует ответственности?",
    "Как связаны причина и следствие?",
    "Что означает сохранять внутреннюю целостность?",
    "Как опыт меняет представление о будущем?",
];
const SESSIONS: [&str; 3] = ["restart-alpha", "restart-beta", "restart-gamma"];

fn database_storage_bytes(path: &Path) -> u64 {
    [
        path.to_path_buf(),
        path.with_extension("db-wal"),
        path.with_extension("db-shm"),
    ]
    .into_iter()
    .filter_map(|candidate| std::fs::metadata(candidate).ok())
    .map(|metadata| metadata.len())
    .sum()
}

fn assert_healthy(path: &Path) {
    let report = doctor_json(path);
    assert_eq!(report["healthy"], true, "unhealthy report: {report}");
}

fn run_restart_soak(turns: usize, name: &str) {
    let dir = TestDir::new(name);
    let left = dir.join("left.db");
    let right = dir.join("right.db");

    assert_healthy(&left);
    assert_healthy(&right);
    let initial_bytes = database_storage_bytes(&left).max(database_storage_bytes(&right));
    let mut expected_counts = [0_usize; SESSIONS.len()];
    let mut left_stdout = Vec::with_capacity(turns);
    let mut right_stdout = Vec::with_capacity(turns);

    for turn_index in 0..turns {
        let session_index = turn_index % SESSIONS.len();
        let session = SESSIONS[session_index];
        let prompt = WORKLOAD[turn_index % WORKLOAD.len()];

        // Each call starts a fresh qxfx0 process. The twin processes run in
        // parallel against independent databases with exactly the same input.
        let (left_output, right_output) = std::thread::scope(|scope| {
            let left_process = scope.spawn(|| turn(&left, session, prompt));
            let right_process = scope.spawn(|| turn(&right, session, prompt));
            (left_process.join().unwrap(), right_process.join().unwrap())
        });
        assert_success(&left_output);
        assert_success(&right_output);
        assert_eq!(left_output.stdout, right_output.stdout, "turn {turn_index}");
        left_stdout.push(left_output.stdout);
        right_stdout.push(right_output.stdout);

        expected_counts[session_index] += 1;
        assert_eq!(
            session_turn_count(&left, session),
            expected_counts[session_index]
        );
        assert_eq!(
            session_turn_count(&right, session),
            expected_counts[session_index]
        );

        if (turn_index + 1) % WORKLOAD.len() == 0 {
            assert_healthy(&left);
            assert_healthy(&right);
        }
    }

    assert_eq!(left_stdout, right_stdout);
    for (session_index, session) in SESSIONS.into_iter().enumerate() {
        assert_eq!(
            session_turn_count(&left, session),
            expected_counts[session_index]
        );
        assert_eq!(
            session_turn_count(&right, session),
            expected_counts[session_index]
        );
        assert_eq!(state_digest(&left, session), state_digest(&right, session));
    }
    assert_healthy(&left);
    assert_healthy(&right);

    // This is intentionally a generous storage bound, not a performance gate.
    // It catches runaway/unbounded persistence while allowing SQLite page and
    // WAL bookkeeping to vary across supported hosts.
    let storage_limit = initial_bytes + 4 * 1024 * 1024 + turns as u64 * 256 * 1024;
    for path in [&left, &right] {
        let bytes = database_storage_bytes(path);
        assert!(
            bytes <= storage_limit,
            "{} grew to {bytes} bytes (limit {storage_limit})",
            path.display()
        );
    }
}

#[test]
fn restart_load_acceptance_is_deterministic_across_twin_databases() {
    run_restart_soak(40, "restart-soak-short");
}

#[test]
#[ignore = "300-turn nightly/manual restart soak"]
fn restart_load_extended_soak_is_deterministic_across_twin_databases() {
    run_restart_soak(300, "restart-soak-long");
}

#[test]
#[ignore = "operational writer-lock boundary; exercises the production five-second timeout"]
fn concurrent_writer_lock_fails_cleanly_and_retry_succeeds() {
    let dir = TestDir::new("writer-lock-boundary");
    let db = dir.join("state.db");
    let session = "writer-boundary";
    assert_success(&turn(&db, session, WORKLOAD[0]));
    let baseline_digest = state_digest(&db, session);

    let connection = rusqlite::Connection::open(&db).unwrap();
    connection.execute_batch("BEGIN IMMEDIATE").unwrap();

    // BEGIN IMMEDIATE is the deterministic coordination point: the child is
    // spawned only after this process owns SQLite's writer lock. No sleeps or
    // scheduler-sensitive race are involved.
    let blocked = turn(&db, session, WORKLOAD[1]);
    assert!(
        !blocked.status.success(),
        "contended writer unexpectedly succeeded"
    );
    assert!(
        !blocked.stderr.is_empty(),
        "contended writer must explain its failure"
    );
    connection.execute_batch("ROLLBACK").unwrap();
    drop(connection);

    assert_eq!(session_turn_count(&db, session), 1);
    assert_eq!(state_digest(&db, session), baseline_digest);
    let retry = turn(&db, session, WORKLOAD[1]);
    assert_success(&retry);
    assert_eq!(session_turn_count(&db, session), 2);
    assert_healthy(&db);
}
