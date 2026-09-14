use chrono::DateTime;
use libsql::{Value, params_from_iter};
use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::{db::Store, graph, labels};

#[derive(Error, Debug)]
pub enum DataError {
    #[error("Database error: {0}")]
    Db(#[from] crate::db::DbError),
    #[error("Envelope Index not found at {0}")]
    EnvelopeNotFound(PathBuf),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Storage error: {0}")]
    Storage(String),
}

pub type DataResult<T> = Result<T, DataError>;

mod model;
pub use model::*;

/// Locate Apple Mail's Envelope Index database.
pub fn find_envelope_index() -> DataResult<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let mail_dir = PathBuf::from(&home).join("Library/Mail");

    // Search for V* directories (V10, V11, etc.)
    if mail_dir.exists() {
        let mut versions: Vec<_> = std::fs::read_dir(&mail_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name().to_str().is_some_and(|n| {
                    n.starts_with('V') && n[1..].chars().all(|c| c.is_ascii_digit())
                })
            })
            .collect();
        versions.sort_by_key(|b| std::cmp::Reverse(b.file_name()));

        for v in versions {
            let idx = v.path().join("MailData/Envelope Index");
            if idx.exists() {
                return Ok(idx);
            }
        }
    }

    let fallback = mail_dir.join("V10/MailData/Envelope Index");
    Err(DataError::EnvelopeNotFound(fallback))
}

/// Open the Envelope Index read-only.
pub fn open_envelope_index(path: &Path) -> DataResult<Store> {
    Ok(Store::open_read_only(path)?)
}

/// Build the folder WHERE clause for inbox queries.
/// Uses case-insensitive matching since mailbox URLs have COLLATE BINARY.
fn inbox_where(folder: Option<&str>) -> (String, Vec<Value>) {
    if let Some(f) = folder {
        (
            "WHERE mb.url LIKE ?1".to_string(),
            vec![format!("%{f}%").into()],
        )
    } else {
        // Default: top-level Inbox only (not subfolders like Inbox/Kudos)
        ("WHERE mb.url LIKE '%/Inbox'".to_string(), vec![])
    }
}

/// Apple Mail V10 stores date_sent as Unix epoch seconds (integer).
/// Convert to ISO 8601 string.
pub fn unix_to_iso8601(unix_ts: i64) -> String {
    DateTime::from_timestamp(unix_ts, 0)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_default()
}

/// Extract folder name from a mailbox URL (e.g., "ews://uuid/Inbox" -> "Inbox").
pub fn folder_from_url(url: &str) -> String {
    url.rsplit('/')
        .find(|s| !s.is_empty())
        .unwrap_or("Unknown")
        .to_string()
}

/// List emails from the Envelope Index using V10 normalized schema.
/// Joins `subjects`, `addresses`, and `message_global_data` lookup tables.
pub fn list_emails(
    envelope_conn: &Store,
    folder: Option<&str>,
    page: usize,
    page_size: usize,
) -> DataResult<ListResponse> {
    // V10 schema: messages.subject -> subjects.ROWID (FK)
    //             messages.sender  -> addresses.ROWID (FK)
    //             messages.global_message_id -> message_global_data.ROWID (FK)
    //             messages.date_sent = Unix epoch integer

    let (where_clause, params) = inbox_where(folder);

    let count_sql = format!(
        "SELECT COUNT(*)
         FROM messages m
         JOIN mailboxes mb ON m.mailbox = mb.ROWID
         {where_clause}"
    );

    let total_count: usize = if params.is_empty() {
        envelope_conn
            .one(&count_sql, (), |row| Ok(row.get::<i64>(0)? as usize))?
            .unwrap_or(0)
    } else {
        envelope_conn
            .one(&count_sql, params_from_iter(params.clone()), |row| {
                Ok(row.get::<i64>(0)? as usize)
            })?
            .unwrap_or(0)
    };

    let offset = page * page_size;
    let query_sql = format!(
        "SELECT m.ROWID,
                COALESCE(mgd.message_id_header, '') as message_id,
                COALESCE(a.comment, '') as sender_name,
                COALESCE(a.address, '') as sender_address,
                COALESCE(m.subject_prefix, '') || COALESCE(sub.subject, '') as subject,
                COALESCE(m.date_sent, 0) as date_sent,
                COALESCE(m.read, 0) as is_read,
                COALESCE(mb.url, '') as folder_url,
                COALESCE(m.conversation_id, 0) as conversation_id
         FROM messages m
         JOIN subjects sub ON m.subject = sub.ROWID
         JOIN addresses a ON m.sender = a.ROWID
         JOIN mailboxes mb ON m.mailbox = mb.ROWID
         LEFT JOIN message_global_data mgd ON mgd.ROWID = m.global_message_id
         {where_clause}
         ORDER BY m.date_sent DESC
         LIMIT ?{limit_param} OFFSET ?{offset_param}",
        limit_param = params.len() + 1,
        offset_param = params.len() + 2,
    );

    let mut all_params = params;
    all_params.push((page_size as i64).into());
    all_params.push((offset as i64).into());

    let emails = envelope_conn.all(&query_sql, params_from_iter(all_params), |row| {
        let rowid: i64 = row.get(0)?;
        let message_id: String = row.get(1)?;
        let sender_name: String = row.get(2)?;
        let sender_address: String = row.get(3)?;
        let subject: String = row.get(4)?;
        let date_sent: i64 = row.get(5)?;
        let read: i32 = row.get(6)?;
        let folder_url: String = row.get(7)?;
        let conversation_id: i64 = row.get(8)?;

        Ok(EmailSummary {
            id: rowid,
            message_id,
            sender_name,
            sender_address,
            subject,
            date: unix_to_iso8601(date_sent),
            is_read: read != 0,
            folder: folder_from_url(&folder_url),
            conversation_id: if conversation_id != 0 {
                Some(conversation_id)
            } else {
                None
            },
            label: None,
            needs_reply: None,
            sender_context: None,
        })
    })?;

    Ok(ListResponse {
        emails,
        total_count,
        page,
        page_size,
    })
}

/// List emails with label/untriaged filtering applied BEFORE pagination.
///
/// When `label_filter` or `untriaged` is set, fetches all matching emails from
/// the envelope, joins labels, filters, then paginates. When neither is set,
/// uses the normal SQL-level pagination for efficiency.
#[allow(clippy::too_many_arguments)] // Preserve the existing listing API during the storage swap.
pub fn list_emails_filtered(
    envelope_conn: &Store,
    store: &Store,
    folder: Option<&str>,
    page: usize,
    page_size: usize,
    label_filter: Option<u8>,
    untriaged: bool,
    needs_reply: bool,
) -> DataResult<ListResponse> {
    let needs_label_filter = label_filter.is_some() || untriaged || needs_reply;

    // When filtering by label/untriaged, fetch all emails (no SQL pagination)
    // so we can filter correctly before paginating
    let (fetch_page, fetch_size) = if needs_label_filter {
        (0, usize::MAX)
    } else {
        (page, page_size)
    };

    let mut result = list_emails(envelope_conn, folder, fetch_page, fetch_size)?;

    // Join shared labels by portable message identity
    let identities: Vec<_> = result
        .emails
        .iter()
        .map(|email| (email.id, email.message_id.clone()))
        .collect();
    let label_map = labels::get_labels_for_messages(store, &identities)
        .map_err(|e| DataError::Storage(e.to_string()))?;
    let mut contexts = std::collections::HashMap::new();
    for email in &mut result.emails {
        email.label = label_map.get(&email.id).copied();
        if !contexts.contains_key(&email.sender_address) {
            let context = graph::get_sender_context(store, &email.sender_address)
                .map_err(|e| DataError::Storage(e.to_string()))?;
            contexts.insert(email.sender_address.clone(), context);
        }
        email.sender_context = contexts.get(&email.sender_address).cloned().flatten();
    }

    if needs_label_filter {
        // Apply label filter
        if let Some(lbl) = label_filter {
            result.emails.retain(|e| e.label == Some(lbl));
        }

        // Apply untriaged filter
        if untriaged {
            result.emails.retain(|e| e.label.is_none());
        }

        // Apply needs-reply filter: annotate then retain only those awaiting a reply.
        if needs_reply {
            let self_ids = self_address_ids(envelope_conn);
            for email in &mut result.emails {
                let nr =
                    compute_needs_reply(envelope_conn, email.id, email.conversation_id, &self_ids);
                email.needs_reply = Some(nr);
            }
            result.emails.retain(|e| e.needs_reply == Some(true));
        }

        // Manual pagination after filtering
        let total_count = result.emails.len();
        let offset = page * page_size;
        let emails: Vec<_> = result
            .emails
            .into_iter()
            .skip(offset)
            .take(page_size)
            .collect();

        Ok(ListResponse {
            emails,
            total_count,
            page,
            page_size,
        })
    } else {
        Ok(result)
    }
}

#[path = "data/thread.rs"]
mod thread;
pub use thread::*;

#[cfg(test)]
mod regression_tests;
