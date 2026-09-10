use qxfx0_types::system_state::SystemState;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use thiserror::Error;

mod db;

const MAX_THESIS_STATE_JSON_BYTES: usize = 4 * 1024 * 1024;
// The learning bridge stores (ADR-0043 U4) are bounded at the SQL edge too:
// the runtime-edge cap already limits a healthy store, but a blob bound
// keeps a corrupt or adversarial worker from growing a session row without
// limit. The quarantine is per-row sized and row-counted; a full ledger is
// a hard error (fail-closed) rather than silent eviction.
const MAX_BRIDGE_EDGES_JSON_BYTES: usize = 4 * 1024 * 1024;
const MAX_BRIDGE_QUARANTINE_ENTRY_BYTES: usize = 64 * 1024;
const MAX_BRIDGE_QUARANTINE_ROWS: i64 = 4_096;
// A promotion overlay is bounded at the SQL edge like every other persisted
// structure: one overlay may not smuggle an unbounded predicate set past the
// human review that makes it a semantic-authority fact.
const MAX_PROMOTION_OVERLAY_JSON_BYTES: usize = 1024 * 1024;

fn perspective_authority_violations(state: &SystemState) -> Vec<String> {
    let mut violations = Vec::new();
    let active_pack = qxfx0_semantic::active_pack_set();
    let perspective_is_empty = state.semantic.perspective.opinions.is_empty()
        && state.semantic.perspective.episodes.is_empty();
    if state.semantic.pack_set_fingerprint.is_empty() && !perspective_is_empty {
        violations.push("non-empty Perspective has no knowledge-pack fingerprint".into());
        return violations;
    }
    if !state.semantic.pack_set_fingerprint.is_empty()
        && state.semantic.pack_set_fingerprint != active_pack.fingerprint()
    {
        violations.push("active knowledge-pack fingerprint mismatch".into());
        return violations;
    }
    violations.extend(
        qxfx0_self::fact_perspective::validate_perspective_against_pack(
            &state.semantic.perspective,
            active_pack,
        ),
    );
    let thesis = &state.semantic.thesis_state;
    if !thesis.is_empty() {
        if thesis.pack_fingerprint != active_pack.fingerprint() {
            violations.push("thesis projection knowledge-pack fingerprint mismatch".into());
        }
        for digest in &thesis.projected_digests {
            if !active_pack.overlay_theses().contains_key(digest) {
                violations.push(format!(
                    "thesis projection references unknown digest {digest}"
                ));
            }
        }
        for (id, lifecycle) in &thesis.lifecycles {
            if let Some(head) = lifecycle.active_head {
                match active_pack.overlay_theses().get(&head) {
                    Some(metadata) if metadata.thesis_id == id.as_str() => {}
                    _ => violations.push(format!(
                        "thesis lifecycle {id} is not bound to the active catalog"
                    )),
                }
            }
        }
    }
    violations
}

#[derive(Error, Debug)]
pub enum PersistenceError {
    #[error("SQLite error: {0}")]
    SQLite(#[from] rusqlite::Error),
    #[error("Migration error: {0}")]
    Migration(#[from] db::migrations::MigrationError),
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("State not found: {0}")]
    NotFound(String),
    #[error("Invalid state: {0}")]
    InvalidState(String),
    #[error("Backup error: {0}")]
    Backup(String),
}

/// Persistence layer — SQLite session state storage.
pub struct Persistence {
    conn: Connection,
}

/// Timing evidence for one SQLite-backed state save.
///
/// `sqlite_write_lock_ms` spans the first write statement, where SQLite may
/// wait to acquire its write lock. `sqlite_commit_checkpoint_ms` measures the
/// commit path, including an automatic WAL checkpoint if SQLite performs one;
/// no explicit checkpoint is issued for diagnostics.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct SaveStateTimings {
    /// State validation and JSON serialization before opening the transaction.
    pub serialization_ms: u64,
    /// Transaction creation before the first write statement.
    pub sqlite_transaction_begin_ms: u64,
    /// First write statement, including any SQLite write-lock wait.
    pub sqlite_write_lock_ms: u64,
    /// Remaining normalized-table writes in the transaction.
    pub sqlite_remaining_writes_ms: u64,
    /// Commit duration, including any automatic WAL checkpoint work.
    pub sqlite_commit_checkpoint_ms: u64,
    /// Total state-save duration.
    pub total_ms: u64,
}

impl SaveStateTimings {
    fn elapsed_ms(started: Instant) -> u64 {
        started.elapsed().as_millis().try_into().unwrap_or(u64::MAX)
    }
}

impl Persistence {
    fn configure_connection(conn: &Connection) -> Result<(), PersistenceError> {
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        // In-memory SQLite keeps the `memory` journal mode; file databases
        // switch to WAL. Both outcomes are valid and the PRAGMA is harmless.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        Ok(())
    }

    /// Open or create a database at the given path.
    pub fn open(path: &str) -> Result<Self, PersistenceError> {
        let mut conn = Connection::open(path)?;
        Self::configure_connection(&conn)?;
        db::migrations::apply_migrations(&mut conn)?;

        Ok(Persistence { conn })
    }

    /// Open an in-memory database (for tests).
    pub fn open_memory() -> Result<Self, PersistenceError> {
        let mut conn = Connection::open_in_memory()?;
        Self::configure_connection(&conn)?;
        db::migrations::apply_migrations(&mut conn)?;

        Ok(Persistence { conn })
    }

    /// Create a consistent online backup without migrating or writing to the
    /// source database. The destination must not exist.
    ///
    /// SQLite writes into a uniquely owned partial file first. After the
    /// connections are closed, the copy is verified, synced, atomically
    /// renamed, and (on Unix) its parent directory is synced. A failure before
    /// the rename removes only this invocation's partial file.
    pub fn backup_database(source: &str, destination: &str) -> Result<(), PersistenceError> {
        static PARTIAL_SEQUENCE: AtomicU64 = AtomicU64::new(0);

        fn backup_error(context: &str, error: impl std::fmt::Display) -> PersistenceError {
            PersistenceError::Backup(format!("{context}: {error}"))
        }

        fn close_connection(connection: Connection, name: &str) -> Result<(), PersistenceError> {
            connection
                .close()
                .map_err(|(_, error)| backup_error(&format!("closing {name} connection"), error))
        }

        #[cfg(unix)]
        fn sync_parent_directory(parent: &Path) -> Result<(), PersistenceError> {
            File::open(parent)
                .and_then(|directory| directory.sync_all())
                .map_err(|error| backup_error("syncing backup parent directory", error))
        }

        #[cfg(not(unix))]
        fn sync_parent_directory(_parent: &Path) -> Result<(), PersistenceError> {
            Ok(())
        }

        let source_path = Path::new(source);
        let destination_path = Path::new(destination);

        if !source_path.is_file() {
            return Err(PersistenceError::Backup(format!(
                "source database '{}' does not exist or is not a file",
                source_path.display()
            )));
        }
        if destination_path.exists() {
            return Err(PersistenceError::Backup(format!(
                "destination '{}' already exists",
                destination_path.display()
            )));
        }

        let source_canonical = std::fs::canonicalize(source_path)
            .map_err(|error| backup_error("resolving source database", error))?;
        let destination_parent = destination_path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let destination_parent = std::fs::canonicalize(destination_parent)
            .map_err(|error| backup_error("resolving destination directory", error))?;
        let destination_name = destination_path.file_name().ok_or_else(|| {
            PersistenceError::Backup("destination must include a file name".into())
        })?;
        let destination_canonical = destination_parent.join(destination_name);

        if source_canonical == destination_canonical {
            return Err(PersistenceError::Backup(
                "source and destination resolve to the same file".into(),
            ));
        }

        let partial_path = (0..100)
            .find_map(|_| {
                let sequence = PARTIAL_SEQUENCE.fetch_add(1, Ordering::Relaxed);
                let partial_name = format!(
                    ".{}.partial-{}-{sequence}",
                    destination_name.to_string_lossy(),
                    std::process::id()
                );
                let candidate = destination_parent.join(partial_name);
                match OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&candidate)
                {
                    Ok(file) => {
                        drop(file);
                        Some(Ok(candidate))
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => None,
                    Err(error) => Some(Err(backup_error("creating partial backup", error))),
                }
            })
            .transpose()?
            .ok_or_else(|| {
                PersistenceError::Backup("could not allocate partial backup file".into())
            })?;

        let result = (|| {
            let source_connection = Connection::open_with_flags(
                &source_canonical,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .map_err(|error| backup_error("opening source database", error))?;
            source_connection
                .busy_timeout(Duration::from_secs(5))
                .map_err(|error| backup_error("configuring source database", error))?;

            let mut destination_connection = Connection::open_with_flags(
                &partial_path,
                OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .map_err(|error| backup_error("opening partial backup", error))?;
            {
                let backup =
                    rusqlite::backup::Backup::new(&source_connection, &mut destination_connection)
                        .map_err(|error| backup_error("starting SQLite backup", error))?;
                backup
                    .run_to_completion(128, Duration::from_millis(10), None)
                    .map_err(|error| backup_error("copying SQLite backup", error))?;
            }
            // The source commonly uses WAL, and the backup copies that
            // persistent journal-mode setting. Return the standalone backup to
            // DELETE mode before closing so integrity verification cannot
            // leave WAL/SHM sidecars behind.
            destination_connection
                .pragma_update(None, "journal_mode", "DELETE")
                .map_err(|error| backup_error("finalizing backup journal mode", error))?;

            close_connection(destination_connection, "partial backup")?;
            close_connection(source_connection, "source")?;

            let integrity_connection = Connection::open_with_flags(
                &partial_path,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .map_err(|error| backup_error("opening backup for integrity check", error))?;
            let quick_check_results = {
                let mut statement = integrity_connection
                    .prepare("PRAGMA quick_check")
                    .map_err(|error| backup_error("preparing backup integrity check", error))?;
                let rows = statement
                    .query_map([], |row| row.get::<_, String>(0))
                    .map_err(|error| backup_error("running backup integrity check", error))?;
                rows.collect::<Result<Vec<_>, _>>()
                    .map_err(|error| backup_error("reading backup integrity check", error))?
            };
            if quick_check_results.as_slice() != ["ok"] {
                return Err(PersistenceError::Backup(format!(
                    "destination quick_check failed: {}",
                    quick_check_results.join("; ")
                )));
            }
            close_connection(integrity_connection, "integrity check")?;

            File::open(&partial_path)
                .and_then(|file| file.sync_all())
                .map_err(|error| backup_error("syncing partial backup", error))?;

            // The API has always required a non-existent destination. Recheck
            // immediately before rename so a failure never replaces a backup
            // that appeared while SQLite was copying.
            if destination_canonical.exists() {
                return Err(PersistenceError::Backup(format!(
                    "destination '{}' already exists",
                    destination_canonical.display()
                )));
            }
            std::fs::rename(&partial_path, &destination_canonical)
                .map_err(|error| backup_error("renaming completed backup", error))?;
            sync_parent_directory(&destination_parent)?;
            Ok(())
        })();

        match result {
            Err(error) if partial_path.exists() => {
                if let Err(cleanup_error) = std::fs::remove_file(&partial_path) {
                    return Err(PersistenceError::Backup(format!(
                        "{error}; additionally failed to remove partial backup '{}': {cleanup_error}",
                        partial_path.display()
                    )));
                }
                Err(error)
            }
            result => result,
        }
    }

    /// Save system state for a session across normalized tables.
    ///
    /// The state is split into three tables:
    /// - `runtime_sessions`: dialogue, last_turn_decision, governance_log
    /// - `session_graphs`: runtime graph atoms/edges
    /// - `session_semantic`: field, essence, adjunction, commitments
    ///
    /// All writes happen in a single transaction so a session is never left
    /// in a half-persisted state.
    pub fn save_state(
        &self,
        session_id: &str,
        state: &SystemState,
    ) -> Result<(), PersistenceError> {
        self.save_state_with_timings(session_id, state).map(|_| ())
    }

    /// Save session state and return observational timing for SQLite work.
    ///
    /// This performs exactly the same validation, statements and transaction
    /// as [`Self::save_state`]. It does not add a checkpoint or alter the
    /// persisted schema/state format.
    pub fn save_state_with_timings(
        &self,
        session_id: &str,
        state: &SystemState,
    ) -> Result<SaveStateTimings, PersistenceError> {
        let total_started = Instant::now();
        let serialization_started = Instant::now();
        if state.session_id != session_id {
            return Err(PersistenceError::InvalidState(format!(
                "storage session '{}' differs from state session '{}'",
                session_id, state.session_id
            )));
        }
        let mut violations = state.validate();
        violations.extend(perspective_authority_violations(state));
        if !violations.is_empty() {
            return Err(PersistenceError::InvalidState(violations.join("; ")));
        }

        let atoms_json = serde_json::to_string(&state.semantic.runtime_graph.atoms)
            .map_err(|e| PersistenceError::Serialization(e.to_string()))?;
        let edges_json = serde_json::to_string(&state.semantic.runtime_graph.edges)
            .map_err(|e| PersistenceError::Serialization(e.to_string()))?;

        let field_json = serde_json::to_string(&state.semantic.field)
            .map_err(|e| PersistenceError::Serialization(e.to_string()))?;
        let essence_json = serde_json::to_string(&state.semantic.essence)
            .map_err(|e| PersistenceError::Serialization(e.to_string()))?;
        let adjunction_json = serde_json::to_string(&state.semantic.adjunction)
            .map_err(|e| PersistenceError::Serialization(e.to_string()))?;
        let commitments_json = serde_json::to_string(&state.semantic.semantic_commitments)
            .map_err(|e| PersistenceError::Serialization(e.to_string()))?;
        let stance_provenance_json = serde_json::to_string(&state.semantic.stance_provenance)
            .map_err(|e| PersistenceError::Serialization(e.to_string()))?;
        let perspective_json = serde_json::to_string(&state.semantic.perspective)
            .map_err(|e| PersistenceError::Serialization(e.to_string()))?;
        let thesis_state_json = serde_json::to_string(&state.semantic.thesis_state)
            .map_err(|e| PersistenceError::Serialization(e.to_string()))?;
        if thesis_state_json.len() > MAX_THESIS_STATE_JSON_BYTES {
            return Err(PersistenceError::InvalidState(format!(
                "thesis_state_json exceeds {MAX_THESIS_STATE_JSON_BYTES} bytes"
            )));
        }
        // ADR-0043 U2 shadow column: NULL carries "no V2 trajectory yet", the
        // same convention the loader reverses. The value is opaque here; the
        // pipeline owns its typed shape.
        let essence_v2_json = match &state.semantic.essence_v2 {
            Some(value) => Some(
                serde_json::to_string(value)
                    .map_err(|e| PersistenceError::Serialization(e.to_string()))?,
            ),
            None => None,
        };
        // ADR-0043 U3 blanket column: NULL carries "no V2 blanket yet",
        // the same convention as the shadow trajectory. Opaque here; the
        // pipeline owns the typed shape.
        let blanket_v2_json = match &state.semantic.blanket_v2 {
            Some(value) => Some(
                serde_json::to_string(value)
                    .map_err(|e| PersistenceError::Serialization(e.to_string()))?,
            ),
            None => None,
        };

        let state_json = serde_json::to_string(state)
            .map_err(|e| PersistenceError::Serialization(e.to_string()))?;
        let serialization_ms = SaveStateTimings::elapsed_ms(serialization_started);

        let transaction_started = Instant::now();
        let tx = self.conn.unchecked_transaction()?;
        let sqlite_transaction_begin_ms = SaveStateTimings::elapsed_ms(transaction_started);

        // Legacy monolithic row: kept for backward compatibility until v7 migration.
        let first_write_started = Instant::now();
        tx.execute(
            "INSERT INTO runtime_sessions (id, state_json, last_active, turn_count)
             VALUES (?1, ?2, datetime('now'), ?3)
             ON CONFLICT(id) DO UPDATE SET
                state_json=excluded.state_json,
                last_active=datetime('now'),
                turn_count=excluded.turn_count",
            params![session_id, state_json, state.dialogue.turn_count],
        )?;
        let sqlite_write_lock_ms = SaveStateTimings::elapsed_ms(first_write_started);

        let remaining_writes_started = Instant::now();
        tx.execute(
            "INSERT INTO session_graphs (session_id, atoms_json, edges_json)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(session_id) DO UPDATE SET
                atoms_json=excluded.atoms_json,
                edges_json=excluded.edges_json",
            params![session_id, atoms_json, edges_json],
        )?;

        tx.execute(
            "INSERT INTO session_semantic (session_id, field_json, essence_json, adjunction_json, commitments_json, stance_provenance_json, perspective_json, thesis_state_json, essence_v2_json, blanket_v2_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(session_id) DO UPDATE SET
                field_json=excluded.field_json,
                essence_json=excluded.essence_json,
                adjunction_json=excluded.adjunction_json,
                commitments_json=excluded.commitments_json,
                stance_provenance_json=excluded.stance_provenance_json,
                perspective_json=excluded.perspective_json,
                thesis_state_json=excluded.thesis_state_json,
                essence_v2_json=excluded.essence_v2_json,
                blanket_v2_json=excluded.blanket_v2_json",
            params![session_id, field_json, essence_json, adjunction_json, commitments_json, stance_provenance_json, perspective_json, thesis_state_json, essence_v2_json, blanket_v2_json],
        )?;
        let sqlite_remaining_writes_ms = SaveStateTimings::elapsed_ms(remaining_writes_started);

        let commit_started = Instant::now();
        tx.commit()?;
        let sqlite_commit_checkpoint_ms = SaveStateTimings::elapsed_ms(commit_started);

        Ok(SaveStateTimings {
            serialization_ms,
            sqlite_transaction_begin_ms,
            sqlite_write_lock_ms,
            sqlite_remaining_writes_ms,
            sqlite_commit_checkpoint_ms,
            total_ms: SaveStateTimings::elapsed_ms(total_started),
        })
    }

    /// Load system state for a session.
    ///
    /// First attempts the normalized split tables (session_graphs + session_semantic).
    /// If those are absent, falls back to the legacy `state_json` blob for backward
    /// compatibility with databases created before the v6 migration.
    pub fn load_state(&self, session_id: &str) -> Result<Option<SystemState>, PersistenceError> {
        // All reads share one snapshot: a concurrent writer must not be able
        // to tear the normalized tables, the session row and the legacy blob
        // apart mid-load. BEGIN DEFERRED takes the snapshot at the first read
        // and WAL readers never block writers.
        let tx = self.conn.unchecked_transaction()?;
        let state = Self::load_state_snapshot(&tx, session_id)?;
        tx.commit()?;
        Ok(state)
    }

    /// Single-snapshot state load, executed inside the caller's transaction.
    fn load_state_snapshot(
        conn: &Connection,
        session_id: &str,
    ) -> Result<Option<SystemState>, PersistenceError> {
        // Try normalized tables first.
        let graph = conn
            .query_row(
                "SELECT atoms_json, edges_json FROM session_graphs WHERE session_id = ?1",
                params![session_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;

        let semantic = conn
            .query_row(
                "SELECT field_json, essence_json, adjunction_json, commitments_json, stance_provenance_json, perspective_json, thesis_state_json, essence_v2_json, blanket_v2_json
                 FROM session_semantic WHERE session_id = ?1",
                params![session_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, Option<String>>(6)?,
                        row.get::<_, Option<String>>(7)?,
                        row.get::<_, Option<String>>(8)?,
                    ))
                },
            )
            .optional()?;

        if let (
            Some((atoms_json, edges_json)),
            Some((
                field_json,
                essence_json,
                adjunction_json,
                commitments_json,
                stance_provenance_json,
                perspective_json,
                thesis_state_json,
                essence_v2_json,
                blanket_v2_json,
            )),
        ) = (graph, semantic)
        {
            let session = conn
                .query_row(
                    "SELECT state_json FROM runtime_sessions WHERE id = ?1",
                    params![session_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;

            let (dialogue, pack_set_fingerprint, last_turn_decision, governance_log) = match session
            {
                Some(state_json) => {
                    let legacy: SystemState = serde_json::from_str(&state_json)
                        .map_err(|e| PersistenceError::Serialization(e.to_string()))?;
                    (
                        legacy.dialogue,
                        legacy.semantic.pack_set_fingerprint,
                        legacy.last_turn_decision,
                        legacy.governance_log,
                    )
                }
                None => return Ok(None),
            };

            let atoms = serde_json::from_str(&atoms_json)
                .map_err(|e| PersistenceError::Serialization(e.to_string()))?;
            let edges = serde_json::from_str(&edges_json)
                .map_err(|e| PersistenceError::Serialization(e.to_string()))?;

            let field = serde_json::from_str(&field_json)
                .map_err(|e| PersistenceError::Serialization(e.to_string()))?;
            let essence = serde_json::from_str(&essence_json)
                .map_err(|e| PersistenceError::Serialization(e.to_string()))?;
            let adjunction = serde_json::from_str(&adjunction_json)
                .map_err(|e| PersistenceError::Serialization(e.to_string()))?;
            let semantic_commitments = match commitments_json.as_deref() {
                Some("null") | Some("") | None => None,
                Some(json) => Some(
                    serde_json::from_str(json)
                        .map_err(|e| PersistenceError::Serialization(e.to_string()))?,
                ),
            };
            let stance_provenance = match stance_provenance_json.as_deref() {
                Some("null") | Some("") | None => Default::default(),
                Some(json) => serde_json::from_str(json)
                    .map_err(|e| PersistenceError::Serialization(e.to_string()))?,
            };
            let thesis_state = match thesis_state_json.as_deref() {
                Some("null") | Some("") | None => Default::default(),
                Some(json) => serde_json::from_str(json)
                    .map_err(|e| PersistenceError::Serialization(e.to_string()))?,
            };
            // Shadow V2 trajectory: absent/empty stays None (pre-U2 session);
            // a present value is passed through as-is. The pipeline decodes it
            // typed and fails closed there.
            let essence_v2 = match essence_v2_json.as_deref() {
                Some("null") | Some("") | None => None,
                Some(json) => Some(
                    serde_json::from_str(json)
                        .map_err(|e| PersistenceError::Serialization(e.to_string()))?,
                ),
            };
            // ADR-0043 U3 blanket record: same convention as the shadow
            // trajectory — absent/empty stays None (pre-U3 session).
            let blanket_v2 = match blanket_v2_json.as_deref() {
                Some("null") | Some("") | None => None,
                Some(json) => Some(
                    serde_json::from_str(json)
                        .map_err(|e| PersistenceError::Serialization(e.to_string()))?,
                ),
            };
            let perspective = match perspective_json.as_deref() {
                Some("null") | Some("") | None => Default::default(),
                Some(json) => serde_json::from_str(json)
                    .map_err(|e| PersistenceError::Serialization(e.to_string()))?,
            };

            let state = SystemState {
                session_id: session_id.into(),
                dialogue,
                semantic: {
                    let mut runtime_graph = qxfx0_types::atom::AtomGraph {
                        atoms,
                        edges,
                        edges_from: BTreeMap::new(),
                        edges_to: BTreeMap::new(),
                    };
                    runtime_graph.rebuild_indices();
                    qxfx0_types::system_state::SemanticState {
                        field,
                        runtime_graph,
                        pack_set_fingerprint,
                        semantic_commitments,
                        essence,
                        adjunction,
                        stance_provenance,
                        perspective,
                        thesis_state,
                        essence_v2,
                        blanket_v2,
                        cached_edge_count: 0,
                        cached_network: None,
                    }
                },
                last_turn_decision,
                governance_log,
            };
            let mut violations = state.validate();
            violations.extend(perspective_authority_violations(&state));
            if !violations.is_empty() {
                return Err(PersistenceError::InvalidState(violations.join("; ")));
            }
            return Ok(Some(state));
        }

        // Legacy fallback.
        let mut stmt =
            conn.prepare_cached("SELECT state_json FROM runtime_sessions WHERE id = ?1")?;

        let result = stmt.query_row(params![session_id], |row| {
            let json: String = row.get(0)?;
            Ok(json)
        });

        match result {
            Ok(json) => {
                let mut state: SystemState = serde_json::from_str(&json)
                    .map_err(|e| PersistenceError::Serialization(e.to_string()))?;
                // The row key is authoritative for legacy blobs. Rebuild
                // derived graph/cache data before enforcing current invariants.
                state.session_id = session_id.into();
                state.semantic.runtime_graph.rebuild_indices();
                state.semantic.cached_edge_count = 0;
                state.semantic.cached_network = None;
                let mut violations = state.validate();
                violations.extend(perspective_authority_violations(&state));
                if !violations.is_empty() {
                    return Err(PersistenceError::InvalidState(violations.join("; ")));
                }
                Ok(Some(state))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(PersistenceError::SQLite(e)),
        }
    }

    /// List all session IDs.
    pub fn list_sessions(&self) -> Result<Vec<String>, PersistenceError> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT id FROM runtime_sessions ORDER BY turn_count DESC, id ASC")?;

        let sessions = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            Ok(id)
        })?;

        sessions
            .collect::<Result<Vec<_>, _>>()
            .map_err(PersistenceError::SQLite)
    }

    /// Delete a session.
    pub fn delete_session(&self, session_id: &str) -> Result<(), PersistenceError> {
        // Explicit child deletes also clean databases whose v6 tables were
        // created before foreign-key constraints were introduced.
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM session_graphs WHERE session_id = ?1",
            params![session_id],
        )?;
        tx.execute(
            "DELETE FROM session_semantic WHERE session_id = ?1",
            params![session_id],
        )?;
        tx.execute(
            "DELETE FROM session_bridge_edges WHERE session_id = ?1",
            params![session_id],
        )?;
        tx.execute(
            "DELETE FROM session_bridge_quarantine WHERE session_id = ?1",
            params![session_id],
        )?;
        tx.execute(
            "DELETE FROM runtime_sessions WHERE id = ?1",
            params![session_id],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Read the serialized runtime-bridge edge store for a session. The
    /// blob is opaque here (schema v13): the bridge crate owns its typed
    /// shape. `None` means the bridge never touched this session (pre-v13
    /// databases and untouched sessions both carry no row).
    pub fn load_bridge_edges(&self, session_id: &str) -> Result<Option<String>, PersistenceError> {
        let edges_json: Option<String> = self
            .conn
            .query_row(
                "SELECT edges_json FROM session_bridge_edges WHERE session_id = ?1",
                params![session_id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(edges_json)
    }

    /// Replace the serialized runtime-bridge edge store for a session
    /// (whole-store write, the worker's atomic boundary). Writing `None`
    /// clears the row so a drained-to-nothing store is indistinguishable
    /// from a never-touched session.
    pub fn save_bridge_edges(
        &self,
        session_id: &str,
        edges_json: Option<&str>,
    ) -> Result<(), PersistenceError> {
        let tx = self.conn.unchecked_transaction()?;
        match edges_json {
            Some(json) => {
                if json.len() > MAX_BRIDGE_EDGES_JSON_BYTES {
                    return Err(PersistenceError::InvalidState(format!(
                        "bridge edges_json exceeds {MAX_BRIDGE_EDGES_JSON_BYTES} bytes"
                    )));
                }
                tx.execute(
                    "INSERT INTO session_bridge_edges (session_id, edges_json)
                     VALUES (?1, ?2)
                     ON CONFLICT(session_id) DO UPDATE SET edges_json=excluded.edges_json",
                    params![session_id, json],
                )?;
            }
            None => {
                tx.execute(
                    "DELETE FROM session_bridge_edges WHERE session_id = ?1",
                    params![session_id],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Append one serialized quarantine entry for a session at the next
    /// monotonic `seq` (the ledger is append-ordered so the U5 review queue
    /// has a stable cursor). Returns the assigned `seq`.
    pub fn enqueue_bridge_quarantine(
        &self,
        session_id: &str,
        entry_json: &str,
    ) -> Result<i64, PersistenceError> {
        if entry_json.len() > MAX_BRIDGE_QUARANTINE_ENTRY_BYTES {
            return Err(PersistenceError::InvalidState(format!(
                "bridge quarantine entry exceeds {MAX_BRIDGE_QUARANTINE_ENTRY_BYTES} bytes"
            )));
        }
        let tx = self.conn.unchecked_transaction()?;
        let next_seq: i64 = tx.query_row(
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM session_bridge_quarantine WHERE session_id = ?1",
            params![session_id],
            |row| row.get(0),
        )?;
        let count: i64 = tx.query_row(
            "SELECT COUNT(*) FROM session_bridge_quarantine WHERE session_id = ?1",
            params![session_id],
            |row| row.get(0),
        )?;
        if count >= MAX_BRIDGE_QUARANTINE_ROWS {
            return Err(PersistenceError::InvalidState(format!(
                "bridge quarantine is full at {MAX_BRIDGE_QUARANTINE_ROWS} rows"
            )));
        }
        tx.execute(
            "INSERT INTO session_bridge_quarantine (session_id, seq, entry_json) VALUES (?1, ?2, ?3)",
            params![session_id, next_seq, entry_json],
        )?;
        tx.commit()?;
        Ok(next_seq)
    }

    /// Load every quarantine entry for a session in `(seq)` order, as
    /// `(seq, entry_json)` pairs. The U5 operator review queue reads this
    /// and nothing else; the turn path never does.
    pub fn load_bridge_quarantine(
        &self,
        session_id: &str,
    ) -> Result<Vec<(i64, String)>, PersistenceError> {
        let mut statement = self.conn.prepare_cached(
            "SELECT seq, entry_json FROM session_bridge_quarantine WHERE session_id = ?1 ORDER BY seq ASC",
        )?;
        let rows = statement.query_map(params![session_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut entries = Vec::new();
        for row in rows {
            entries.push(row?);
        }
        Ok(entries)
    }

    /// Clear every quarantine entry for a session (called after the U5
    /// review pass has released or dismissed them).
    pub fn clear_bridge_quarantine(&self, session_id: &str) -> Result<(), PersistenceError> {
        self.conn.execute(
            "DELETE FROM session_bridge_quarantine WHERE session_id = ?1",
            params![session_id],
        )?;
        Ok(())
    }

    /// Store a serialized promotion overlay (schema v14). The blob is opaque
    /// here — the bridge owns the lifecycle machine and the checksum; the
    /// `status` column mirrors the embedded state so the CHECK constraint
    /// rejects an illegal declared status. An existing version is *never*
    /// rewritten by this path: the version is content-addressed, so a second
    /// draft of unchanged evidence is the same version (idempotent no-op,
    /// `Ok(false)`), while lifecycle transitions go through
    /// `replace_promotion_overlay_if_matches`. A passed checksum that
    /// disagrees with the stored row means an inconsistent caller (a
    /// version that does not address its own content) and fails closed.
    #[allow(clippy::too_many_arguments)] // flat SQL row shape; the bridge owns the struct
    pub fn save_promotion_overlay(
        &self,
        version: &str,
        status: &str,
        snapshot_id: &str,
        parent_version: Option<&str>,
        checksum: &str,
        overlay_json: &str,
        created_at: i64,
    ) -> Result<bool, PersistenceError> {
        if overlay_json.len() > MAX_PROMOTION_OVERLAY_JSON_BYTES {
            return Err(PersistenceError::InvalidState(format!(
                "promotion overlay_json exceeds {MAX_PROMOTION_OVERLAY_JSON_BYTES} bytes"
            )));
        }
        let tx = self.conn.unchecked_transaction()?;
        let existing: Option<(String, String)> = tx
            .query_row(
                "SELECT status, checksum FROM promotion_overlays WHERE version = ?1",
                params![version],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        match existing {
            Some((_, stored_checksum)) => {
                if stored_checksum != checksum {
                    return Err(PersistenceError::InvalidState(format!(
                        "promotion overlay {version} is stored under a different checksum (caller inconsistency)"
                    )));
                }
                let _ = status;
                tx.commit()?;
                Ok(false)
            }
            None => {
                tx.execute(
                    "INSERT INTO promotion_overlays
                        (version, status, snapshot_id, parent_version, checksum, overlay_json, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![version, status, snapshot_id, parent_version, checksum, overlay_json, created_at],
                )?;
                tx.commit()?;
                Ok(true)
            }
        }
    }

    /// Compare-and-swap a stored overlay to its next lifecycle row (the
    /// bridge's state machine validates the transition; this guard makes a
    /// lost update impossible: a concurrent approve between load and replace
    /// fails instead of silently overwriting). The content address is pinned:
    /// `checksum` must equal the stored one — a status transition never
    /// changes the predicate set it releases (a Released overlay is
    /// immutable).
    pub fn replace_promotion_overlay_if_matches(
        &self,
        version: &str,
        expected_overlay_json: &str,
        new_status: &str,
        new_overlay_json: &str,
        checksum: &str,
    ) -> Result<(), PersistenceError> {
        if new_overlay_json.len() > MAX_PROMOTION_OVERLAY_JSON_BYTES {
            return Err(PersistenceError::InvalidState(format!(
                "promotion overlay_json exceeds {MAX_PROMOTION_OVERLAY_JSON_BYTES} bytes"
            )));
        }
        let tx = self.conn.unchecked_transaction()?;
        let current: Option<(String, String)> = tx
            .query_row(
                "SELECT overlay_json, checksum FROM promotion_overlays WHERE version = ?1",
                params![version],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        let Some((stored_json, stored_checksum)) = current else {
            return Err(PersistenceError::NotFound(format!(
                "promotion overlay {version}"
            )));
        };
        if stored_json != expected_overlay_json {
            return Err(PersistenceError::InvalidState(format!(
                "promotion overlay {version} changed concurrently (CAS mismatch)"
            )));
        }
        if stored_checksum != checksum {
            return Err(PersistenceError::InvalidState(format!(
                "promotion overlay {version} content address changed mid-lifecycle (immutability violation)"
            )));
        }
        tx.execute(
            "UPDATE promotion_overlays SET status = ?2, overlay_json = ?3 WHERE version = ?1",
            params![version, new_status, new_overlay_json],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Load a promotion overlay by version, as `(status, overlay_json)`.
    pub fn load_promotion_overlay(
        &self,
        version: &str,
    ) -> Result<Option<(String, String)>, PersistenceError> {
        let row = self
            .conn
            .query_row(
                "SELECT status, overlay_json FROM promotion_overlays WHERE version = ?1",
                params![version],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        Ok(row)
    }

    /// List every stored overlay version with its status, newest creation
    /// first (`promotion list` shows the human the whole journal).
    pub fn list_promotion_overlays(&self) -> Result<Vec<(String, String, i64)>, PersistenceError> {
        let mut statement = self.conn.prepare_cached(
            "SELECT version, status, created_at FROM promotion_overlays
             ORDER BY created_at DESC, version ASC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;
        let mut overlays = Vec::new();
        for row in rows {
            overlays.push(row?);
        }
        Ok(overlays)
    }

    /// The active overlay version (schema v14 singleton), if any.
    pub fn load_active_promotion_overlay(&self) -> Result<Option<String>, PersistenceError> {
        let version: Option<String> = self
            .conn
            .query_row(
                "SELECT overlay_version FROM promotion_active WHERE singleton = 1",
                [],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        Ok(version)
    }

    /// Move the active pointer to a stored overlay version (`release`
    /// materializes the artifact and this makes it current; `rollback`
    /// retires it). `overlay_version = None` clears the pointer at the root.
    /// The target must exist and must be Released — pointing at a Draft would
    /// promote unreviewed evidence into semantic authority.
    pub fn set_active_promotion_overlay(
        &self,
        overlay_version: Option<&str>,
        updated_at: i64,
    ) -> Result<(), PersistenceError> {
        let tx = self.conn.unchecked_transaction()?;
        if let Some(version) = overlay_version {
            let status: Option<String> = tx
                .query_row(
                    "SELECT status FROM promotion_overlays WHERE version = ?1",
                    params![version],
                    |row| row.get(0),
                )
                .optional()?;
            match status.as_deref() {
                Some("Released") => {}
                Some(other) => {
                    return Err(PersistenceError::InvalidState(format!(
                        "promotion overlay {version} has status {other:?}, not Released"
                    )));
                }
                None => {
                    return Err(PersistenceError::NotFound(format!(
                        "promotion overlay {version}"
                    )));
                }
            }
        }
        tx.execute(
            "INSERT INTO promotion_active (singleton, overlay_version, updated_at)
             VALUES (1, ?1, ?2)
             ON CONFLICT(singleton) DO UPDATE SET
                overlay_version=excluded.overlay_version,
                updated_at=excluded.updated_at",
            params![overlay_version, updated_at],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Get the current schema version.
    pub fn schema_version(&self) -> Result<i64, PersistenceError> {
        let version: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        Ok(version)
    }

    /// Run SQLite and typed-state checks used by the CLI doctor command.
    /// Empty output means healthy.
    pub fn health_check(&self) -> Result<Vec<String>, PersistenceError> {
        let mut violations = Vec::new();
        let quick_check: String = self
            .conn
            .query_row("PRAGMA quick_check", [], |row| row.get(0))?;
        if quick_check != "ok" {
            violations.push(format!("SQLite quick_check: {quick_check}"));
        }

        let mut foreign_keys = self.conn.prepare("PRAGMA foreign_key_check")?;
        let foreign_key_rows = foreign_keys.query_map([], |row| {
            Ok(format!(
                "table={}, rowid={}, parent={}, constraint={}",
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?
            ))
        })?;
        for row in foreign_key_rows {
            violations.push(format!("foreign key violation: {}", row?));
        }

        let version = self.schema_version()?;
        if version != db::migrations::CURRENT_SCHEMA_VERSION {
            violations.push(format!(
                "schema version is {version}, expected {}",
                db::migrations::CURRENT_SCHEMA_VERSION
            ));
        }
        for session_id in self.list_sessions()? {
            if let Err(error) = self.load_state(&session_id) {
                violations.push(format!("session '{session_id}' failed validation: {error}"));
            }
        }
        Ok(violations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qxfx0_self::collapse_essence;
    use qxfx0_self::fact_perspective::integrate_curated_claims;
    use qxfx0_semantic::ClaimRole;
    use qxfx0_types::governance::{GovernanceEvent, GovernanceEventType};
    use qxfx0_types::system_state::DialogueState;
    use qxfx0_types::system_state::*;
    use qxfx0_types::{BeliefPolarity, ConceptId, FactId, OpinionCore};
    use std::collections::BTreeSet;
    use std::sync::{Arc, Barrier};

    fn qualified_without_counterpoint_state(session_id: &str) -> SystemState {
        let packs = qxfx0_semantic::active_pack_set();
        let topic = ConceptId("concept.свобода".into());
        let thesis = FactId::try_new("fact.freedom_choice").unwrap();
        let (mut perspective, _) = integrate_curated_claims(
            &Default::default(),
            1,
            &[(ClaimRole::Thesis, thesis)],
            packs.facts(),
        )
        .unwrap();
        perspective.opinions.get_mut(&topic).unwrap().polarity = BeliefPolarity::Qualified;
        assert!(perspective.validate().is_empty());
        SystemState {
            session_id: session_id.into(),
            dialogue: DialogueState {
                turn_count: 1,
                ..Default::default()
            },
            semantic: SemanticState {
                pack_set_fingerprint: packs.fingerprint().into(),
                perspective,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn schema_v9_to_current_is_additive_idempotent_and_null_defaults() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "PRAGMA foreign_keys=ON;
             CREATE TABLE runtime_sessions (id TEXT PRIMARY KEY, state_json TEXT NOT NULL, last_active TEXT NOT NULL DEFAULT current_timestamp, turn_count INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE session_graphs (session_id TEXT PRIMARY KEY, atoms_json TEXT NOT NULL, edges_json TEXT NOT NULL, FOREIGN KEY(session_id) REFERENCES runtime_sessions(id) ON DELETE CASCADE);
             CREATE TABLE session_semantic (session_id TEXT PRIMARY KEY, field_json TEXT NOT NULL, essence_json TEXT NOT NULL, adjunction_json TEXT NOT NULL, commitments_json TEXT, stance_provenance_json TEXT, perspective_json TEXT, FOREIGN KEY(session_id) REFERENCES runtime_sessions(id) ON DELETE CASCADE);
             PRAGMA user_version=9;"
        ).unwrap();
        let state = SystemState {
            session_id: "v9".into(),
            ..Default::default()
        };
        let legacy = serde_json::to_string(&state).unwrap();
        conn.execute(
            "INSERT INTO runtime_sessions(id,state_json) VALUES (?1,?2)",
            params!["v9", legacy],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO session_graphs VALUES (?1,?2,?3)",
            params!["v9", "[]", "[]"],
        )
        .unwrap();
        let field = serde_json::to_string(&state.semantic.field).unwrap();
        let essence = serde_json::to_string(&state.semantic.essence).unwrap();
        let adj = serde_json::to_string(&state.semantic.adjunction).unwrap();
        conn.execute("INSERT INTO session_semantic(session_id,field_json,essence_json,adjunction_json) VALUES (?1,?2,?3,?4)", params!["v9", field, essence, adj]).unwrap();
        let before: String = conn
            .query_row(
                "SELECT state_json FROM runtime_sessions WHERE id=?1",
                ["v9"],
                |r| r.get(0),
            )
            .unwrap();
        db::migrations::apply_migrations(&mut conn).unwrap();
        db::migrations::apply_migrations(&mut conn).unwrap();
        assert_eq!(
            conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            db::migrations::CURRENT_SCHEMA_VERSION
        );
        let after: String = conn
            .query_row(
                "SELECT state_json FROM runtime_sessions WHERE id=?1",
                ["v9"],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(before.as_bytes(), after.as_bytes());
        let value: Option<String> = conn
            .query_row(
                "SELECT thesis_state_json FROM session_semantic WHERE session_id=?1",
                ["v9"],
                |r| r.get(0),
            )
            .unwrap();
        assert!(value.is_none());
        let essence_v2: Option<String> = conn
            .query_row(
                "SELECT essence_v2_json FROM session_semantic WHERE session_id=?1",
                ["v9"],
                |r| r.get(0),
            )
            .unwrap();
        assert!(essence_v2.is_none());
        let blanket_v2: Option<String> = conn
            .query_row(
                "SELECT blanket_v2_json FROM session_semantic WHERE session_id=?1",
                ["v9"],
                |r| r.get(0),
            )
            .unwrap();
        assert!(blanket_v2.is_none());
        assert_eq!(
            conn.query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
        assert_eq!(
            conn.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r
                .get::<_, i64>(
                0
            ))
            .unwrap(),
            0
        );
    }

    #[test]
    fn thesis_state_roundtrips_exactly() {
        let db = Persistence::open_memory().unwrap();
        let packs = qxfx0_semantic::active_pack_set();
        let (digest, metadata) = packs.overlay_theses().iter().next().unwrap();
        let fact = packs.facts().get(&metadata.authority_fact_id).unwrap();
        let mut thesis = fact.canonical_thesis().unwrap();
        thesis.id = qxfx0_types::ThesisId::try_new(metadata.thesis_id.clone()).unwrap();
        let lifecycle = qxfx0_types::ThesisLifecycle::draft(thesis).unwrap();
        let mut state = SystemState {
            session_id: "thesis-roundtrip".into(),
            ..Default::default()
        };
        state.semantic.thesis_state.pack_fingerprint = packs.fingerprint().into();
        state
            .semantic
            .thesis_state
            .projected_digests
            .insert(*digest);
        state
            .semantic
            .thesis_state
            .lifecycles
            .insert(lifecycle.thesis_id.clone(), lifecycle);
        db.save_state(&state.session_id, &state).unwrap();
        let loaded = db.load_state(&state.session_id).unwrap().unwrap();
        assert_eq!(loaded.semantic.thesis_state, state.semantic.thesis_state);
    }

    #[test]
    fn test_open_memory() {
        let db = Persistence::open_memory();
        assert!(db.is_ok());
    }

    #[test]
    fn file_connections_configure_five_second_busy_timeout() {
        let directory = backup_test_directory("busy-timeout");
        let path = directory.join("state.db");
        let db = Persistence::open(path.to_str().unwrap()).unwrap();
        let timeout_ms: i64 = db
            .conn
            .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
            .unwrap();
        assert_eq!(timeout_ms, 5_000);
        drop(db);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn concurrent_writer_lock_failure_is_diagnosable() {
        let directory = backup_test_directory("writer-contention");
        let path = directory.join("state.db");
        let lock_holder = Persistence::open(path.to_str().unwrap()).unwrap();
        lock_holder.conn.execute_batch("BEGIN IMMEDIATE").unwrap();

        let barrier = Arc::new(Barrier::new(2));
        let worker_barrier = Arc::clone(&barrier);
        let worker_path = path.clone();
        let worker = std::thread::spawn(move || {
            let writer = Persistence::open(worker_path.to_str().unwrap()).unwrap();
            // Keep this contention test fast and deterministic. The configured
            // production timeout is asserted independently above.
            writer.conn.busy_timeout(Duration::ZERO).unwrap();
            let state = SystemState {
                session_id: "contended".into(),
                ..SystemState::default()
            };
            worker_barrier.wait();
            writer.save_state("contended", &state)
        });

        barrier.wait();
        let error = worker.join().unwrap().unwrap_err();
        assert!(matches!(
            error,
            PersistenceError::SQLite(rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error {
                    code: rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked,
                    ..
                },
                _
            ))
        ));

        lock_holder.conn.execute_batch("ROLLBACK").unwrap();
        drop(lock_holder);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn operations_schema_marker_matches_current_schema_version() {
        const PREFIX: &str = "<!-- qxfx0-current-schema-version: ";
        let operations = include_str!("../../ops/README.md");
        let marker = operations
            .lines()
            .find_map(|line| line.strip_prefix(PREFIX)?.strip_suffix(" -->"))
            .expect("ops README must contain the structured schema-version marker");
        assert_eq!(
            marker.parse::<i64>().unwrap(),
            db::migrations::CURRENT_SCHEMA_VERSION
        );
    }

    fn backup_test_directory(label: &str) -> std::path::PathBuf {
        static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let sequence = TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "qxfx0-backup-{label}-{}-{sequence}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir(&directory).unwrap();
        directory
    }

    fn partial_backups(directory: &Path) -> Vec<std::path::PathBuf> {
        std::fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .contains(".partial-")
            })
            .collect()
    }

    #[test]
    fn online_backup_is_consistent_durable_and_leaves_no_partial() {
        let directory = backup_test_directory("success");
        let source = directory.join("source.db");
        let destination = directory.join("backup.db");

        let db = Persistence::open(source.to_str().unwrap()).unwrap();
        let state = SystemState {
            session_id: "backup-session".into(),
            dialogue: DialogueState {
                turn_count: 3,
                history: vec!["one".into(), "two".into(), "three".into()],
                ..DialogueState::default()
            },
            ..SystemState::default()
        };
        db.save_state("backup-session", &state).unwrap();

        Persistence::backup_database(source.to_str().unwrap(), destination.to_str().unwrap())
            .unwrap();
        assert!(destination.is_file());
        assert!(partial_backups(&directory).is_empty());

        let backup = Persistence::open(destination.to_str().unwrap()).unwrap();
        let restored = backup.load_state("backup-session").unwrap().unwrap();
        assert_eq!(restored.dialogue.turn_count, 3);
        assert_eq!(restored.dialogue.history.len(), 3);
        assert!(backup.health_check().unwrap().is_empty());

        drop(backup);
        drop(db);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn failed_backup_cleans_partial_and_preserves_existing_destination() {
        let directory = backup_test_directory("failure");
        let invalid_source = directory.join("invalid.db");
        let destination = directory.join("backup.db");
        std::fs::write(&invalid_source, b"not a SQLite database").unwrap();

        let error = Persistence::backup_database(
            invalid_source.to_str().unwrap(),
            destination.to_str().unwrap(),
        )
        .unwrap_err();
        assert!(matches!(error, PersistenceError::Backup(_)));
        assert!(!destination.exists());
        assert!(partial_backups(&directory).is_empty());

        let original_backup = b"previous valid backup";
        std::fs::write(&destination, original_backup).unwrap();
        let error = Persistence::backup_database(
            invalid_source.to_str().unwrap(),
            destination.to_str().unwrap(),
        )
        .unwrap_err();
        assert!(
            matches!(error, PersistenceError::Backup(message) if message.contains("already exists"))
        );
        assert_eq!(std::fs::read(&destination).unwrap(), original_backup);
        assert!(partial_backups(&directory).is_empty());

        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn test_save_and_load() {
        let db = Persistence::open_memory().unwrap();
        let state = SystemState {
            session_id: "test".into(),
            dialogue: DialogueState {
                turn_count: 3,
                history: vec!["привет".into(), "что такое свобода?".into()],
                ..Default::default()
            },
            ..Default::default()
        };

        db.save_state("test", &state).unwrap();
        let loaded = db.load_state("test").unwrap();
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.session_id, "test");
        assert_eq!(loaded.dialogue.turn_count, 3);
        assert_eq!(loaded.dialogue.history.len(), 2);
    }

    #[test]
    fn stance_provenance_round_trips_in_normalized_state() {
        let db = Persistence::open_memory().unwrap();
        let mut state = SystemState {
            session_id: "stance".into(),
            ..SystemState::default()
        };
        state
            .semantic
            .stance_provenance
            .record(qxfx0_types::stance::StanceObservation {
                turn: 1,
                topic: qxfx0_types::stance::StanceTopic::new("свобода").unwrap(),
                polarity: qxfx0_types::stance::StancePolarity::Affirmed,
                source: qxfx0_types::stance::StanceSource::SystemDecision,
            });
        db.save_state("stance", &state).unwrap();
        let replayed = db.load_state("stance").unwrap().unwrap();
        assert_eq!(
            replayed.semantic.stance_provenance,
            state.semantic.stance_provenance
        );
        assert_eq!(replayed.semantic.stance_provenance.version(), 1);
    }

    #[test]
    fn fact_grounded_perspective_round_trips_and_replay_is_idempotent() {
        let db = Persistence::open_memory().unwrap();
        let packs = qxfx0_semantic::active_pack_set();
        let thesis = FactId::try_new("fact.freedom_choice").unwrap();
        let claims = vec![(ClaimRole::Thesis, thesis)];
        let (perspective, first_update) =
            integrate_curated_claims(&Default::default(), 1, &claims, packs.facts()).unwrap();
        assert_eq!(first_update.episodes_added, 1);
        let (replayed, second_update) =
            integrate_curated_claims(&perspective, 2, &claims, packs.facts()).unwrap();
        assert_eq!(replayed, perspective);
        assert_eq!(second_update.episodes_added, 0);

        let mut state = SystemState {
            session_id: "fact-grounded-roundtrip".into(),
            ..Default::default()
        };
        state.semantic.pack_set_fingerprint = packs.fingerprint().into();
        state.semantic.perspective = perspective;
        db.save_state(&state.session_id, &state).unwrap();
        let loaded = db.load_state(&state.session_id).unwrap().unwrap();
        assert_eq!(loaded.semantic.pack_set_fingerprint, packs.fingerprint());
        assert_eq!(loaded.semantic.perspective, state.semantic.perspective);
    }

    #[test]
    fn non_empty_perspective_without_fingerprint_is_rejected_on_save() {
        let db = Persistence::open_memory().unwrap();
        let mut state = qualified_without_counterpoint_state("missing-pack-identity");
        state
            .semantic
            .perspective
            .opinions
            .get_mut(&ConceptId("concept.свобода".into()))
            .unwrap()
            .polarity = BeliefPolarity::Affirmed;
        state.semantic.pack_set_fingerprint.clear();
        let error = db
            .save_state(&state.session_id, &state)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("non-empty Perspective has no knowledge-pack fingerprint"),
            "{error}"
        );
    }

    #[test]
    fn well_formed_but_semantically_corrupt_normalized_and_legacy_json_fail_closed() {
        let db = Persistence::open_memory().unwrap();

        let normalized_id = "corrupt-normalized-perspective";
        let clean = SystemState {
            session_id: normalized_id.into(),
            ..Default::default()
        };
        db.save_state(normalized_id, &clean).unwrap();
        let corrupt_normalized = qualified_without_counterpoint_state(normalized_id);
        db.conn
            .execute(
                "UPDATE runtime_sessions SET state_json = ?1 WHERE id = ?2",
                params![
                    serde_json::to_string(&corrupt_normalized).unwrap(),
                    normalized_id
                ],
            )
            .unwrap();
        db.conn
            .execute(
                "UPDATE session_semantic SET perspective_json = ?1 WHERE session_id = ?2",
                params![
                    serde_json::to_string(&corrupt_normalized.semantic.perspective).unwrap(),
                    normalized_id
                ],
            )
            .unwrap();
        let error = db.load_state(normalized_id).unwrap_err().to_string();
        assert!(
            error.contains("qualified without a curated counterpoint"),
            "{error}"
        );

        let legacy_id = "corrupt-legacy-perspective";
        let corrupt_legacy = qualified_without_counterpoint_state(legacy_id);
        db.conn
            .execute(
                "INSERT INTO runtime_sessions (id, state_json, turn_count) VALUES (?1, ?2, ?3)",
                params![
                    legacy_id,
                    serde_json::to_string(&corrupt_legacy).unwrap(),
                    corrupt_legacy.dialogue.turn_count
                ],
            )
            .unwrap();
        let error = db.load_state(legacy_id).unwrap_err().to_string();
        assert!(
            error.contains("qualified without a curated counterpoint"),
            "{error}"
        );
    }

    #[test]
    fn forged_fact_and_corrupt_perspective_json_fail_closed() {
        let db = Persistence::open_memory().unwrap();
        let topic = ConceptId("concept.свобода".into());
        let forged = FactId::try_new("fact.user-forged").unwrap();
        let mut state = SystemState {
            session_id: "forged-perspective".into(),
            ..Default::default()
        };
        state.semantic.pack_set_fingerprint =
            qxfx0_semantic::active_pack_set().fingerprint().into();
        state.semantic.perspective.opinions.insert(
            topic.clone(),
            OpinionCore {
                topic,
                primary_fact: forged.clone(),
                polarity: BeliefPolarity::Affirmed,
                grounding_facts: BTreeSet::from([forged]),
                confidence_basis_points: 1_000,
                revision_seq: 1,
            },
        );
        let error = db
            .save_state(&state.session_id, &state)
            .unwrap_err()
            .to_string();
        assert!(error.contains("invalid authority"), "{error}");

        let clean = SystemState {
            session_id: "corrupt-perspective".into(),
            ..Default::default()
        };
        db.save_state(&clean.session_id, &clean).unwrap();
        db.conn
            .execute(
                "UPDATE session_semantic SET perspective_json = ?1 WHERE session_id = ?2",
                params!["{not-json}", clean.session_id],
            )
            .unwrap();
        let error = db.load_state(&clean.session_id).unwrap_err().to_string();
        assert!(error.contains("Serialization error"), "{error}");
    }

    #[test]
    fn legacy_null_stance_provenance_loads_as_empty_v1() {
        let db = Persistence::open_memory().unwrap();
        let state = SystemState {
            session_id: "legacy-null-stance".into(),
            ..SystemState::default()
        };
        db.save_state("legacy-null-stance", &state).unwrap();
        db.conn
            .execute(
                "UPDATE session_semantic SET stance_provenance_json = NULL WHERE session_id = ?1",
                params!["legacy-null-stance"],
            )
            .unwrap();

        let loaded = db.load_state("legacy-null-stance").unwrap().unwrap();
        assert!(loaded.semantic.stance_provenance.is_empty());
        assert_eq!(loaded.semantic.stance_provenance.version(), 1);
    }

    #[test]
    fn test_legacy_essence_floor_replays_without_implicit_migration() {
        let db = Persistence::open_memory().unwrap();
        let state = SystemState {
            session_id: "legacy-essence".into(),
            dialogue: DialogueState {
                turn_count: 2,
                ..Default::default()
            },
            semantic: SemanticState {
                essence: EssenceState {
                    witnesses: vec![
                        EssenceWitness {
                            turn: 1,
                            mode: "Define".into(),
                            statement: "свобода".into(),
                            salience_driver: "fixture".into(),
                            reconcile_rule: "RuleFormalAdvantage".into(),
                            agreement: "DivergeMultiple".into(),
                            divergence: 0.5,
                            conatus_scalar: 12.0,
                        },
                        EssenceWitness {
                            turn: 2,
                            mode: "Define".into(),
                            statement: "ответственность".into(),
                            salience_driver: "fixture".into(),
                            reconcile_rule: "RuleAgreement".into(),
                            agreement: "Agree".into(),
                            divergence: 0.0,
                            conatus_scalar: 11.0,
                        },
                    ],
                    angst: 0.95,
                    trajectory_committed: true,
                    conatus_floor: 11.0,
                    capacity: 32,
                    commitment: Some(EssenceCommitment {
                        mode: CommitmentMode::Contemplative,
                        trigger: CommitmentTrigger::TriggerAngstThreshold,
                        committed_at: 2,
                        witness_hash: "sha256:legacy-fixture".into(),
                    }),
                    reset_events: Vec::new(),
                },
                ..Default::default()
            },
            ..Default::default()
        };

        let mut legacy_json = serde_json::to_value(&state).unwrap();
        let legacy_essence = legacy_json["semantic"]["essence"]
            .as_object_mut()
            .expect("serialized state must contain essence object");
        for field in ["conatus_floor", "capacity", "commitment", "reset_events"] {
            legacy_essence.remove(field);
        }
        let legacy_json = serde_json::to_string(&legacy_json).unwrap();
        db.conn
            .execute(
                "INSERT INTO runtime_sessions (id, state_json, turn_count) VALUES (?1, ?2, ?3)",
                params!["legacy-essence", legacy_json, 2],
            )
            .unwrap();

        let stored_before_load: String = db
            .conn
            .query_row(
                "SELECT state_json FROM runtime_sessions WHERE id = 'legacy-essence'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let mut loaded = db.load_state("legacy-essence").unwrap().unwrap();
        let stored_after_load: String = db
            .conn
            .query_row(
                "SELECT state_json FROM runtime_sessions WHERE id = 'legacy-essence'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(stored_after_load, stored_before_load);
        assert_eq!(loaded.semantic.essence.conatus_floor, f64::MAX);
        assert_eq!(loaded.semantic.essence.capacity, 0);
        assert!(loaded.semantic.essence.commitment.is_none());
        assert!(loaded.semantic.essence.reset_events.is_empty());

        let event = collapse_essence(3, &mut loaded.semantic.essence);
        assert_eq!(event.turn, 3);
        assert_eq!(event.previous_angst, 0.95);
        assert_eq!(event.previous_witness_count, 2);
        assert_eq!(loaded.semantic.essence.conatus_floor, f64::MAX);
        assert!(!loaded.semantic.essence.trajectory_committed);
        assert!(loaded.semantic.essence.commitment.is_none());
        assert!(loaded.semantic.essence.witnesses.is_empty());

        db.save_state("legacy-essence", &loaded).unwrap();
        let replayed = db.load_state("legacy-essence").unwrap().unwrap();
        assert_eq!(replayed.semantic.essence.conatus_floor, f64::MAX);
        assert!(replayed.semantic.essence.witnesses.is_empty());
        assert!(replayed.semantic.essence.commitment.is_none());
        assert_eq!(replayed.semantic.essence.reset_events.len(), 1);
        assert_eq!(replayed.semantic.essence.reset_events[0].turn, 3);
        assert_eq!(
            replayed.semantic.essence.reset_events[0].previous_witness_count,
            2
        );
    }

    #[test]
    fn test_load_nonexistent() {
        let db = Persistence::open_memory().unwrap();
        let result = db.load_state("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_list_sessions() {
        let db = Persistence::open_memory().unwrap();
        let state1 = SystemState {
            session_id: "s1".into(),
            ..SystemState::default()
        };
        let state2 = SystemState {
            session_id: "s2".into(),
            ..SystemState::default()
        };
        db.save_state("s1", &state1).unwrap();
        db.save_state("s2", &state2).unwrap();
        let sessions = db.list_sessions().unwrap();
        assert_eq!(sessions.len(), 2);
    }

    #[test]
    fn test_delete_session() {
        let db = Persistence::open_memory().unwrap();
        let state = SystemState {
            session_id: "s1".into(),
            ..SystemState::default()
        };
        db.save_state("s1", &state).unwrap();
        db.delete_session("s1").unwrap();
        assert!(db.load_state("s1").unwrap().is_none());
        let graph_rows: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM session_graphs WHERE session_id = 's1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let semantic_rows: i64 = db
            .conn
            .query_row(
                "SELECT COUNT(*) FROM session_semantic WHERE session_id = 's1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!((graph_rows, semantic_rows), (0, 0));
    }

    #[test]
    fn test_migrates_legacy_main_schema_without_touching_schema_version() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE schema_version (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL DEFAULT (datetime('now')),
                description TEXT NOT NULL
            );
            INSERT INTO schema_version (version, description)
                VALUES (1, 'initial schema'), (2, 'rename state_revision to turn_count');
            CREATE TABLE runtime_sessions (
                id TEXT PRIMARY KEY,
                started_at TEXT NOT NULL DEFAULT (datetime('now')),
                last_active TEXT NOT NULL DEFAULT (datetime('now')),
                state_json TEXT NOT NULL DEFAULT '{}',
                turn_count INTEGER NOT NULL DEFAULT 0
            );
            "#,
        )
        .unwrap();

        let legacy = SystemState {
            session_id: "legacy".into(),
            dialogue: DialogueState {
                turn_count: 2,
                ..Default::default()
            },
            ..Default::default()
        };
        let json = serde_json::to_string(&legacy).unwrap();
        conn.execute(
            "INSERT INTO runtime_sessions (id, state_json, turn_count) VALUES (?1, ?2, ?3)",
            params!["legacy", json, 2],
        )
        .unwrap();

        Persistence::configure_connection(&conn).unwrap();
        db::migrations::apply_migrations(&mut conn).unwrap();
        let db = Persistence { conn };

        assert_eq!(db.schema_version().unwrap(), 14);
        let loaded = db.load_state("legacy").unwrap().unwrap();
        assert_eq!(loaded.session_id, "legacy");
        assert_eq!(loaded.dialogue.turn_count, 2);
        let legacy_versions: i64 = db
            .conn
            .query_row("SELECT COUNT(*) FROM schema_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(legacy_versions, 2);
    }

    #[test]
    fn newer_schema_version_fails_closed_instead_of_opening() {
        let path =
            std::env::temp_dir().join(format!("qxfx0-newer-schema-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        // Create a current-version database, then simulate a newer build
        // having written it by bumping user_version past what this binary
        // understands.
        {
            let persistence = Persistence::open(path.to_str().unwrap()).unwrap();
            persistence
                .save_state(
                    "newer",
                    &SystemState {
                        session_id: "newer".into(),
                        ..SystemState::default()
                    },
                )
                .unwrap();
        }
        {
            let conn = Connection::open(&path).unwrap();
            conn.pragma_update(
                None,
                "user_version",
                db::migrations::CURRENT_SCHEMA_VERSION + 1,
            )
            .unwrap();
        }
        let error = match Persistence::open(path.to_str().unwrap()) {
            Ok(_) => panic!("a newer schema must fail closed at open"),
            Err(error) => error,
        };
        assert!(
            matches!(
                &error,
                PersistenceError::Migration(db::migrations::MigrationError::NewerSchema(version))
                    if *version == db::migrations::CURRENT_SCHEMA_VERSION + 1
            ),
            "a newer schema must fail closed at open, got: {error}"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_migrates_file_backed_legacy_copy() {
        let path = std::env::temp_dir().join(format!(
            "qxfx0-legacy-migration-copy-{}.db",
            std::process::id()
        ));
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                r#"
                CREATE TABLE schema_version (
                    version INTEGER PRIMARY KEY,
                    applied_at TEXT NOT NULL DEFAULT (datetime('now')),
                    description TEXT NOT NULL
                );
                INSERT INTO schema_version (version, description)
                    VALUES (1, 'initial schema'), (2, 'legacy production schema');
                CREATE TABLE runtime_sessions (
                    id TEXT PRIMARY KEY,
                    started_at TEXT NOT NULL DEFAULT (datetime('now')),
                    last_active TEXT NOT NULL DEFAULT (datetime('now')),
                    state_json TEXT NOT NULL DEFAULT '{}',
                    turn_count INTEGER NOT NULL DEFAULT 0
                );
                "#,
            )
            .unwrap();
            let state = SystemState {
                session_id: "file-legacy".into(),
                dialogue: DialogueState {
                    turn_count: 4,
                    ..DialogueState::default()
                },
                ..SystemState::default()
            };
            conn.execute(
                "INSERT INTO runtime_sessions (id, state_json, turn_count) VALUES (?1, ?2, 4)",
                params!["file-legacy", serde_json::to_string(&state).unwrap()],
            )
            .unwrap();
        }

        {
            let db = Persistence::open(path.to_str().unwrap()).unwrap();
            assert_eq!(db.schema_version().unwrap(), 14);
            let loaded = db.load_state("file-legacy").unwrap().unwrap();
            assert_eq!(loaded.dialogue.turn_count, 4);
            let legacy_versions: i64 = db
                .conn
                .query_row("SELECT COUNT(*) FROM schema_version", [], |row| row.get(0))
                .unwrap();
            assert_eq!(legacy_versions, 2);
        }

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("db-wal"));
        let _ = std::fs::remove_file(path.with_extension("db-shm"));
    }

    #[test]
    fn test_round_trip_with_graph() {
        let db = Persistence::open_memory().unwrap();
        let state = SystemState {
            session_id: "graph-test".into(),
            dialogue: DialogueState {
                turn_count: 1,
                ..Default::default()
            },
            semantic: SemanticState {
                runtime_graph: qxfx0_semantic::seed_graph(),
                ..Default::default()
            },
            ..Default::default()
        };

        db.save_state("graph-test", &state).unwrap();
        let loaded = db.load_state("graph-test").unwrap().unwrap();
        assert_eq!(
            loaded.semantic.runtime_graph.atoms.len(),
            state.semantic.runtime_graph.atoms.len()
        );
        assert_eq!(
            loaded.semantic.runtime_graph.edges.len(),
            state.semantic.runtime_graph.edges.len()
        );

        // Regression check: indexes must be rebuilt so relations_from/relations_to work.
        let topic = qxfx0_types::atom::AtomId::new("свобода");
        let from_before = state.semantic.runtime_graph.relations_from(&topic).len();
        let from_after = loaded.semantic.runtime_graph.relations_from(&topic).len();
        assert_eq!(
            from_after, from_before,
            "relations_from indexes must survive round-trip"
        );

        let to_before = state.semantic.runtime_graph.relations_to(&topic).len();
        let to_after = loaded.semantic.runtime_graph.relations_to(&topic).len();
        assert_eq!(
            to_after, to_before,
            "relations_to indexes must survive round-trip"
        );
    }

    #[test]
    fn test_round_trip_with_governance_log() {
        let db = Persistence::open_memory().unwrap();
        let mut state = SystemState {
            session_id: "gov-test".into(),
            dialogue: DialogueState {
                turn_count: 2,
                ..Default::default()
            },
            ..Default::default()
        };

        state.governance_log.append(GovernanceEvent {
            turn: 1,
            event_type: GovernanceEventType::TurnCompleted,
            family: qxfx0_types::CanonicalMoveFamily::CMDefine,
            guard_status: GuardStatus::InvariantOk,
            timestamp: "2026-01-01T00:00:01Z".into(),
        });
        state.governance_log.append(GovernanceEvent {
            turn: 2,
            event_type: GovernanceEventType::GuardBlocked,
            family: qxfx0_types::CanonicalMoveFamily::CMRepair,
            guard_status: GuardStatus::InvariantBlock("safety".into()),
            timestamp: "2026-01-01T00:00:02Z".into(),
        });

        db.save_state("gov-test", &state).unwrap();
        let loaded = db.load_state("gov-test").unwrap().unwrap();

        assert_eq!(loaded.governance_log.len(), 2);
        assert!(loaded.governance_log.has_blocks());
        assert!(loaded.governance_log.replay_check().is_empty());
        assert_eq!(
            loaded
                .governance_log
                .count_by_type(&GovernanceEventType::TurnCompleted),
            1
        );
        assert_eq!(
            loaded
                .governance_log
                .count_by_type(&GovernanceEventType::GuardBlocked),
            1
        );
    }

    // ---- learning bridge stores (ADR-0043 U4) ----

    fn bridge_session(db: &Persistence, id: &str) {
        let state = SystemState {
            session_id: id.into(),
            ..SystemState::default()
        };
        db.save_state(id, &state).unwrap();
    }

    #[test]
    fn bridge_edges_round_trip_and_absence_is_none() {
        let db = Persistence::open_memory().unwrap();
        bridge_session(&db, "bridge-a");
        assert_eq!(db.load_bridge_edges("bridge-a").unwrap(), None);
        db.save_bridge_edges("bridge-a", Some(r#"[["свобода","выбор",0.8]]"#))
            .unwrap();
        assert_eq!(
            db.load_bridge_edges("bridge-a").unwrap().as_deref(),
            Some(r#"[["свобода","выбор",0.8]]"#)
        );
        // Writing None clears the row: drained-to-nothing == never touched.
        db.save_bridge_edges("bridge-a", None).unwrap();
        assert_eq!(db.load_bridge_edges("bridge-a").unwrap(), None);
    }

    #[test]
    fn bridge_edges_reject_oversized_blobs() {
        let db = Persistence::open_memory().unwrap();
        bridge_session(&db, "bridge-b");
        let oversized = "x".repeat(MAX_BRIDGE_EDGES_JSON_BYTES + 1);
        assert!(matches!(
            db.save_bridge_edges("bridge-b", Some(&oversized)),
            Err(PersistenceError::InvalidState(_))
        ));
        assert_eq!(db.load_bridge_edges("bridge-b").unwrap(), None);
    }

    #[test]
    fn bridge_quarantine_is_append_ordered_monotonic_and_clearable() {
        let db = Persistence::open_memory().unwrap();
        bridge_session(&db, "bridge-q");
        assert!(db.load_bridge_quarantine("bridge-q").unwrap().is_empty());
        let first = db
            .enqueue_bridge_quarantine("bridge-q", r#"{"n":1}"#)
            .unwrap();
        let second = db
            .enqueue_bridge_quarantine("bridge-q", r#"{"n":2}"#)
            .unwrap();
        assert_eq!((first, second), (1, 2));
        let entries = db.load_bridge_quarantine("bridge-q").unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].0, 1);
        assert_eq!(entries[1].0, 2);
        db.clear_bridge_quarantine("bridge-q").unwrap();
        assert!(db.load_bridge_quarantine("bridge-q").unwrap().is_empty());
        // After a clear, the next seq restarts at 1 — the ledger is empty.
        assert_eq!(db.enqueue_bridge_quarantine("bridge-q", "{}").unwrap(), 1);
    }

    #[test]
    fn bridge_quarantine_rejects_oversized_entries() {
        let db = Persistence::open_memory().unwrap();
        bridge_session(&db, "bridge-e");
        let oversized = "x".repeat(MAX_BRIDGE_QUARANTINE_ENTRY_BYTES + 1);
        assert!(matches!(
            db.enqueue_bridge_quarantine("bridge-e", &oversized),
            Err(PersistenceError::InvalidState(_))
        ));
    }

    #[test]
    fn delete_session_removes_bridge_rows() {
        let db = Persistence::open_memory().unwrap();
        bridge_session(&db, "bridge-d");
        db.save_bridge_edges("bridge-d", Some("[]")).unwrap();
        db.enqueue_bridge_quarantine("bridge-d", "{}").unwrap();
        assert!(db.load_bridge_edges("bridge-d").unwrap().is_some());
        db.delete_session("bridge-d").unwrap();
        assert_eq!(db.load_bridge_edges("bridge-d").unwrap(), None);
        assert!(db.load_bridge_quarantine("bridge-d").unwrap().is_empty());
    }

    #[test]
    fn bridge_quarantine_enforces_the_row_cap() {
        let db = Persistence::open_memory().unwrap();
        bridge_session(&db, "bridge-cap");
        for _ in 0..MAX_BRIDGE_QUARANTINE_ROWS {
            db.enqueue_bridge_quarantine("bridge-cap", "{}").unwrap();
        }
        assert!(matches!(
            db.enqueue_bridge_quarantine("bridge-cap", "{}"),
            Err(PersistenceError::InvalidState(_))
        ));
        assert_eq!(
            db.load_bridge_quarantine("bridge-cap").unwrap().len() as i64,
            MAX_BRIDGE_QUARANTINE_ROWS
        );
    }

    #[test]
    fn v12_to_current_creates_bridge_and_promotion_tables_without_touching_state() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE runtime_sessions (id TEXT PRIMARY KEY, state_json TEXT NOT NULL,
               last_active TEXT NOT NULL, turn_count INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE session_graphs (session_id TEXT PRIMARY KEY, atoms_json TEXT NOT NULL,
               edges_json TEXT NOT NULL);
             CREATE TABLE session_semantic (session_id TEXT PRIMARY KEY, field_json TEXT NOT NULL,
               essence_json TEXT NOT NULL, adjunction_json TEXT NOT NULL,
               commitments_json TEXT, stance_provenance_json TEXT, perspective_json TEXT,
               thesis_state_json TEXT, essence_v2_json TEXT, blanket_v2_json TEXT);
             PRAGMA user_version = 12;",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO runtime_sessions (id, state_json, last_active, turn_count) VALUES ('v12', '{\"session_id\":\"v12\",\"dialogue\":{\"turn_count\":0,\"history\":[],\"journal\":[],\"last_topic\":null,\"last_family\":null,\"conversation_state\":null},\"semantic\":{\"field\":{\"resonance\":0.5,\"atmosphere\":{\"valence\":0.0,\"arousal\":0.4},\"confidence\":0.5,\"consolidation\":0.5,\"counterfactual\":0.5},\"runtime_graph\":{\"atoms\":{},\"edges\":[]},\"pack_set_fingerprint\":\"\",\"semantic_commitments\":null,\"essence\":{\"witnesses\":[],\"angst\":0.0,\"commitment\":null,\"reset_events\":[],\"trajectory_committed\":false,\"consecutive_low_conatus\":[]},\"adjunction\":{\"holistic_value\":0.5,\"formal_value\":0.5,\"reconciled_value\":0.5,\"holistic_dominant\":false},\"perspective\":{\"opinions\":[],\"episodes\":[]},\"stance_provenance\":{\"events\":[]}},\"last_turn_decision\":null,\"governance_log\":{\"events\":[]}}', 'now', 0)",
            [],
        )
        .unwrap();
        db::migrations::apply_migrations(&mut conn).unwrap();
        assert_eq!(
            conn.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            db::migrations::CURRENT_SCHEMA_VERSION
        );
        // New tables exist and are empty.
        let bridge_edges: i64 = conn
            .query_row("SELECT count(*) FROM session_bridge_edges", [], |r| {
                r.get(0)
            })
            .unwrap();
        let quarantine: i64 = conn
            .query_row("SELECT count(*) FROM session_bridge_quarantine", [], |r| {
                r.get(0)
            })
            .unwrap();
        let overlays: i64 = conn
            .query_row("SELECT count(*) FROM promotion_overlays", [], |r| r.get(0))
            .unwrap();
        let active: i64 = conn
            .query_row("SELECT count(*) FROM promotion_active", [], |r| r.get(0))
            .unwrap();
        assert_eq!((bridge_edges, quarantine, overlays, active), (0, 0, 0, 0));
        // Existing session state untouched.
        let after: String = conn
            .query_row(
                "SELECT state_json FROM runtime_sessions WHERE id='v12'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(after.starts_with("{\"session_id\":\"v12\""));
        assert_eq!(
            conn.query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
    }

    #[test]
    fn promotion_overlay_store_is_idempotent_and_cas_gated() {
        let db = Persistence::open_memory().unwrap();
        let version = "overlay-deadbeef";
        let draft_json = r#"{"status":"Draft","predicates":[]}"#;
        let activated_json = r#"{"status":"Activated","predicates":[]}"#;
        let released_json = r#"{"status":"Released","predicates":[]}"#;

        // First insert creates the row; an identical re-insert is a no-op.
        assert!(db
            .save_promotion_overlay(version, "Draft", "snap", None, "cs-1", draft_json, 100)
            .unwrap());
        assert!(!db
            .save_promotion_overlay(version, "Draft", "snap", None, "cs-1", draft_json, 100)
            .unwrap());
        // A different content under the same version is a caller
        // inconsistency (content address is violated), not an overwrite.
        assert!(db
            .save_promotion_overlay(version, "Draft", "snap", None, "cs-2", draft_json, 100)
            .is_err());
        assert_eq!(
            db.load_promotion_overlay(version).unwrap(),
            Some(("Draft".to_string(), draft_json.to_string()))
        );

        // CAS transition draft -> activated, pinned on the current blob.
        db.replace_promotion_overlay_if_matches(
            version,
            draft_json,
            "Activated",
            activated_json,
            "cs-1",
        )
        .unwrap();
        // A stale expected blob loses the CAS and errors.
        assert!(db
            .replace_promotion_overlay_if_matches(
                version,
                draft_json,
                "Released",
                released_json,
                "cs-1"
            )
            .is_err());
        // A CAS that would change the content address fails closed.
        assert!(db
            .replace_promotion_overlay_if_matches(
                version,
                activated_json,
                "Released",
                released_json,
                "cs-999"
            )
            .is_err());
        db.replace_promotion_overlay_if_matches(
            version,
            activated_json,
            "Released",
            released_json,
            "cs-1",
        )
        .unwrap();
        assert_eq!(
            db.load_promotion_overlay(version).unwrap().map(|row| row.0),
            Some("Released".into())
        );

        // The active pointer refuses a Draft/Activated target and accepts a
        // Released one; None clears it.
        assert!(db
            .set_active_promotion_overlay(Some("overlay-missing"), 200)
            .is_err());
        db.save_promotion_overlay(
            "overlay-cafe",
            "Draft",
            "snap",
            None,
            "cs-2",
            draft_json,
            150,
        )
        .unwrap();
        assert!(
            db.set_active_promotion_overlay(Some("overlay-cafe"), 200)
                .is_err(),
            "a Draft is not authority"
        );
        db.set_active_promotion_overlay(Some(version), 200).unwrap();
        assert_eq!(
            db.load_active_promotion_overlay().unwrap().as_deref(),
            Some(version)
        );
        db.set_active_promotion_overlay(None, 250).unwrap();
        assert_eq!(db.load_active_promotion_overlay().unwrap(), None);

        // List returns both overlays newest-creation first.
        let listed = db.list_promotion_overlays().unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(
            listed[0].0, "overlay-cafe",
            "created_at 150 > 100 sorts first"
        );
    }
}
