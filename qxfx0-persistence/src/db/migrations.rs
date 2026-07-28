use rusqlite::{Connection, Result};

/// Current SQLite schema understood by this build.
///
/// Versions 1-6 existed in two incompatible migration systems: early builds
/// tracked versions in a `schema_version` table while later development builds
/// used `PRAGMA user_version`. Version 8 adds typed stance provenance without
/// rewriting session rows, so databases from either lineage upgrade safely.
pub const CURRENT_SCHEMA_VERSION: i64 = 8;

const SCHEMA_V8: &str = r#"
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
    stance_provenance_json TEXT,
    FOREIGN KEY (session_id) REFERENCES runtime_sessions(id) ON DELETE CASCADE
);

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
    tx.execute_batch(SCHEMA_V8)?;
    let has_column = {
        let mut statement = tx.prepare("PRAGMA table_info(session_semantic)")?;
        let names = statement
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>>>()?;
        names.iter().any(|name| name == "stance_provenance_json")
    };
    if !has_column {
        tx.execute_batch("ALTER TABLE session_semantic ADD COLUMN stance_provenance_json TEXT")?;
    }
    tx.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)?;
    tx.commit()?;
    Ok(())
}
