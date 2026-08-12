#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP_DIR: AtomicU64 = AtomicU64::new(0);

pub struct TestDir(PathBuf);

impl TestDir {
    pub fn new(name: &str) -> Self {
        let sequence = NEXT_TEMP_DIR.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("qxfx0-{name}-{}-{sequence}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir(&path).expect("create isolated CLI test directory");
        Self(path)
    }

    pub fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub fn run(db: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_qxfx0"))
        .arg("--db")
        .arg(db)
        .args(args)
        .output()
        .expect("spawn qxfx0 CLI")
}

pub fn turn(db: &Path, session: &str, text: &str) -> Output {
    run(db, &["--session-id", session, "turn", text])
}

pub fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

pub fn assert_rejected_without_secret(output: &Output, secret: &str) {
    assert!(!output.status.success(), "command unexpectedly succeeded");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.trim().is_empty(),
        "failure must explain itself on stderr"
    );
    assert!(
        !stderr.contains(secret),
        "stderr leaked the sensitive test marker"
    );
}

pub fn state_digest(db: &Path, session: &str) -> String {
    let persistence = qxfx0_persistence::Persistence::open(db.to_str().unwrap()).unwrap();
    let state = persistence.load_state(session).unwrap().unwrap();
    qxfx0_pipeline::execution_trace::calculate_stable_digest(&state).unwrap()
}

pub fn doctor_json(db: &Path) -> serde_json::Value {
    let output = run(db, &["doctor", "--json"]);
    assert_success(&output);
    serde_json::from_slice(&output.stdout).expect("doctor emits JSON")
}

pub fn raw_session_rows(db: &Path, session: &str) -> Vec<Option<String>> {
    let connection = rusqlite::Connection::open(db).unwrap();
    let runtime = connection
        .query_row(
            "SELECT state_json FROM runtime_sessions WHERE id = ?1",
            [session],
            |row| row.get(0),
        )
        .unwrap();
    let graph = connection
        .query_row(
            "SELECT atoms_json || char(0) || edges_json FROM session_graphs WHERE session_id = ?1",
            [session],
            |row| row.get(0),
        )
        .unwrap();
    let semantic = connection
        .query_row(
            "SELECT field_json || char(0) || essence_json || char(0) || adjunction_json || char(0) || coalesce(commitments_json, '') || char(0) || coalesce(stance_provenance_json, '') || char(0) || coalesce(perspective_json, '') || char(0) || coalesce(thesis_state_json, '') FROM session_semantic WHERE session_id = ?1",
            [session],
            |row| row.get(0),
        )
        .unwrap();
    vec![runtime, graph, semantic]
}

pub fn session_turn_count(db: &Path, session: &str) -> usize {
    let connection = rusqlite::Connection::open(db).unwrap();
    connection
        .query_row(
            "SELECT turn_count FROM runtime_sessions WHERE id = ?1",
            [session],
            |row| row.get(0),
        )
        .unwrap()
}
