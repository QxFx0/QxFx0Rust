use qxfx0_types::system_state::SystemState;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;
use thiserror::Error;

mod db;

#[derive(Error, Debug)]
pub enum PersistenceError {
    #[error("SQLite error: {0}")]
    SQLite(#[from] rusqlite::Error),
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
    /// SQLite writes into a process-specific partial file first. The partial
    /// copy is verified with `PRAGMA quick_check` and atomically renamed only
    /// after the backup has completed successfully.
    pub fn backup_database(source: &str, destination: &str) -> Result<(), PersistenceError> {
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
            .map_err(|error| PersistenceError::Backup(error.to_string()))?;
        let destination_parent = destination_path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let destination_parent = std::fs::canonicalize(destination_parent)
            .map_err(|error| PersistenceError::Backup(error.to_string()))?;
        let destination_name = destination_path.file_name().ok_or_else(|| {
            PersistenceError::Backup("destination must include a file name".into())
        })?;
        let destination_canonical = destination_parent.join(destination_name);

        if source_canonical == destination_canonical {
            return Err(PersistenceError::Backup(
                "source and destination resolve to the same file".into(),
            ));
        }

        let partial_name = format!(
            ".{}.partial-{}",
            destination_name.to_string_lossy(),
            std::process::id()
        );
        let partial_path = destination_parent.join(partial_name);
        if partial_path.exists() {
            return Err(PersistenceError::Backup(format!(
                "partial destination '{}' already exists",
                partial_path.display()
            )));
        }

        let result = (|| {
            let source_connection = Connection::open_with_flags(
                &source_canonical,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )?;
            source_connection.busy_timeout(Duration::from_secs(5))?;

            let mut destination_connection = Connection::open_with_flags(
                &partial_path,
                OpenFlags::SQLITE_OPEN_READ_WRITE
                    | OpenFlags::SQLITE_OPEN_CREATE
                    | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )?;
            {
                let backup =
                    rusqlite::backup::Backup::new(&source_connection, &mut destination_connection)?;
                backup.run_to_completion(128, Duration::from_millis(10), None)?;
            }

            let quick_check: String =
                destination_connection.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
            if quick_check != "ok" {
                return Err(PersistenceError::Backup(format!(
                    "destination quick_check failed: {quick_check}"
                )));
            }
            drop(destination_connection);

            std::fs::rename(&partial_path, &destination_canonical)
                .map_err(|error| PersistenceError::Backup(error.to_string()))?;
            Ok(())
        })();

        if result.is_err() {
            let _ = std::fs::remove_file(&partial_path);
        }
        result
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
        if state.session_id != session_id {
            return Err(PersistenceError::InvalidState(format!(
                "storage session '{}' differs from state session '{}'",
                session_id, state.session_id
            )));
        }
        let violations = state.validate();
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

        let state_json = serde_json::to_string(state)
            .map_err(|e| PersistenceError::Serialization(e.to_string()))?;

        let tx = self.conn.unchecked_transaction()?;

        // Legacy monolithic row: kept for backward compatibility until v7 migration.
        tx.execute(
            "INSERT INTO runtime_sessions (id, state_json, last_active, turn_count)
             VALUES (?1, ?2, datetime('now'), ?3)
             ON CONFLICT(id) DO UPDATE SET
                state_json=excluded.state_json,
                last_active=datetime('now'),
                turn_count=excluded.turn_count",
            params![session_id, state_json, state.dialogue.turn_count],
        )?;

        tx.execute(
            "INSERT INTO session_graphs (session_id, atoms_json, edges_json)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(session_id) DO UPDATE SET
                atoms_json=excluded.atoms_json,
                edges_json=excluded.edges_json",
            params![session_id, atoms_json, edges_json],
        )?;

        tx.execute(
            "INSERT INTO session_semantic (session_id, field_json, essence_json, adjunction_json, commitments_json)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(session_id) DO UPDATE SET
                field_json=excluded.field_json,
                essence_json=excluded.essence_json,
                adjunction_json=excluded.adjunction_json,
                commitments_json=excluded.commitments_json",
            params![session_id, field_json, essence_json, adjunction_json, commitments_json],
        )?;

        tx.commit()?;
        Ok(())
    }

    /// Load system state for a session.
    ///
    /// First attempts the normalized split tables (session_graphs + session_semantic).
    /// If those are absent, falls back to the legacy `state_json` blob for backward
    /// compatibility with databases created before the v6 migration.
    pub fn load_state(&self, session_id: &str) -> Result<Option<SystemState>, PersistenceError> {
        // Try normalized tables first.
        let graph = self
            .conn
            .query_row(
                "SELECT atoms_json, edges_json FROM session_graphs WHERE session_id = ?1",
                params![session_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;

        let semantic = self
            .conn
            .query_row(
                "SELECT field_json, essence_json, adjunction_json, commitments_json
                 FROM session_semantic WHERE session_id = ?1",
                params![session_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                    ))
                },
            )
            .optional()?;

        if let (
            Some((atoms_json, edges_json)),
            Some((field_json, essence_json, adjunction_json, commitments_json)),
        ) = (graph, semantic)
        {
            let session = self
                .conn
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
                        cached_edge_count: 0,
                        cached_network: None,
                    }
                },
                last_turn_decision,
                governance_log,
            };
            let violations = state.validate();
            if !violations.is_empty() {
                return Err(PersistenceError::InvalidState(violations.join("; ")));
            }
            return Ok(Some(state));
        }

        // Legacy fallback.
        let mut stmt = self
            .conn
            .prepare_cached("SELECT state_json FROM runtime_sessions WHERE id = ?1")?;

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
                let violations = state.validate();
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
            "DELETE FROM runtime_sessions WHERE id = ?1",
            params![session_id],
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
    use qxfx0_types::governance::{GovernanceEvent, GovernanceEventType};
    use qxfx0_types::system_state::DialogueState;
    use qxfx0_types::system_state::*;

    #[test]
    fn test_open_memory() {
        let db = Persistence::open_memory();
        assert!(db.is_ok());
    }

    #[test]
    fn test_online_backup_is_consistent_and_refuses_overwrite() {
        let source = std::env::temp_dir().join(format!(
            "qxfx0-online-backup-source-{}.db",
            std::process::id()
        ));
        let destination = std::env::temp_dir().join(format!(
            "qxfx0-online-backup-destination-{}.db",
            std::process::id()
        ));
        for path in [&source, &destination] {
            let _ = std::fs::remove_file(path);
            let _ = std::fs::remove_file(format!("{}-wal", path.display()));
            let _ = std::fs::remove_file(format!("{}-shm", path.display()));
        }

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
        let overwrite =
            Persistence::backup_database(source.to_str().unwrap(), destination.to_str().unwrap());
        assert!(
            matches!(overwrite, Err(PersistenceError::Backup(message)) if message.contains("already exists"))
        );

        let backup = Persistence::open(destination.to_str().unwrap()).unwrap();
        let restored = backup.load_state("backup-session").unwrap().unwrap();
        assert_eq!(restored.dialogue.turn_count, 3);
        assert_eq!(restored.dialogue.history.len(), 3);
        assert!(backup.health_check().unwrap().is_empty());

        drop(backup);
        drop(db);
        for path in [&source, &destination] {
            let _ = std::fs::remove_file(path);
            let _ = std::fs::remove_file(format!("{}-wal", path.display()));
            let _ = std::fs::remove_file(format!("{}-shm", path.display()));
        }
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

        assert_eq!(db.schema_version().unwrap(), 7);
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
            assert_eq!(db.schema_version().unwrap(), 7);
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
}
