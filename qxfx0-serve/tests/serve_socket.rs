//! Socket-level integration tests for `qxfx0 serve` (ADR-0043 U1).
//!
//! Two contracts:
//! 1. **Byte parity with the CLI journal path.** The same inputs through the
//!    socket and through `run_journal_turn` must produce identical response
//!    text — the daemon is a process-lifetime optimization, never a
//!    behavioral fork.
//! 2. **The warm-turn gate.** After the first turn pays one-time
//!    initialization, every subsequent turn must stay under the daemon
//!    budget (50 ms in release; debug builds get a proportionally looser
//!    sanity bound).

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use qxfx0_codex::journal::run_journal_turn;
use qxfx0_persistence::Persistence;
use qxfx0_pipeline::RendererAuthority;

static DAEMON_SEQ: AtomicUsize = AtomicUsize::new(0);

fn unique_name(tag: &str, ext: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "qxfx0-serve-{tag}-{}-{}.{ext}",
        std::process::id(),
        DAEMON_SEQ.fetch_add(1, Ordering::SeqCst)
    ))
}

struct TestDaemon {
    socket: PathBuf,
    db: PathBuf,
}

impl TestDaemon {
    fn start() -> Self {
        let socket = unique_name("test", "sock");
        let db = unique_name("test", "db");
        let _ = std::fs::remove_file(&socket);
        let listener = qxfx0_serve::bind(&socket).expect("bind test socket");
        let db_path = db.display().to_string();
        std::thread::spawn(move || {
            let _ = qxfx0_serve::serve(listener, &db_path);
        });
        // Wait for the listener to accept.
        for _ in 0..100 {
            if UnixStream::connect(&socket).is_ok() {
                return Self { socket, db };
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("daemon never came up");
    }

    fn request(&self, session_id: &str, text: &str) -> String {
        let stream = UnixStream::connect(&self.socket).expect("connect");
        let mut writer = stream.try_clone().expect("clone");
        let reader = BufReader::new(stream);
        let payload = serde_json::json!({"session_id": session_id, "text": text});
        writeln!(writer, "{payload}").expect("write request");
        writer.flush().expect("flush");
        let line = reader
            .lines()
            .next()
            .expect("daemon closed without a reply")
            .expect("read reply");
        let value: serde_json::Value = serde_json::from_str(&line).expect("reply is json");
        serde_json::to_string(&value).expect("re-encode")
    }
}

impl Drop for TestDaemon {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket);
        let _ = std::fs::remove_file(&self.db);
    }
}

#[test]
fn socket_turns_match_the_cli_journal_path_byte_for_byte() {
    let daemon = TestDaemon::start();
    let cli_db = Persistence::open_memory().expect("in-memory db");
    let inputs = [
        "что такое свобода?",
        "как связаны свобода и ответственность?",
        "справедливым ли было это решение?",
        "что делает решение справедливым?",
        "я считаю, что свобода первична",
    ];
    for text in inputs {
        let served = daemon.request("parity-session", text);
        let served: serde_json::Value = serde_json::from_str(&served).expect("json");
        let expected = run_journal_turn(
            &cli_db,
            "parity-session",
            text,
            19_000, // synthetic epoch day; responses do not depend on it
            RendererAuthority::AuditedPlan,
        )
        .expect("cli journal turn");
        assert_eq!(
            served["response"].as_str().expect("response text"),
            expected,
            "socket and CLI paths diverged on: {text}"
        );
    }
    // The turn counter advanced identically on both sides.
    let served: serde_json::Value =
        serde_json::from_str(&daemon.request("parity-session", "что такое истина?")).expect("json");
    let expected = run_journal_turn(
        &cli_db,
        "parity-session",
        "что такое истина?",
        19_001,
        RendererAuthority::AuditedPlan,
    )
    .expect("cli journal turn");
    assert_eq!(served["response"].as_str().expect("response"), expected);
}

#[test]
fn warm_turns_stay_under_the_daemon_budget() {
    let daemon = TestDaemon::start();
    // Turn 1 pays one-time initialization: the noun blob and seed graph on
    // any prompt, the adjective columnar blob on the first adjective
    // surface — so the warm-up turn uses the adjective prompt deliberately.
    let _ = daemon.request("latency-session", "что делает решение справедливым?");

    let budget = if cfg!(debug_assertions) {
        // Debug builds are not the gate target; keep a sanity bound only.
        Duration::from_millis(2_000)
    } else {
        Duration::from_millis(50)
    };
    let mut worst = Duration::ZERO;
    for turn in 0..20 {
        let text = if turn % 2 == 0 {
            "что делает решение справедливым?"
        } else {
            "как память влияет на личность?"
        };
        let started = Instant::now();
        let reply = daemon.request("latency-session", text);
        let elapsed = started.elapsed();
        assert!(!reply.contains("\"error\""), "unexpected error: {reply}");
        assert!(
            elapsed < budget,
            "warm turn {turn} took {elapsed:?} (budget {budget:?})"
        );
        worst = worst.max(elapsed);
    }
    eprintln!("serve warm-turn worst of 20: {worst:?} (budget {budget:?})");
}

#[test]
fn invalid_session_ids_and_malformed_lines_are_rejected_without_mutation() {
    let daemon = TestDaemon::start();

    let bad_id = daemon.request("bad\u{0}id", "что такое свобода?");
    assert!(
        bad_id.contains("\"error\""),
        "control-char id accepted: {bad_id}"
    );
    let empty_id = daemon.request("", "что такое свобода?");
    assert!(
        empty_id.contains("\"error\""),
        "empty id accepted: {empty_id}"
    );

    // A malformed line gets an error object and the daemon keeps serving.
    let stream = UnixStream::connect(&daemon.socket).expect("connect");
    let mut writer = stream.try_clone().expect("clone");
    let reader = BufReader::new(stream);
    writeln!(writer, "this is not json").expect("write");
    writer.flush().expect("flush");
    let mut lines = reader.lines();
    let malformed = lines.next().expect("reply").expect("line");
    assert!(
        malformed.contains("\"error\""),
        "malformed line accepted: {malformed}"
    );
    writeln!(writer, "{{}}").expect("write");
    writer.flush().expect("flush");
    let missing = lines.next().expect("reply").expect("line");
    assert!(
        missing.contains("\"error\""),
        "empty object accepted: {missing}"
    );

    // And a valid request still works on a fresh connection afterwards.
    let ok = daemon.request("recovery-session", "что такое истина?");
    assert!(!ok.contains("\"error\""), "daemon did not recover: {ok}");
}
