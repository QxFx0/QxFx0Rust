//! QxFx0 cli crate — generated from Haskell specification.
//!
//! Exposes the same entry point used by `main.rs` so integration tests can
//! drive the turn / chat flow without spawning a subprocess.

use qxfx0_pipeline::{TurnInput, process_turn};
use qxfx0_semantic::seed_graph;
use qxfx0_types::system_state::{SemanticState, SystemState};

/// Build a freshly seeded `SystemState` for a given session id.
pub fn fresh_state(session_id: &str) -> SystemState {
    SystemState {
        session_id: session_id.to_string(),
        semantic: SemanticState {
            runtime_graph: seed_graph(),
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Load existing state for the session, or create a fresh one seeded with the
/// knowledge graph. Any persistence error other than "no such session row"
/// (which `Persistence::load_state` already maps to `Ok(None)`) is propagated
/// via `?` so the caller can surface it to the user.
pub fn load_or_create_state(
    db: &qxfx0_persistence::Persistence,
    session_id: &str,
) -> anyhow::Result<SystemState> {
    match db.load_state(session_id) {
        Ok(Some(state)) => Ok(state),
        Ok(None) => Ok(fresh_state(session_id)),
        Err(e) => Err(anyhow::anyhow!(e)),
    }
}

/// Run a single turn through the pipeline (mirrors the `Turn` CLI branch).
/// Persists before returning the response text — see H4 in the audit.
pub fn run_turn(
    db: &qxfx0_persistence::Persistence,
    session_id: &str,
    text: &str,
) -> anyhow::Result<String> {
    let mut state = load_or_create_state(db, session_id)?;
    let input = TurnInput {
        raw_text: text.to_string(),
        session_id: session_id.to_string(),
    };
    let output = process_turn(&input, &mut state);
    db.save_state(session_id, &state)?;
    Ok(output.response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::process::Stdio;

    /// M7.1 — smoke test: run a `Turn` against an in-memory DB and assert the
    /// pipeline returns a non-empty response. Mirrors the `Turn` CLI branch.
    #[test]
    fn test_turn_smoke() {
        let db = qxfx0_persistence::Persistence::open_memory().expect("open in-memory db");
        let response = run_turn(&db, "smoke-session", "что такое свобода?")
            .expect("turn should succeed");
        assert!(
            !response.is_empty(),
            "pipeline produced empty response for seeded topic"
        );

        // State should have been persisted by run_turn.
        let loaded = db
            .load_state("smoke-session")
            .expect("load after save")
            .expect("session row must exist");
        assert_eq!(loaded.session_id, "smoke-session");
        assert!(loaded.dialogue.turn_count >= 1);
    }

    /// M7.2 — chat-mode EOF must still persist state. We spawn the actual
    /// binary with piped stdin that ends in EOF (no `:quit`) so the chat
    /// loop reaches the unconditional save at the end.
    #[test]
    fn test_chat_eof_saves() {
        let tmp = std::env::temp_dir().join(format!(
            "qxfx0-cli-eof-{}.db",
            std::process::id()
        ));
        let db_path = tmp.to_string_lossy().to_string();
        let session_id = format!("eof-session-{}", std::process::id());

        let stdin_payload = "что такое истина?\n".to_string();

        // Resolve the binary path. `CARGO_BIN_EXE_qxfx0` is set when this
        // crate's own integration tests build (or when running via
        // `cargo test --bin qxfx0`); fall back to the conventional
        // `target/debug/qxfx0` path for plain `cargo test --workspace`.
        let bin = match std::env::var("CARGO_BIN_EXE_qxfx0") {
            Ok(p) => p,
            Err(_) => {
                let target_dir = std::env::var("CARGO_TARGET_DIR")
                    .unwrap_or_else(|_| "target".to_string());
                let workspace = std::env::var("CARGO_MANIFEST_DIR")
                    .map(|m| std::path::PathBuf::from(m).join(&target_dir).join("debug/qxfx0"))
                    .unwrap_or_else(|_| std::path::PathBuf::from(&target_dir).join("debug/qxfx0"));
                workspace.to_string_lossy().to_string()
            }
        };

        if !std::path::Path::new(&bin).exists() {
            eprintln!(
                "skipping test_chat_eof_saves: qxfx0 binary not found at {}",
                bin
            );
            return;
        }

        let mut child = std::process::Command::new(&bin)
            .args(["chat", "--session-id", &session_id, "--db", &db_path])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn qxfx0 chat");

        {
            let mut stdin = child.stdin.take().expect("stdin pipe");
            stdin
                .write_all(stdin_payload.as_bytes())
                .expect("write stdin");
            // Dropping `stdin` here closes the pipe → EOF on the child side.
        }

        let output = child.wait_with_output().expect("wait for qxfx0");
        assert!(
            output.status.success(),
            "qxfx0 chat exited non-zero: {:?}\nstderr: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );

        let db = qxfx0_persistence::Persistence::open(&db_path)
            .expect("reopen db written by child");
        let loaded = db
            .load_state(&session_id)
            .expect("load after child exit")
            .expect("session row must exist after EOF exit");
        assert_eq!(loaded.session_id, session_id);
        assert!(
            loaded.dialogue.turn_count >= 1,
            "expected at least one processed turn before EOF"
        );

        let _ = std::fs::remove_file(&db_path);
    }

    /// M7.3 — a corrupted session row must surface as an `Err` rather than
    /// silently being treated as "no state, create a fresh one".
    #[test]
    fn test_corrupted_db() {
        // Write a corrupt row to an isolated on-disk DB by going through a
        // real `Persistence` (which exposes only the typed API), then verify
        // `load_or_create_state` rejects it.
        let tmp = std::env::temp_dir().join(format!(
            "qxfx0-cli-corrupt-{}.db",
            std::process::id()
        ));
        let db_path = tmp.to_string_lossy().to_string();

        // Create the schema by opening the DB once.
        {
            let db = qxfx0_persistence::Persistence::open(&db_path).expect("open");
            // Use the persistence API to insert a valid row first (so the
            // table exists), then overwrite the JSON column with garbage
            // through a side-channel `rusqlite::Connection`.
            db.save_state(
                "corrupt-session",
                &fresh_state("corrupt-session"),
            )
            .expect("initial save");
        }

        // Corrupt the JSON column directly.
        {
            use rusqlite::{params, Connection};
            let conn = Connection::open(&db_path).expect("open conn");
            conn.execute(
                "UPDATE runtime_sessions SET state_json = ?1 WHERE id = ?2",
                params!["{not valid json", "corrupt-session"],
            )
            .expect("corrupt row");
        }

        // Re-open via the typed Persistence and try load_or_create_state.
        let db = qxfx0_persistence::Persistence::open(&db_path).expect("reopen");
        let result = load_or_create_state(&db, "corrupt-session");
        assert!(
            result.is_err(),
            "load_or_create_state must propagate serialization errors, got Ok"
        );

        let _ = std::fs::remove_file(&db_path);
    }
}
