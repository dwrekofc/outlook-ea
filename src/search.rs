use libsql::{Value, params_from_iter};
use serde::{Deserialize, Serialize};
use std::process::Command;
use thiserror::Error;

use crate::data::{EmailSummary, folder_from_url, unix_to_iso8601};
use crate::db::Store;
use crate::graph;

#[derive(Error, Debug)]
pub enum SearchError {
    #[error("Database error: {0}")]
    Db(#[from] crate::db::DbError),
    #[error("Spotlight error: {0}")]
    Spotlight(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Storage error: {0}")]
    Storage(String),
}

pub type SearchResult<T> = Result<T, SearchError>;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SearchQuery {
    pub sender: Option<String>,
    pub subject: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub body_text: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResponse {
    pub emails: Vec<EmailSummary>,
    pub total_count: usize,
}

/// Search emails using metadata filters against the V10 Envelope Index.
/// Joins subjects, addresses, and message_global_data lookup tables.
pub fn search_metadata(
    envelope_conn: &Store,
    query: &SearchQuery,
) -> SearchResult<Vec<EmailSummary>> {
    let mut conditions = vec!["1=1".to_string()];
    let mut params: Vec<Value> = vec![];
    let mut param_idx = 1;

    if let Some(ref sender) = query.sender {
        conditions.push(format!(
            "(a.address LIKE ?{pi} OR a.comment LIKE ?{pi})",
            pi = param_idx
        ));
        params.push(format!("%{sender}%").into());
        param_idx += 1;
    }

    if let Some(ref subject) = query.subject {
        conditions.push(format!("sub.subject LIKE ?{param_idx}"));
        params.push(format!("%{subject}%").into());
        param_idx += 1;
    }

    if let Some(ref date_from) = query.date_from
        && let Some(unix_ts) = iso8601_to_unix(date_from)
    {
        conditions.push(format!("m.date_sent >= ?{param_idx}"));
        params.push(unix_ts.into());
        param_idx += 1;
    }

    if let Some(ref date_to) = query.date_to
        && let Some(unix_ts) = iso8601_to_unix(date_to)
    {
        conditions.push(format!("m.date_sent <= ?{param_idx}"));
        params.push(unix_ts.into());
        param_idx += 1;
    }
    let _ = param_idx;

    let where_clause = conditions.join(" AND ");
    let sql = format!(
        "SELECT m.ROWID,
                COALESCE(mgd.message_id_header, '') as message_id,
                COALESCE(a.comment, '') as sender_name,
                COALESCE(a.address, '') as sender_address,
                COALESCE(m.subject_prefix, '') || COALESCE(sub.subject, '') as subject,
                COALESCE(m.date_sent, 0) as date_sent,
                COALESCE(m.read, 0) as is_read,
                COALESCE(mb.url, '') as folder_url
         FROM messages m
         JOIN subjects sub ON m.subject = sub.ROWID
         JOIN addresses a ON m.sender = a.ROWID
         JOIN mailboxes mb ON m.mailbox = mb.ROWID
         LEFT JOIN message_global_data mgd ON mgd.ROWID = m.global_message_id
         WHERE {where_clause}
         ORDER BY m.date_sent DESC"
    );

    Ok(envelope_conn.all(&sql, params_from_iter(params), |row| {
        let rowid: i64 = row.get(0)?;
        let message_id: String = row.get(1)?;
        let sender_name: String = row.get(2)?;
        let sender_address: String = row.get(3)?;
        let subject: String = row.get(4)?;
        let date_sent: i64 = row.get(5)?;
        let read: i64 = row.get(6)?;
        let folder_url: String = row.get(7)?;

        Ok(EmailSummary {
            id: rowid,
            message_id,
            sender_name,
            sender_address,
            subject,
            date: unix_to_iso8601(date_sent),
            is_read: read != 0,
            folder: folder_from_url(&folder_url),
            conversation_id: None,
            label: None,
            needs_reply: None,
            sender_context: None,
        })
    })?)
}

/// Search email bodies using macOS Spotlight (mdfind).
pub fn search_spotlight(body_text: &str) -> SearchResult<Vec<String>> {
    let output = Command::new("mdfind")
        .args([
            "-onlyin",
            &format!(
                "{}/Library/Mail",
                std::env::var("HOME").unwrap_or_else(|_| ".".into())
            ),
            &format!("kMDItemTextContent == '*{body_text}*'cd"),
        ])
        .output()
        .map_err(|e| SearchError::Spotlight(e.to_string()))?;

    if !output.status.success() {
        return Err(SearchError::Spotlight(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }

    let paths: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| l.ends_with(".emlx"))
        .map(|l| l.to_string())
        .collect();

    Ok(paths)
}

/// Extract rowid from an .emlx file path (e.g., "/path/to/12345.emlx" -> 12345).
pub fn rowid_from_emlx_path(path: &str) -> Option<i64> {
    let filename = path.rsplit('/').next()?;
    let stem = filename.strip_suffix(".emlx")?;
    // Handle partial files like "12345.partial.emlx"
    let id_str = stem.split('.').next()?;
    id_str.parse().ok()
}

/// Combined search: metadata + optional body text via Spotlight.
/// When `overlay_conn` is provided, enriches results with sender context from the graph.
pub fn search_emails(
    envelope_conn: &Store,
    query: &SearchQuery,
    store: Option<&Store>,
) -> SearchResult<SearchResponse> {
    let mut metadata_results = search_metadata(envelope_conn, query)?;

    // If body search is requested, intersect with Spotlight results
    if let Some(ref body_text) = query.body_text {
        let spotlight_paths = search_spotlight(body_text)?;
        let spotlight_rowids: std::collections::HashSet<i64> = spotlight_paths
            .iter()
            .filter_map(|p| rowid_from_emlx_path(p))
            .collect();

        if !spotlight_rowids.is_empty() {
            if metadata_results.is_empty()
                && query.sender.is_none()
                && query.subject.is_none()
                && query.date_from.is_none()
                && query.date_to.is_none()
            {
                // Body-only search: fetch metadata for Spotlight matches
                let placeholders: String = spotlight_rowids
                    .iter()
                    .map(|_| "?")
                    .collect::<Vec<_>>()
                    .join(",");
                let sql = format!(
                    "SELECT m.ROWID,
                            COALESCE(mgd.message_id_header, '') as message_id,
                            COALESCE(a.comment, '') as sender_name,
                            COALESCE(a.address, '') as sender_address,
                            COALESCE(m.subject_prefix, '') || COALESCE(sub.subject, '') as subject,
                            COALESCE(m.date_sent, 0) as date_sent,
                            COALESCE(m.read, 0) as is_read,
                            COALESCE(mb.url, '') as folder_url
                     FROM messages m
                     JOIN subjects sub ON m.subject = sub.ROWID
                     JOIN addresses a ON m.sender = a.ROWID
                     JOIN mailboxes mb ON m.mailbox = mb.ROWID
                     LEFT JOIN message_global_data mgd ON mgd.ROWID = m.global_message_id
                     WHERE m.ROWID IN ({placeholders})
                     ORDER BY m.date_sent DESC"
                );
                let params: Vec<Value> = spotlight_rowids.iter().map(|&id| id.into()).collect();
                metadata_results = envelope_conn.all(&sql, params_from_iter(params), |row| {
                    let rowid: i64 = row.get(0)?;
                    let message_id: String = row.get(1)?;
                    let sender_name: String = row.get(2)?;
                    let sender_address: String = row.get(3)?;
                    let subject: String = row.get(4)?;
                    let date_sent: i64 = row.get(5)?;
                    let read: i64 = row.get(6)?;
                    let folder_url: String = row.get(7)?;

                    Ok(EmailSummary {
                        id: rowid,
                        message_id,
                        sender_name,
                        sender_address,
                        subject,
                        date: unix_to_iso8601(date_sent),
                        is_read: read != 0,
                        folder: folder_from_url(&folder_url),
                        conversation_id: None,
                        label: None,
                        needs_reply: None,
                        sender_context: None,
                    })
                })?;
            } else {
                // Intersect metadata results with Spotlight results
                metadata_results.retain(|e| spotlight_rowids.contains(&e.id));
            }
        } else {
            // Spotlight found nothing
            if !metadata_results.is_empty() {
                metadata_results.clear();
            }
        }
    }

    // Enrich with sender context from graph if overlay connection is available
    if let Some(store) = store {
        let mut seen: std::collections::HashMap<String, Option<graph::SenderContext>> =
            std::collections::HashMap::new();
        for email in &mut metadata_results {
            if email.sender_address.is_empty() {
                continue;
            }
            if !seen.contains_key(&email.sender_address) {
                let context = graph::get_sender_context(store, &email.sender_address)
                    .map_err(|e| SearchError::Storage(e.to_string()))?;
                seen.insert(email.sender_address.clone(), context);
            }
            let ctx = seen
                .get(&email.sender_address)
                .expect("sender context cached");
            if let Some(sc) = ctx.as_ref() {
                email.sender_context = Some(sc.clone());
            }
        }
    }

    let total_count = metadata_results.len();
    Ok(SearchResponse {
        emails: metadata_results,
        total_count,
    })
}

/// Convert ISO 8601 date string to Unix epoch seconds.
fn iso8601_to_unix(iso: &str) -> Option<i64> {
    let dt = chrono::DateTime::parse_from_rfc3339(iso).ok()?;
    Some(dt.timestamp())
}

#[cfg(test)]
mod regression_tests;
