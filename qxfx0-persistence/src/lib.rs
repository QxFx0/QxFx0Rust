use qxfx0_types::system_state::SystemState;
use rusqlite::{params, Connection};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PersistenceError {
    #[error("SQLite error: {0}")]
    SQLite(#[from] rusqlite::Error),
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("State not found: {0}")]
    NotFound(String),
}

/// Schema SQL for the runtime database.
const SCHEMA_SQL: &str = "
PRAGMA journal_mode=WAL;
PRAGMA busy_timeout=5000;
PRAGMA foreign_keys=ON;

CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER PRIMARY KEY,
    applied_at TEXT NOT NULL DEFAULT (datetime('now')),
    description TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS runtime_sessions (
    id TEXT PRIMARY KEY,
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_active TEXT NOT NULL DEFAULT (datetime('now')),
    state_json TEXT NOT NULL DEFAULT '{}',
    turn_count INTEGER NOT NULL DEFAULT 0
);
";

const MIGRATION_001: &str = "
INSERT OR IGNORE INTO schema_version (version, description) VALUES (1, 'initial schema');
";

/// Migration 002: rename `state_revision` column to `turn_count` (schema v1→v2).
/// Guarded: only runs if the legacy column still exists, so fresh DBs (already v2) are unaffected.
const MIGRATION_002_RENAME: &str = "
ALTER TABLE runtime_sessions RENAME COLUMN state_revision TO turn_count;
";

const MIGRATION_002_RECORD: &str = "
INSERT OR IGNORE INTO schema_version (version, description) VALUES (2, 'rename state_revision to turn_count');
";

/// Persistence layer — SQLite session state storage.
pub struct Persistence {
    conn: Connection,
}

impl Persistence {
    /// Open or create a database at the given path.
    pub fn open(path: &str) -> Result<Self, PersistenceError> {
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA_SQL)?;
        conn.execute_batch(MIGRATION_001)?;
        Self::run_migration_002(&conn)?;
        Ok(Persistence { conn })
    }

    /// Open an in-memory database (for tests).
    pub fn open_memory() -> Result<Self, PersistenceError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA_SQL)?;
        conn.execute_batch(MIGRATION_001)?;
        Self::run_migration_002(&conn)?;
        Ok(Persistence { conn })
    }

    /// Run migration 002 (column rename) if not yet applied.
    /// Checks whether the legacy `state_revision` column exists before renaming,
    /// so this is a no-op on fresh databases that already use `turn_count`.
    fn run_migration_002(conn: &Connection) -> Result<(), PersistenceError> {
        let applied: i64 = conn
            .query_row(
                "SELECT MAX(version) FROM schema_version",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        if applied >= 2 {
            return Ok(());
        }
        let has_legacy_column: bool = conn
            .prepare("PRAGMA table_info(runtime_sessions)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .filter_map(|r| r.ok())
            .any(|name| name == "state_revision");
        if has_legacy_column {
            conn.execute_batch(MIGRATION_002_RENAME)?;
        }
        conn.execute_batch(MIGRATION_002_RECORD)?;
        Ok(())
    }

    /// Save system state for a session.
    pub fn save_state(
        &self,
        session_id: &str,
        state: &SystemState,
    ) -> Result<(), PersistenceError> {
        let json = serde_json::to_string(state)
            .map_err(|e| PersistenceError::Serialization(e.to_string()))?;

        self.conn.execute(
            "INSERT INTO runtime_sessions (id, state_json, last_active, turn_count) VALUES (?1, ?2, datetime('now'), ?3)
             ON CONFLICT(id) DO UPDATE SET state_json=excluded.state_json, last_active=datetime('now'), turn_count=excluded.turn_count",
            params![session_id, json, state.dialogue.turn_count],
        )?;

        Ok(())
    }

    /// Load system state for a session.
    pub fn load_state(&self, session_id: &str) -> Result<Option<SystemState>, PersistenceError> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT state_json FROM runtime_sessions WHERE id = ?1")?;

        let result = stmt.query_row(params![session_id], |row| {
            let json: String = row.get(0)?;
            Ok(json)
        });

        match result {
            Ok(json) => {
                let state: SystemState = serde_json::from_str(&json)
                    .map_err(|e| PersistenceError::Serialization(e.to_string()))?;
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

        sessions.collect::<Result<Vec<_>, _>>().map_err(PersistenceError::SQLite)
    }

    /// Delete a session.
    pub fn delete_session(&self, session_id: &str) -> Result<(), PersistenceError> {
        self.conn
            .execute(
                "DELETE FROM runtime_sessions WHERE id = ?1",
                params![session_id],
            )?;
        Ok(())
    }

    /// Get the current schema version.
    pub fn schema_version(&self) -> Result<i64, PersistenceError> {
        let version: i64 = self
            .conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
                row.get(0)
            })
            .unwrap_or(0);
        Ok(version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qxfx0_types::governance::{GovernanceEvent, GovernanceEventType};
    use qxfx0_types::system_state::*;

    #[test]
    fn test_open_memory() {
        let db = Persistence::open_memory();
        assert!(db.is_ok());
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
        db.save_state("s1", &SystemState::default()).unwrap();
        db.save_state("s2", &SystemState::default()).unwrap();
        let sessions = db.list_sessions().unwrap();
        assert_eq!(sessions.len(), 2);
    }

    #[test]
    fn test_delete_session() {
        let db = Persistence::open_memory().unwrap();
        db.save_state("s1", &SystemState::default()).unwrap();
        db.delete_session("s1").unwrap();
        assert!(db.load_state("s1").unwrap().is_none());
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
            loaded.governance_log.count_by_type(&GovernanceEventType::TurnCompleted),
            1
        );
        assert_eq!(
            loaded.governance_log.count_by_type(&GovernanceEventType::GuardBlocked),
            1
        );
    }
}
