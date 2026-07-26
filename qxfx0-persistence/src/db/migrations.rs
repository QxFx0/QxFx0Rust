use rusqlite::{Connection, Result};

/// Current SQLite schema understood by this build.
///
/// Versions 1-6 existed in two incompatible migration systems: early builds
/// tracked versions in a `schema_version` table while later development builds
/// used `PRAGMA user_version`.  Version 7 is deliberately idempotent and does
/// not write to the legacy table, so databases produced by either lineage can
/// be upgraded safely.
pub const CURRENT_SCHEMA_VERSION: i64 = 7;

const SCHEMA_V7: &str = r#"
CREATE TABLE IF NOT EXISTS runtime_sessions (
    id TEXT PRIMARY KEY,
    state_json TEXT NOT NULL,
    last_active TEXT NOT NULL DEFAULT (datetime('now')),
    turn_count INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS session_graphs (
    session_id TEXT PRIMARY KEY,
    atoms_json TEXT NOT NULL,
    edges_json TEXT NOT NULL,
    FOREIGN KEY (session_id) REFERENCES runtime_sessions(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS session_semantic (
    session_id TEXT PRIMARY KEY,
    field_json TEXT NOT NULL,
    essence_json TEXT NOT NULL,
    adjunction_json TEXT NOT NULL,
    commitments_json TEXT,
    FOREIGN KEY (session_id) REFERENCES runtime_sessions(id) ON DELETE CASCADE
);

PRAGMA user_version = 7;
"#;

/// Apply the compatibility schema in one transaction.
///
/// `CREATE TABLE IF NOT EXISTS` preserves the original `runtime_sessions`
/// table (including its historical `started_at` column) and all session rows.
/// We intentionally leave the old `schema_version` table untouched because
/// its shape differs between released database generations.
pub fn apply_migrations(conn: &mut Connection) -> Result<()> {
    let current_version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if current_version >= CURRENT_SCHEMA_VERSION {
        return Ok(());
    }

    let tx = conn.transaction()?;
    tx.execute_batch(SCHEMA_V7)?;
    tx.commit()?;
    Ok(())
}
