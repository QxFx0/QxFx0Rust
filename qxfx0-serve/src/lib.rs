//! `qxfx0 serve` — the canonical long-lived process (ADR-0043 U1).
//!
//! A synchronous unix-socket server speaking one JSON object per line:
//! a request `{"session_id":"…","text":"…"}` is answered with
//! `{"session_id","turn","response","blocked"}` or `{"error":"…"}`. Every
//! turn is the exact journal sequence the CLI runs — load state, process
//! with `TurnOptions`, stamp the practice calendar, persist — so responses
//! are byte-identical to `qxfx0 turn` on the same session. The daemon does
//! not change determinism; it changes who pays initialization. The
//! morphology blobs and the seed graph are built once per process instead
//! of once per turn, which removes the ~200–450 ms cold-init class from
//! every turn after the first.
//!
//! Concurrency model: one OS thread per connection (std threads only — the
//! workspace stays no-async), and a single database worker thread that owns
//! the only `Persistence` handle. Turns execute strictly sequentially in
//! the worker: there is no cross-connection SQLite locking, no lock
//! hierarchy, and same-session turns are serialized by construction. A
//! hung client blocks only its own connection, never the listener and
//! never other sessions' progress beyond queue order.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::mpsc;

use qxfx0_codex::journal::{load_or_create_state, save_journal_state};
use qxfx0_persistence::Persistence;
use qxfx0_pipeline::{process_turn_with_options, RendererAuthority, TurnInput, TurnOptions};
use qxfx0_types::system_state::SystemState;
use serde::{Deserialize, Serialize};

/// One line in: what the client asks for.
#[derive(Debug, Deserialize)]
pub struct ServeRequest {
    pub session_id: String,
    pub text: String,
}

/// One line out: what a completed turn produced.
#[derive(Debug, Serialize)]
pub struct ServeResponse {
    pub session_id: String,
    pub turn: usize,
    pub response: String,
    pub blocked: bool,
}

/// One line out on a rejected request. A rejected request mutates nothing.
#[derive(Debug, Serialize)]
pub struct ServeError {
    pub error: String,
}

#[derive(Serialize)]
#[serde(untagged)]
enum Reply {
    Turn(ServeResponse),
    Error(ServeError),
}

/// A parsed request plus the channel its reply arrives on.
struct Job {
    request: ServeRequest,
    reply_to: mpsc::Sender<Reply>,
}

/// Session-id policy mirrors the persistence boundary: a turn is rejected
/// without mutation on an empty id, control characters, or length > 128.
fn valid_session_id(session_id: &str) -> bool {
    !session_id.is_empty() && session_id.len() <= 128 && !session_id.chars().any(|c| c.is_control())
}

/// Bind the socket, removing a stale socket file left by a previous run.
/// Refuses to touch the path if something is still listening there.
pub fn bind(socket_path: &Path) -> std::io::Result<UnixListener> {
    if socket_path.exists() {
        // A live listener answers; a stale file does not. Only replace the
        // file when nothing answers.
        if UnixStream::connect(socket_path).is_ok() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AddrInUse,
                format!("another daemon is listening at {}", socket_path.display()),
            ));
        }
        std::fs::remove_file(socket_path)?;
    }
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    UnixListener::bind(socket_path)
}

/// Serve connections forever. Returns only on a fatal accept error (e.g.
/// the listener was closed); per-connection errors are logged to stderr and
/// never stop the daemon.
pub fn serve(listener: UnixListener, db_path: &str) -> std::io::Result<()> {
    let (job_tx, job_rx) = mpsc::channel::<Job>();
    // The single database worker: owns the only Persistence handle, runs
    // one turn at a time, forever.
    let worker_db_path = db_path.to_string();
    std::thread::spawn(move || match Persistence::open(&worker_db_path) {
        Ok(db) => {
            while let Ok(job) = job_rx.recv() {
                let _ = job.reply_to.send(turn_reply(&db, &job.request));
            }
        }
        Err(error) => {
            eprintln!("serve: database open failed: {error}");
            // Drain the queue so every waiting client gets an error reply
            // instead of a hang.
            while let Ok(job) = job_rx.recv() {
                let _ = job
                    .reply_to
                    .send(error_reply(format!("database open failed: {error}")));
            }
        }
    });

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let job_tx = job_tx.clone();
                std::thread::spawn(move || {
                    if let Err(error) = handle_connection(stream, &job_tx) {
                        eprintln!("serve: connection ended: {error}");
                    }
                });
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn handle_connection(stream: UnixStream, job_tx: &mpsc::Sender<Job>) -> std::io::Result<()> {
    let reader = BufReader::new(stream.try_clone()?);
    let mut writer = stream;
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<ServeRequest>(&line) {
            Ok(request) => {
                let (reply_tx, reply_rx) = mpsc::channel();
                match job_tx.send(Job {
                    request,
                    reply_to: reply_tx,
                }) {
                    Ok(()) => match reply_rx.recv() {
                        Ok(reply) => reply,
                        Err(_) => error_reply("database worker is gone".into()),
                    },
                    Err(_) => error_reply("database worker is gone".into()),
                }
            }
            Err(error) => error_reply(format!("malformed request line: {error}")),
        };
        writeln!(writer, "{}", serde_json::to_string(&reply).map_err(io_err)?)?;
        writer.flush()?;
    }
    Ok(())
}

fn turn_reply(db: &Persistence, request: &ServeRequest) -> Reply {
    if !valid_session_id(&request.session_id) {
        return error_reply("session id must be 1-128 chars without control characters".into());
    }
    let mut state: SystemState = match load_or_create_state(db, &request.session_id) {
        Ok(state) => state,
        Err(error) => return error_reply(format!("state load failed: {error}")),
    };
    let input = TurnInput {
        raw_text: request.text.clone(),
        session_id: request.session_id.clone(),
    };
    let output = process_turn_with_options(
        &input,
        &mut state,
        TurnOptions::new().with_renderer(RendererAuthority::AuditedPlan),
    );
    if let Err(error) = save_journal_state(db, &request.session_id, &mut state) {
        return error_reply(format!("state save failed: {error}"));
    }
    Reply::Turn(ServeResponse {
        session_id: request.session_id.clone(),
        turn: state.dialogue.turn_count,
        response: output.response,
        blocked: output.blocked,
    })
}

fn error_reply(message: String) -> Reply {
    Reply::Error(ServeError { error: message })
}

fn io_err(error: serde_json::Error) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, error)
}
