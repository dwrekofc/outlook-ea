use chrono::Utc;
use rusqlite::Connection;
use serde::Serialize;
use thiserror::Error;

use crate::db;

#[derive(Error, Debug)]
pub enum LabelError {
    #[error("Invalid label number {0}: must be 0-5 (0 to clear)")]
    InvalidLabel(u8),
    #[error("Database error: {0}")]
    Db(#[from] db::DbError),
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

pub type LabelResult<T> = Result<T, LabelError>;

#[derive(Debug, Clone, Serialize)]
pub struct TriageLabel {
    pub rowid: i64,
    pub label_number: u8,
    pub label_name: String,
    pub assigned_at: String,
}

/// Human-readable name for each label number.
pub fn label_name(n: u8) -> &'static str {
    match n {
        1 => "Follow Up",
        2 => "Waiting",
        3 => "Reference",
        4 => "Read Later",
        5 => "Receipts",
        _ => "Unknown",
    }
}

/// Assign a label (1-5) to an email. Replaces any existing label.
/// Label 0 clears the label.
pub fn assign_label(conn: &Connection, rowid: i64, message_id: &str, label: u8) -> LabelResult<()> {
    if label > 5 {
        return Err(LabelError::InvalidLabel(label));
    }

    // Ensure identity mapping exists
    db::ensure_identity(conn, rowid, message_id)?;

    if label == 0 {
        // Clear label
        conn.execute("DELETE FROM labels WHERE rowid = ?1", [rowid])?;
    } else {
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO labels (rowid, label_number, assigned_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(rowid) DO UPDATE SET label_number = excluded.label_number, assigned_at = excluded.assigned_at",
            rusqlite::params![rowid, label, now],
        )?;
    }

    Ok(())
}

/// Get the label for a specific email, if any.
pub fn get_label(conn: &Connection, rowid: i64) -> LabelResult<Option<TriageLabel>> {
    let mut stmt =
        conn.prepare("SELECT rowid, label_number, assigned_at FROM labels WHERE rowid = ?1")?;

    let result = stmt
        .query_row([rowid], |row| {
            let rowid: i64 = row.get(0)?;
            let label_number: u8 = row.get(1)?;
            let assigned_at: String = row.get(2)?;
            Ok(TriageLabel {
                rowid,
                label_number,
                label_name: label_name(label_number).to_string(),
                assigned_at,
            })
        })
        .ok();

    Ok(result)
}

/// Get all emails with a specific label.
pub fn get_emails_by_label(conn: &Connection, label: u8) -> LabelResult<Vec<i64>> {
    if label == 0 || label > 5 {
        return Err(LabelError::InvalidLabel(label));
    }

    let mut stmt =
        conn.prepare("SELECT rowid FROM labels WHERE label_number = ?1 ORDER BY assigned_at DESC")?;
    let rows: Vec<i64> = stmt
        .query_map([label], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(rows)
}

/// Get all rowids that have NO label (untriaged).
/// Requires a list of candidate rowids (from the Envelope Index).
pub fn get_untriaged(conn: &Connection, candidate_rowids: &[i64]) -> LabelResult<Vec<i64>> {
    if candidate_rowids.is_empty() {
        return Ok(vec![]);
    }

    let sql = format!(
        "SELECT rowid FROM ({ids}) WHERE rowid NOT IN (SELECT rowid FROM labels)",
        ids = candidate_rowids
            .iter()
            .map(|id| format!("SELECT {id} AS rowid"))
            .collect::<Vec<_>>()
            .join(" UNION ALL ")
    );

    let mut stmt = conn.prepare(&sql)?;
    let rows: Vec<i64> = stmt
        .query_map([], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(rows)
}

/// Get all labels as a map (rowid -> label_number) for batch joining.
pub fn get_all_labels(conn: &Connection) -> LabelResult<std::collections::HashMap<i64, u8>> {
    let mut stmt = conn.prepare("SELECT rowid, label_number FROM labels")?;
    let map = stmt
        .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, u8>(1)?)))?
        .filter_map(|r| r.ok())
        .collect();
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_overlay_db_memory;

    #[test]
    fn test_assign_and_get_label() {
        let conn = open_overlay_db_memory().unwrap();
        assign_label(&conn, 1, "msg@test", 3).unwrap();
        let label = get_label(&conn, 1).unwrap().unwrap();
        assert_eq!(label.label_number, 3);
        assert_eq!(label.label_name, "Reference");
    }

    #[test]
    fn test_assign_replaces_existing() {
        let conn = open_overlay_db_memory().unwrap();
        assign_label(&conn, 1, "msg@test", 1).unwrap();
        assign_label(&conn, 1, "msg@test", 4).unwrap();
        let label = get_label(&conn, 1).unwrap().unwrap();
        assert_eq!(label.label_number, 4);
        assert_eq!(label.label_name, "Read Later");
    }

    #[test]
    fn test_clear_label() {
        let conn = open_overlay_db_memory().unwrap();
        assign_label(&conn, 1, "msg@test", 2).unwrap();
        assign_label(&conn, 1, "msg@test", 0).unwrap();
        let label = get_label(&conn, 1).unwrap();
        assert!(label.is_none());
    }

    #[test]
    fn test_invalid_label() {
        let conn = open_overlay_db_memory().unwrap();
        let err = assign_label(&conn, 1, "msg@test", 6);
        assert!(err.is_err());
    }

    #[test]
    fn test_get_emails_by_label() {
        let conn = open_overlay_db_memory().unwrap();
        assign_label(&conn, 10, "a@t", 1).unwrap();
        assign_label(&conn, 20, "b@t", 1).unwrap();
        assign_label(&conn, 30, "c@t", 2).unwrap();

        let follow_ups = get_emails_by_label(&conn, 1).unwrap();
        assert_eq!(follow_ups.len(), 2);
        assert!(follow_ups.contains(&10));
        assert!(follow_ups.contains(&20));
    }

    #[test]
    fn test_get_untriaged() {
        let conn = open_overlay_db_memory().unwrap();
        assign_label(&conn, 1, "a@t", 1).unwrap();
        // rowid 2 and 3 have no label
        db::ensure_identity(&conn, 2, "b@t").unwrap();
        db::ensure_identity(&conn, 3, "c@t").unwrap();

        let untriaged = get_untriaged(&conn, &[1, 2, 3]).unwrap();
        assert_eq!(untriaged.len(), 2);
        assert!(untriaged.contains(&2));
        assert!(untriaged.contains(&3));
    }

    #[test]
    fn test_get_no_label_returns_none() {
        let conn = open_overlay_db_memory().unwrap();
        let label = get_label(&conn, 999).unwrap();
        assert!(label.is_none());
    }

    #[test]
    fn test_labels_persist() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");

        {
            let conn = crate::db::open_overlay_db(&path).unwrap();
            assign_label(&conn, 5, "persist@test", 2).unwrap();
        }
        {
            let conn = crate::db::open_overlay_db(&path).unwrap();
            let label = get_label(&conn, 5).unwrap().unwrap();
            assert_eq!(label.label_number, 2);
        }
    }

    #[test]
    fn test_get_all_labels() {
        let conn = open_overlay_db_memory().unwrap();
        assign_label(&conn, 1, "a@t", 1).unwrap();
        assign_label(&conn, 2, "b@t", 3).unwrap();
        let map = get_all_labels(&conn).unwrap();
        assert_eq!(map.len(), 2);
        assert_eq!(map[&1], 1);
        assert_eq!(map[&2], 3);
    }
}
