use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::process::Command;
use thiserror::Error;

use crate::data::{EmailSummary, nsdate_to_iso8601, parse_sender};

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

/// Search emails using metadata filters against the Envelope Index.
pub fn search_metadata(
    envelope_conn: &Connection,
    query: &SearchQuery,
) -> SearchResult<Vec<EmailSummary>> {
    let mut conditions = vec!["1=1".to_string()];
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![];
    let mut param_idx = 1;

    if let Some(ref sender) = query.sender {
        conditions.push(format!("m.sender LIKE ?{param_idx}"));
        params.push(Box::new(format!("%{sender}%")));
        param_idx += 1;
    }

    if let Some(ref subject) = query.subject {
        conditions.push(format!("m.subject LIKE ?{param_idx}"));
        params.push(Box::new(format!("%{subject}%")));
        param_idx += 1;
    }

    if let Some(ref date_from) = query.date_from {
        // Convert ISO 8601 to NSDate
        if let Some(nsdate) = iso8601_to_nsdate(date_from) {
            conditions.push(format!("m.date_sent >= ?{param_idx}"));
            params.push(Box::new(nsdate));
            param_idx += 1;
        }
    }

    if let Some(ref date_to) = query.date_to
        && let Some(nsdate) = iso8601_to_nsdate(date_to)
    {
        conditions.push(format!("m.date_sent <= ?{param_idx}"));
        params.push(Box::new(nsdate));
        param_idx += 1;
    }
    let _ = param_idx;

    let where_clause = conditions.join(" AND ");
    let sql = format!(
        "SELECT m.ROWID, COALESCE(m.message_id, ''), COALESCE(m.sender, ''),
                COALESCE(m.subject, ''), COALESCE(m.date_sent, 0),
                COALESCE(m.read, 0), COALESCE(mb.url, '')
         FROM messages m
         JOIN mailboxes mb ON m.mailbox = mb.ROWID
         WHERE {where_clause}
         ORDER BY m.date_sent DESC"
    );

    let mut stmt = envelope_conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(&params), |row| {
        let rowid: i64 = row.get(0)?;
        let message_id: String = row.get(1)?;
        let sender_raw: String = row.get(2)?;
        let subject: String = row.get(3)?;
        let date_sent: f64 = row.get(4)?;
        let read: i32 = row.get(5)?;
        let folder_url: String = row.get(6)?;

        let (name, addr) = parse_sender(&sender_raw);
        let date_str = nsdate_to_iso8601(date_sent);
        let folder_name = folder_url
            .rsplit('/')
            .find(|s| !s.is_empty())
            .unwrap_or("Unknown")
            .to_string();

        Ok(EmailSummary {
            id: rowid,
            message_id,
            sender_name: name,
            sender_address: addr,
            subject,
            date: date_str,
            is_read: read != 0,
            folder: folder_name,
            label: None,
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
pub fn search_emails(
    envelope_conn: &Connection,
    query: &SearchQuery,
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
                    "SELECT m.ROWID, COALESCE(m.message_id, ''), COALESCE(m.sender, ''),
                            COALESCE(m.subject, ''), COALESCE(m.date_sent, 0),
                            COALESCE(m.read, 0), COALESCE(mb.url, '')
                     FROM messages m
                     JOIN mailboxes mb ON m.mailbox = mb.ROWID
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
                    let sender_raw: String = row.get(2)?;
                    let subject: String = row.get(3)?;
                    let date_sent: f64 = row.get(4)?;
                    let read: i32 = row.get(5)?;
                    let folder_url: String = row.get(6)?;

                    let (name, addr) = parse_sender(&sender_raw);
                    let date_str = nsdate_to_iso8601(date_sent);
                    let folder_name = folder_url
                        .rsplit('/')
                        .find(|s| !s.is_empty())
                        .unwrap_or("Unknown")
                        .to_string();

                    Ok(EmailSummary {
                        id: rowid,
                        message_id,
                        sender_name: name,
                        sender_address: addr,
                        subject,
                        date: date_str,
                        is_read: read != 0,
                        folder: folder_name,
                        label: None,
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

    let total_count = metadata_results.len();
    Ok(SearchResponse {
        emails: metadata_results,
        total_count,
    })
}

/// Convert ISO 8601 date string to NSDate (seconds since 2001-01-01).
fn iso8601_to_nsdate(iso: &str) -> Option<f64> {
    const NS_EPOCH_OFFSET: i64 = 978_307_200;
    let dt = chrono::DateTime::parse_from_rfc3339(iso).ok()?;
    Some((dt.timestamp() - NS_EPOCH_OFFSET) as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_envelope_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE mailboxes (ROWID INTEGER PRIMARY KEY, url TEXT);
             CREATE TABLE messages (
                ROWID INTEGER PRIMARY KEY, message_id TEXT, sender TEXT,
                subject TEXT, date_sent REAL, read INTEGER, flagged INTEGER, mailbox INTEGER
             );
             INSERT INTO mailboxes VALUES (1, 'imap://user@server/INBOX');
             INSERT INTO messages VALUES (1, 'msg1@test', 'Alice Smith <alice@example.com>', 'Project Update', 725760000.0, 0, 0, 1);
             INSERT INTO messages VALUES (2, 'msg2@test', 'Bob Jones <bob@example.com>', 'Meeting Tomorrow', 725760100.0, 1, 0, 1);
             INSERT INTO messages VALUES (3, 'msg3@test', 'Alice Smith <alice@example.com>', 'Invoice #123', 725760200.0, 0, 0, 1);",
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
        // NSDate 725760000 = 2024-01-01T00:00:00Z
        // NSDate 725760100 = 2024-01-01T00:01:40Z
        let query = SearchQuery {
            date_from: Some("2024-01-01T00:01:00+00:00".to_string()),
            ..Default::default()
        };
        let results = search_metadata(&conn, &query).unwrap();
        // Should get msg2 and msg3 (date_sent >= 725846460)
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
    fn test_iso8601_to_nsdate() {
        let nsdate = iso8601_to_nsdate("2024-01-01T00:00:00+00:00").unwrap();
        assert_eq!(nsdate, 725760000.0);
    }

    #[test]
    fn test_search_results_shape_matches_list() {
        let conn = mock_envelope_db();
        let query = SearchQuery::default();
        let results = search_metadata(&conn, &query).unwrap();
        // All results should have the EmailSummary shape
        for email in &results {
            assert!(email.id > 0);
            assert!(!email.date.is_empty());
        }
    }
}
