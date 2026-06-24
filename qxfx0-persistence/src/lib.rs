use qxfx0_types::system_state::SystemState;
use rusqlite::{params, Connection};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PersistenceError {
    #[error("SQLite error: {0}")]
    SQLite(String),
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
    state_revision INTEGER NOT NULL DEFAULT 0
);
";

const MIGRATION_001: &str = "
INSERT OR IGNORE INTO schema_version (version, description) VALUES (1, 'initial schema');
";

/// Persistence layer — SQLite session state storage.
pub struct Persistence {
    conn: Connection,
}

impl Persistence {
    /// Open or create a database at the given path.
    pub fn open(path: &str) -> Result<Self, PersistenceError> {
        let conn = Connection::open(path).map_err(|e| PersistenceError::SQLite(e.to_string()))?;
        conn.execute_batch(SCHEMA_SQL)
            .map_err(|e| PersistenceError::SQLite(e.to_string()))?;
        conn.execute_batch(MIGRATION_001)
            .map_err(|e| PersistenceError::SQLite(e.to_string()))?;
        Ok(Persistence { conn })
    }

    /// Open an in-memory database (for tests).
    pub fn open_memory() -> Result<Self, PersistenceError> {
        let conn =
            Connection::open_in_memory().map_err(|e| PersistenceError::SQLite(e.to_string()))?;
        conn.execute_batch(SCHEMA_SQL)
            .map_err(|e| PersistenceError::SQLite(e.to_string()))?;
        conn.execute_batch(MIGRATION_001)
            .map_err(|e| PersistenceError::SQLite(e.to_string()))?;
        Ok(Persistence { conn })
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
            "INSERT OR REPLACE INTO runtime_sessions (id, state_json, last_active, state_revision) VALUES (?1, ?2, datetime('now'), ?3)",
            params![session_id, json, state.dialogue.turn_count],
        ).map_err(|e| PersistenceError::SQLite(e.to_string()))?;

        Ok(())
    }

    /// Load system state for a session.
    pub fn load_state(&self, session_id: &str) -> Result<Option<SystemState>, PersistenceError> {
        let mut stmt = self
            .conn
            .prepare("SELECT state_json FROM runtime_sessions WHERE id = ?1")
            .map_err(|e| PersistenceError::SQLite(e.to_string()))?;

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
            Err(e) => Err(PersistenceError::SQLite(e.to_string())),
        }
    }

    /// List all session IDs.
    pub fn list_sessions(&self) -> Result<Vec<String>, PersistenceError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM runtime_sessions ORDER BY last_active DESC")
            .map_err(|e| PersistenceError::SQLite(e.to_string()))?;

        let sessions = stmt
            .query_map([], |row| {
                let id: String = row.get(0)?;
                Ok(id)
            })
            .map_err(|e| PersistenceError::SQLite(e.to_string()))?;

        sessions
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| PersistenceError::SQLite(e.to_string()))
    }

    /// Delete a session.
    pub fn delete_session(&self, session_id: &str) -> Result<(), PersistenceError> {
        self.conn
            .execute(
                "DELETE FROM runtime_sessions WHERE id = ?1",
                params![session_id],
            )
            .map_err(|e| PersistenceError::SQLite(e.to_string()))?;
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
}
