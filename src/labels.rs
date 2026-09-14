use chrono::Utc;
use libsql::params;
use serde::Serialize;
use std::collections::HashMap;
use thiserror::Error;

use crate::db::{self, Store};

#[derive(Error, Debug)]
pub enum LabelError {
    #[error("Invalid label number {0}: must be 0-5 (0 to clear)")]
    InvalidLabel(u8),
    #[error("Database error: {0}")]
    Db(#[from] db::DbError),
}

pub type LabelResult<T> = Result<T, LabelError>;

#[derive(Debug, Clone, Serialize)]
pub struct TriageLabel {
    pub rowid: i64,
    pub message_id: String,
    pub label_number: u8,
    pub label_name: String,
    pub assigned_at: String,
}

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

pub fn assign_label(store: &Store, rowid: i64, message_id: &str, label: u8) -> LabelResult<()> {
    store.transaction(|store| {
        if label > 5 {
            return Err(LabelError::InvalidLabel(label));
        }

        let identity_id = db::ensure_identity(store, rowid, message_id)?;
        if label == 0 {
            store.execute(
                "DELETE FROM mail_labels WHERE identity_id = ?1",
                params![identity_id],
            )?;
        } else {
            let now = Utc::now().to_rfc3339();
            store.execute(
                "INSERT INTO mail_labels (identity_id, label_number, assigned_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(identity_id) DO UPDATE SET
                label_number = excluded.label_number,
                assigned_at = excluded.assigned_at",
                params![identity_id, i64::from(label), now],
            )?;
        }

        Ok(())
    })
}

pub fn get_label(store: &Store, rowid: i64, message_id: &str) -> LabelResult<Option<TriageLabel>> {
    let Some(identity_id) = identity_id_for(store, rowid, message_id)? else {
        return Ok(None);
    };
    Ok(store.one(
        "SELECT l.label_number, l.assigned_at, i.message_id
         FROM mail_labels l
         JOIN mail_identities i ON i.id = l.identity_id
         WHERE l.identity_id = ?1",
        params![identity_id],
        |row| {
            let label_number = row.get::<i64>(0)? as u8;
            Ok(TriageLabel {
                rowid,
                message_id: row.get(2)?,
                label_number,
                label_name: label_name(label_number).to_string(),
                assigned_at: row.get(1)?,
            })
        },
    )?)
}

pub fn get_labels_for_messages(
    store: &Store,
    messages: &[(i64, String)],
) -> LabelResult<HashMap<i64, u8>> {
    let labels: HashMap<String, u8> = store
        .all(
            "SELECT i.message_id, l.label_number FROM mail_labels l
         JOIN mail_identities i ON i.id = l.identity_id",
            (),
            |row| Ok((row.get(0)?, row.get::<i64>(1)? as u8)),
        )?
        .into_iter()
        .collect();
    Ok(messages
        .iter()
        .filter_map(|(rowid, message_id)| {
            if message_id.trim().is_empty() {
                return None;
            }
            labels.get(message_id).map(|label| (*rowid, *label))
        })
        .collect())
}

pub fn get_emails_by_label(store: &Store, label: u8) -> LabelResult<Vec<i64>> {
    if label == 0 || label > 5 {
        return Err(LabelError::InvalidLabel(label));
    }
    Ok(store.all(
        "SELECT s.source_id
         FROM mail_labels l
         JOIN mail_machine_identity_sources s ON s.identity_id = l.identity_id
         WHERE l.label_number = ?1 AND s.machine_id = ?2
         ORDER BY l.assigned_at DESC",
        params![i64::from(label), db::machine_id()?],
        |row| Ok(row.get(0)?),
    )?)
}

fn identity_id_for(store: &Store, _rowid: i64, message_id: &str) -> LabelResult<Option<i64>> {
    if message_id.trim().is_empty() {
        return Ok(None);
    }
    Ok(store.one(
        "SELECT id FROM mail_identities WHERE message_id = ?1",
        params![message_id],
        |row| Ok(row.get(0)?),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_lookup_follows_message_id_not_reused_rowid() {
        let store = db::test_store().unwrap();
        assign_label(&store, 1, "old@test", 3).unwrap();
        let stale = get_label(&store, 1, "new@test").unwrap();
        assert!(stale.is_none());
    }

    #[test]
    fn aliases_are_preserved_for_same_message() {
        let store = db::test_store().unwrap();
        assign_label(&store, 1, "same@test", 2).unwrap();
        assign_label(&store, 99, "same@test", 4).unwrap();
        let label = get_label(&store, 1, "same@test").unwrap().unwrap();
        assert_eq!(label.label_number, 4);
        assert_eq!(get_emails_by_label(&store, 4).unwrap(), vec![1, 99]);
    }

    #[test]
    fn clear_label_removes_identity_label() {
        let store = db::test_store().unwrap();
        assign_label(&store, 1, "msg@test", 2).unwrap();
        assign_label(&store, 1, "msg@test", 0).unwrap();
        assert!(get_label(&store, 1, "msg@test").unwrap().is_none());
    }
}

#[cfg(test)]
mod regression_tests;
