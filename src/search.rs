use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::process::Command;
use thiserror::Error;

use crate::data::{EmailSummary, folder_from_url, unix_to_iso8601};
use crate::graph;

#[derive(Error, Debug)]
pub enum SearchError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Spotlight error: {0}")]
    Spotlight(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
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
    envelope_conn: &Connection,
    query: &SearchQuery,
) -> SearchResult<Vec<EmailSummary>> {
    let mut conditions = vec!["1=1".to_string()];
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![];
    let mut param_idx = 1;

    if let Some(ref sender) = query.sender {
        conditions.push(format!(
            "(a.address LIKE ?{pi} OR a.comment LIKE ?{pi})",
            pi = param_idx
        ));
        params.push(Box::new(format!("%{sender}%")));
        param_idx += 1;
    }

    if let Some(ref subject) = query.subject {
        conditions.push(format!("sub.subject LIKE ?{param_idx}"));
        params.push(Box::new(format!("%{subject}%")));
        param_idx += 1;
    }

    if let Some(ref date_from) = query.date_from
        && let Some(unix_ts) = iso8601_to_unix(date_from)
    {
        conditions.push(format!("m.date_sent >= ?{param_idx}"));
        params.push(Box::new(unix_ts));
        param_idx += 1;
    }

    if let Some(ref date_to) = query.date_to
        && let Some(unix_ts) = iso8601_to_unix(date_to)
    {
        conditions.push(format!("m.date_sent <= ?{param_idx}"));
        params.push(Box::new(unix_ts));
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

    let mut stmt = envelope_conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(&params), |row| {
        let rowid: i64 = row.get(0)?;
        let message_id: String = row.get(1)?;
        let sender_name: String = row.get(2)?;
        let sender_address: String = row.get(3)?;
        let subject: String = row.get(4)?;
        let date_sent: i64 = row.get(5)?;
        let read: i32 = row.get(6)?;
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
            label: None,
            sender_context: None,
        })
    })?;

    Ok(rows.filter_map(|r| r.ok()).collect())
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
    envelope_conn: &Connection,
    query: &SearchQuery,
    overlay_conn: Option<&Connection>,
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
                let mut stmt = envelope_conn.prepare(&sql)?;
                let params: Vec<Box<dyn rusqlite::types::ToSql>> = spotlight_rowids
                    .iter()
                    .map(|&id| Box::new(id) as Box<dyn rusqlite::types::ToSql>)
                    .collect();
                let rows = stmt.query_map(rusqlite::params_from_iter(&params), |row| {
                    let rowid: i64 = row.get(0)?;
                    let message_id: String = row.get(1)?;
                    let sender_name: String = row.get(2)?;
                    let sender_address: String = row.get(3)?;
                    let subject: String = row.get(4)?;
                    let date_sent: i64 = row.get(5)?;
                    let read: i32 = row.get(6)?;
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
                        label: None,
                        sender_context: None,
                    })
                })?;
                metadata_results = rows.filter_map(|r| r.ok()).collect();
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
    if let Some(ov_conn) = overlay_conn {
        let mut seen: std::collections::HashMap<String, Option<graph::SenderContext>> =
            std::collections::HashMap::new();
        for email in &mut metadata_results {
            if email.sender_address.is_empty() {
                continue;
            }
            let ctx = seen.entry(email.sender_address.clone()).or_insert_with(|| {
                graph::get_sender_context(ov_conn, &email.sender_address)
                    .ok()
                    .flatten()
            });
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
mod tests {
    use super::*;

    /// Create a mock Envelope Index matching V10 normalized schema.
    fn mock_envelope_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE mailboxes (ROWID INTEGER PRIMARY KEY, url TEXT COLLATE BINARY);
             CREATE TABLE subjects (ROWID INTEGER PRIMARY KEY, subject TEXT);
             CREATE TABLE addresses (ROWID INTEGER PRIMARY KEY, address TEXT, comment TEXT);
             CREATE TABLE message_global_data (ROWID INTEGER PRIMARY KEY, message_id INTEGER, message_id_header TEXT);
             CREATE TABLE messages (
                ROWID INTEGER PRIMARY KEY, message_id INTEGER DEFAULT 0, global_message_id INTEGER,
                subject_prefix TEXT, sender INTEGER, subject INTEGER,
                date_sent INTEGER, read INTEGER DEFAULT 0, flagged INTEGER DEFAULT 0,
                deleted INTEGER DEFAULT 0, mailbox INTEGER
             );
             INSERT INTO mailboxes VALUES (1, 'ews://test-uuid/Inbox');
             INSERT INTO subjects VALUES (1, 'Project Update');
             INSERT INTO subjects VALUES (2, 'Meeting Tomorrow');
             INSERT INTO subjects VALUES (3, 'Invoice #123');
             INSERT INTO addresses VALUES (1, 'alice@example.com', 'Alice Smith');
             INSERT INTO addresses VALUES (2, 'bob@example.com', 'Bob Jones');
             INSERT INTO message_global_data VALUES (1, 1, 'msg1@test');
             INSERT INTO message_global_data VALUES (2, 2, 'msg2@test');
             INSERT INTO message_global_data VALUES (3, 3, 'msg3@test');
             INSERT INTO messages VALUES (1, 0, 1, '', 1, 1, 1704067200, 0, 0, 0, 1);
             INSERT INTO messages VALUES (2, 0, 2, '', 2, 2, 1704067300, 1, 0, 0, 1);
             INSERT INTO messages VALUES (3, 0, 3, '', 1, 3, 1704067400, 0, 0, 0, 1);",
        ).unwrap();
        conn
    }

    #[test]
    fn test_search_by_sender() {
        let conn = mock_envelope_db();
        let query = SearchQuery {
            sender: Some("alice".to_string()),
            ..Default::default()
        };
        let results = search_metadata(&conn, &query).unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|e| e.sender_address.contains("alice")));
    }

    #[test]
    fn test_search_by_sender_name() {
        let conn = mock_envelope_db();
        let query = SearchQuery {
            sender: Some("Bob".to_string()),
            ..Default::default()
        };
        let results = search_metadata(&conn, &query).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].sender_name, "Bob Jones");
    }

    #[test]
    fn test_search_by_subject() {
        let conn = mock_envelope_db();
        let query = SearchQuery {
            subject: Some("Meeting".to_string()),
            ..Default::default()
        };
        let results = search_metadata(&conn, &query).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].subject, "Meeting Tomorrow");
    }

    #[test]
    fn test_search_by_date_range() {
        let conn = mock_envelope_db();
        // 1704067300 = 2024-01-01T00:01:40Z
        let query = SearchQuery {
            date_from: Some("2024-01-01T00:01:00+00:00".to_string()),
            ..Default::default()
        };
        let results = search_metadata(&conn, &query).unwrap();
        // Should get msg2 (1704067300) and msg3 (1704067400)
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_search_combined_filters() {
        let conn = mock_envelope_db();
        let query = SearchQuery {
            sender: Some("alice".to_string()),
            subject: Some("Invoice".to_string()),
            ..Default::default()
        };
        let results = search_metadata(&conn, &query).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].subject, "Invoice #123");
    }

    #[test]
    fn test_search_no_results() {
        let conn = mock_envelope_db();
        let query = SearchQuery {
            sender: Some("nonexistent@nowhere.com".to_string()),
            ..Default::default()
        };
        let results = search_metadata(&conn, &query).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_rowid_from_emlx_path() {
        assert_eq!(
            rowid_from_emlx_path("/Users/me/Library/Mail/V10/blah/12345.emlx"),
            Some(12345)
        );
        assert_eq!(
            rowid_from_emlx_path("/path/to/67890.partial.emlx"),
            Some(67890)
        );
        assert_eq!(rowid_from_emlx_path("/path/to/notanumber.emlx"), None);
    }

    #[test]
    fn test_iso8601_to_unix() {
        let unix = iso8601_to_unix("2024-01-01T00:00:00+00:00").unwrap();
        assert_eq!(unix, 1704067200);
    }

    #[test]
    fn test_search_results_shape_matches_list() {
        let conn = mock_envelope_db();
        let query = SearchQuery::default();
        let results = search_metadata(&conn, &query).unwrap();
        for email in &results {
            assert!(email.id > 0);
            assert!(!email.date.is_empty());
        }
    }
}
