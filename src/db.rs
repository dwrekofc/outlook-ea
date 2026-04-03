use rusqlite::Connection;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DbError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type DbResult<T> = Result<T, DbError>;

/// Open (or create) the overlay database at the given path and run migrations.
pub fn open_overlay_db(path: &Path) -> DbResult<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    migrate(&conn)?;
    Ok(conn)
}

/// Open an in-memory overlay database (for testing).
pub fn open_overlay_db_memory() -> DbResult<Connection> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;
    migrate(&conn)?;
    Ok(conn)
}

/// Return the default overlay DB path: ~/.mea/overlay.db
pub fn default_overlay_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".mea").join("overlay.db")
}

fn migrate(conn: &Connection) -> DbResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER NOT NULL
        );",
    )?;

    let version: i32 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_version",
        [],
        |r| r.get(0),
    )?;

    if version < 1 {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS email_identity (
                rowid INTEGER PRIMARY KEY,
                message_id TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_email_identity_message_id
                ON email_identity(message_id);

            CREATE TABLE IF NOT EXISTS labels (
                rowid INTEGER PRIMARY KEY REFERENCES email_identity(rowid),
                label_number INTEGER NOT NULL CHECK(label_number BETWEEN 1 AND 5),
                assigned_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS cached_bodies (
                rowid INTEGER PRIMARY KEY REFERENCES email_identity(rowid),
                body_text TEXT NOT NULL,
                body_format TEXT NOT NULL CHECK(body_format IN ('plain', 'markdown')),
                cached_at TEXT NOT NULL
            );

            INSERT INTO schema_version (version) VALUES (1);",
        )?;
    }

    if version < 2 {
        conn.execute_batch(
            "ALTER TABLE cached_bodies ADD COLUMN cached_to TEXT NOT NULL DEFAULT '';
             ALTER TABLE cached_bodies ADD COLUMN cached_cc TEXT NOT NULL DEFAULT '';
             INSERT INTO schema_version (version) VALUES (2);",
        )?;
    }

    if version < 3 {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS nodes (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                node_type   TEXT NOT NULL,
                name        TEXT NOT NULL,
                email       TEXT,
                description TEXT,
                metadata    TEXT DEFAULT '{}',
                is_vip      INTEGER DEFAULT 0,
                created_at  TEXT NOT NULL,
                updated_at  TEXT NOT NULL
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_nodes_email ON nodes(email) WHERE email IS NOT NULL;
            CREATE INDEX IF NOT EXISTS idx_nodes_type ON nodes(node_type);

            CREATE TABLE IF NOT EXISTS edges (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                source_id   INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
                target_id   INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
                predicate   TEXT NOT NULL,
                context     TEXT,
                weight      REAL DEFAULT 1.0,
                created_at  TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_edges_source ON edges(source_id);
            CREATE INDEX IF NOT EXISTS idx_edges_target ON edges(target_id);
            CREATE INDEX IF NOT EXISTS idx_edges_predicate ON edges(predicate);
            CREATE UNIQUE INDEX IF NOT EXISTS idx_edges_unique ON edges(source_id, target_id, predicate);

            INSERT INTO schema_version (version) VALUES (3);",
        )?;
    }

    Ok(())
}

/// Ensure an email identity mapping exists. Upserts the message_id for a given rowid.
pub fn ensure_identity(conn: &Connection, rowid: i64, message_id: &str) -> DbResult<()> {
    conn.execute(
        "INSERT INTO email_identity (rowid, message_id) VALUES (?1, ?2)
         ON CONFLICT(rowid) DO UPDATE SET message_id = excluded.message_id",
        rusqlite::params![rowid, message_id],
    )?;
    Ok(())
}

/// Look up a rowid by message_id (for re-mapping after index rebuilds).
pub fn find_rowid_by_message_id(conn: &Connection, message_id: &str) -> DbResult<Option<i64>> {
    let mut stmt = conn.prepare("SELECT rowid FROM email_identity WHERE message_id = ?1")?;
    let result = stmt.query_row([message_id], |r| r.get(0)).ok();
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_overlay_db() {
        let conn = open_overlay_db_memory().unwrap();
        let version: i32 = conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 3);
    }

    #[test]
    fn test_migration_idempotent() {
        let conn = open_overlay_db_memory().unwrap();
        // Running migrate again should not fail
        migrate(&conn).unwrap();
        let version: i32 = conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 3);
    }

    #[test]
    fn test_ensure_identity() {
        let conn = open_overlay_db_memory().unwrap();
        ensure_identity(&conn, 42, "abc@example.com").unwrap();
        let mid: String = conn
            .query_row(
                "SELECT message_id FROM email_identity WHERE rowid = 42",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(mid, "abc@example.com");
    }

    #[test]
    fn test_ensure_identity_upsert() {
        let conn = open_overlay_db_memory().unwrap();
        ensure_identity(&conn, 42, "old@example.com").unwrap();
        ensure_identity(&conn, 42, "new@example.com").unwrap();
        let mid: String = conn
            .query_row(
                "SELECT message_id FROM email_identity WHERE rowid = 42",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(mid, "new@example.com");
    }

    #[test]
    fn test_find_rowid_by_message_id() {
        let conn = open_overlay_db_memory().unwrap();
        ensure_identity(&conn, 99, "test@example.com").unwrap();
        assert_eq!(
            find_rowid_by_message_id(&conn, "test@example.com").unwrap(),
            Some(99)
        );
        assert_eq!(
            find_rowid_by_message_id(&conn, "nonexistent@example.com").unwrap(),
            None
        );
    }

    #[test]
    fn test_tables_exist() {
        let conn = open_overlay_db_memory().unwrap();
        // Verify all expected tables exist by querying them
        conn.execute_batch(
            "SELECT * FROM email_identity LIMIT 0;
             SELECT * FROM labels LIMIT 0;
             SELECT * FROM cached_bodies LIMIT 0;
             SELECT * FROM schema_version LIMIT 0;
             SELECT * FROM nodes LIMIT 0;
             SELECT * FROM edges LIMIT 0;",
        )
        .unwrap();
    }

    #[test]
    fn test_migration_v3_creates_nodes_edges() {
        let conn = open_overlay_db_memory().unwrap();
        // Verify the nodes and edges tables exist by querying them
        conn.execute_batch(
            "SELECT * FROM nodes LIMIT 0;
             SELECT * FROM edges LIMIT 0;",
        )
        .unwrap();
    }

    #[test]
    fn test_migration_v2_adds_cached_to_cc() {
        let conn = open_overlay_db_memory().unwrap();
        // Verify the cached_to and cached_cc columns exist by inserting into them
        ensure_identity(&conn, 1, "test@msg").unwrap();
        conn.execute(
            "INSERT INTO cached_bodies (rowid, body_text, body_format, cached_at, cached_to, cached_cc) VALUES (1, 'body', 'plain', '2024-01-01', '[\"a@b\"]', '[]')",
            [],
        )
        .unwrap();
        let (to, cc): (String, String) = conn
            .query_row(
                "SELECT cached_to, cached_cc FROM cached_bodies WHERE rowid = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(to, "[\"a@b\"]");
        assert_eq!(cc, "[]");
    }

    #[test]
    fn test_persistent_db() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");

        // Create and insert
        {
            let conn = open_overlay_db(&path).unwrap();
            ensure_identity(&conn, 1, "persist@test.com").unwrap();
        }

        // Reopen and verify
        {
            let conn = open_overlay_db(&path).unwrap();
            let mid: String = conn
                .query_row(
                    "SELECT message_id FROM email_identity WHERE rowid = 1",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(mid, "persist@test.com");
        }
    }
}
