use rusqlite::{Connection, Result};

/// Current SQLite schema understood by this build.
///
/// Versions 1-6 existed in two incompatible migration systems: early builds
/// tracked versions in a `schema_version` table while later development builds
/// used `PRAGMA user_version`. Version 7 introduced normalized session tables;
/// version 8 adds the bounded perspective store without touching the legacy
/// `schema_version` table.
pub const CURRENT_SCHEMA_VERSION: i64 = 8;

const COMPATIBILITY_SCHEMA: &str = r#"
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
    perspective_json TEXT NOT NULL DEFAULT '{"opinions":{},"episodes":[],"next_episode_id":0}',
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
    tx.execute_batch(COMPATIBILITY_SCHEMA)?;
    let has_perspective_column = {
        let mut columns = tx.prepare("PRAGMA table_info(session_semantic)")?;
        let names = columns.query_map([], |row| row.get::<_, String>(1))?;
        let mut found = false;
        for name in names {
            if name? == "perspective_json" {
                found = true;
                break;
            }
        }
        found
    };
    if !has_perspective_column {
        tx.execute_batch(
            r#"ALTER TABLE session_semantic
               ADD COLUMN perspective_json TEXT NOT NULL
               DEFAULT '{"opinions":{},"episodes":[],"next_episode_id":0}';"#,
        )?;
    }
    tx.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)?;
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v7_semantic_table_gets_perspective_column() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                r#"
                CREATE TABLE runtime_sessions (
                    id TEXT PRIMARY KEY,
                    state_json TEXT NOT NULL,
                    last_active TEXT NOT NULL DEFAULT (datetime('now')),
                    turn_count INTEGER NOT NULL DEFAULT 0
                );
                CREATE TABLE session_semantic (
                    session_id TEXT PRIMARY KEY,
                    field_json TEXT NOT NULL,
                    essence_json TEXT NOT NULL,
                    adjunction_json TEXT NOT NULL,
                    commitments_json TEXT,
                    FOREIGN KEY (session_id) REFERENCES runtime_sessions(id) ON DELETE CASCADE
                );
                PRAGMA user_version = 7;
                "#,
            )
            .unwrap();

        apply_migrations(&mut connection).unwrap();

        assert_eq!(
            connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            8
        );
        let has_column = connection
            .prepare("PRAGMA table_info(session_semantic)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .any(|name| name.unwrap() == "perspective_json");
        assert!(has_column);
    }
}
