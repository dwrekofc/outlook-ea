use chrono::DateTime;
use rusqlite::Connection;
use serde::Serialize;
use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::{db, graph, labels};

#[derive(Error, Debug)]
pub enum DataError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Envelope Index not found at {0}")]
    EnvelopeNotFound(PathBuf),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type DataResult<T> = Result<T, DataError>;

#[derive(Debug, Clone, Serialize)]
pub struct EmailSummary {
    pub id: i64,
    pub message_id: String,
    pub sender_name: String,
    pub sender_address: String,
    pub subject: String,
    pub date: String,
    pub is_read: bool,
    pub folder: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sender_context: Option<graph::SenderContext>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListResponse {
    pub emails: Vec<EmailSummary>,
    pub total_count: usize,
    pub page: usize,
    pub page_size: usize,
}

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
pub fn open_envelope_index(path: &Path) -> DataResult<Connection> {
    let conn = Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    Ok(conn)
}

/// Build the folder WHERE clause for inbox queries.
/// Uses case-insensitive matching since mailbox URLs have COLLATE BINARY.
fn inbox_where(folder: Option<&str>) -> (String, Vec<Box<dyn rusqlite::types::ToSql>>) {
    if let Some(f) = folder {
        (
            "WHERE mb.url LIKE ?1".to_string(),
            vec![Box::new(format!("%{f}%"))],
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
    envelope_conn: &Connection,
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
        envelope_conn.query_row(&count_sql, [], |r| r.get(0))?
    } else {
        envelope_conn.query_row(&count_sql, rusqlite::params_from_iter(&params), |r| {
            r.get(0)
        })?
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
                COALESCE(mb.url, '') as folder_url
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

    let mut all_params: Vec<Box<dyn rusqlite::types::ToSql>> = params;
    all_params.push(Box::new(page_size as i64));
    all_params.push(Box::new(offset as i64));

    let mut stmt = envelope_conn.prepare(&query_sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(&all_params), |row| {
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

    let emails: Vec<EmailSummary> = rows.filter_map(|r| r.ok()).collect();

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
pub fn list_emails_filtered(
    envelope_conn: &Connection,
    overlay_conn: &Connection,
    folder: Option<&str>,
    page: usize,
    page_size: usize,
    label_filter: Option<u8>,
    untriaged: bool,
) -> DataResult<ListResponse> {
    let needs_label_filter = label_filter.is_some() || untriaged;

    // When filtering by label/untriaged, fetch all emails (no SQL pagination)
    // so we can filter correctly before paginating
    let (fetch_page, fetch_size) = if needs_label_filter {
        (0, usize::MAX)
    } else {
        (page, page_size)
    };

    let mut result = list_emails(envelope_conn, folder, fetch_page, fetch_size)?;

    // Join labels from overlay DB
    let label_map = labels::get_all_labels(overlay_conn).unwrap_or_default();
    for email in &mut result.emails {
        email.label = label_map.get(&email.id).copied();
        let _ = db::ensure_identity(overlay_conn, email.id, &email.message_id);
        // Attach sender context from graph if available
        if let Ok(Some(ctx)) = graph::get_sender_context(overlay_conn, &email.sender_address) {
            email.sender_context = Some(ctx);
        }
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

/// Parse "Name <address>" or bare address formats.
pub fn parse_sender(raw: &str) -> (String, String) {
    if let Some(lt) = raw.find('<')
        && let Some(gt) = raw.find('>')
    {
        let name = raw[..lt].trim().trim_matches('"').to_string();
        let addr = raw[lt + 1..gt].trim().to_string();
        return (name, addr);
    }
    // Bare address
    (String::new(), raw.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_overlay_db_memory;

    /// Create a mock Envelope Index matching Apple Mail V10's normalized schema.
    fn mock_envelope(n: usize) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE mailboxes (ROWID INTEGER PRIMARY KEY, url TEXT COLLATE BINARY);
             CREATE TABLE subjects (ROWID INTEGER PRIMARY KEY, subject TEXT);
             CREATE TABLE addresses (ROWID INTEGER PRIMARY KEY, address TEXT, comment TEXT);
             CREATE TABLE message_global_data (ROWID INTEGER PRIMARY KEY, message_id INTEGER, message_id_header TEXT);
             CREATE TABLE messages (
                ROWID INTEGER PRIMARY KEY,
                message_id INTEGER DEFAULT 0,
                global_message_id INTEGER,
                subject_prefix TEXT,
                sender INTEGER,
                subject INTEGER,
                date_sent INTEGER,
                read INTEGER DEFAULT 0,
                flagged INTEGER DEFAULT 0,
                deleted INTEGER DEFAULT 0,
                mailbox INTEGER
             );
             INSERT INTO mailboxes VALUES (1, 'ews://test-uuid/Inbox');",
        )
        .unwrap();
        for i in 1..=n {
            // Insert address
            conn.execute(
                "INSERT INTO addresses VALUES (?1, ?2, ?3)",
                rusqlite::params![i as i64, format!("user{i}@test.com"), format!("User {i}")],
            )
            .unwrap();
            // Insert subject
            conn.execute(
                "INSERT INTO subjects VALUES (?1, ?2)",
                rusqlite::params![i as i64, format!("Subject {i}")],
            )
            .unwrap();
            // Insert message_global_data
            conn.execute(
                "INSERT INTO message_global_data VALUES (?1, ?2, ?3)",
                rusqlite::params![i as i64, i as i64, format!("msg{i}@test")],
            )
            .unwrap();
            // Insert message
            conn.execute(
                "INSERT INTO messages (ROWID, message_id, global_message_id, subject_prefix, sender, subject, date_sent, read, flagged, deleted, mailbox)
                 VALUES (?1, 0, ?2, '', ?3, ?4, ?5, 0, 0, 0, 1)",
                rusqlite::params![
                    i as i64,
                    i as i64,
                    i as i64,
                    i as i64,
                    (i as i64) * 100,
                ],
            )
            .unwrap();
        }
        conn
    }

    #[test]
    fn test_label_filter_finds_emails_beyond_first_page() {
        let envelope = mock_envelope(10);
        let overlay = open_overlay_db_memory().unwrap();

        labels::assign_label(&overlay, 3, "msg3@test", 1).unwrap();

        let result = list_emails_filtered(&envelope, &overlay, None, 0, 5, Some(1), false).unwrap();
        assert_eq!(result.total_count, 1);
        assert_eq!(result.emails.len(), 1);
        assert_eq!(result.emails[0].id, 3);
    }

    #[test]
    fn test_untriaged_filter_correct_pagination() {
        let envelope = mock_envelope(5);
        let overlay = open_overlay_db_memory().unwrap();

        labels::assign_label(&overlay, 5, "msg5@test", 1).unwrap();
        labels::assign_label(&overlay, 3, "msg3@test", 2).unwrap();

        let page0 = list_emails_filtered(&envelope, &overlay, None, 0, 2, None, true).unwrap();
        assert_eq!(page0.total_count, 3);
        assert_eq!(page0.emails.len(), 2);

        let page1 = list_emails_filtered(&envelope, &overlay, None, 1, 2, None, true).unwrap();
        assert_eq!(page1.total_count, 3);
        assert_eq!(page1.emails.len(), 1);
    }

    #[test]
    fn test_no_filter_uses_sql_pagination() {
        let envelope = mock_envelope(5);
        let overlay = open_overlay_db_memory().unwrap();

        let result = list_emails_filtered(&envelope, &overlay, None, 0, 3, None, false).unwrap();
        assert_eq!(result.total_count, 5);
        assert_eq!(result.emails.len(), 3);
    }

    #[test]
    fn test_parse_sender_with_name() {
        let (name, addr) = parse_sender("John Doe <john@example.com>");
        assert_eq!(name, "John Doe");
        assert_eq!(addr, "john@example.com");
    }

    #[test]
    fn test_parse_sender_quoted_name() {
        let (name, addr) = parse_sender("\"Jane Doe\" <jane@example.com>");
        assert_eq!(name, "Jane Doe");
        assert_eq!(addr, "jane@example.com");
    }

    #[test]
    fn test_parse_sender_bare_address() {
        let (name, addr) = parse_sender("user@example.com");
        assert_eq!(name, "");
        assert_eq!(addr, "user@example.com");
    }

    #[test]
    fn test_unix_to_iso8601() {
        // 2024-01-01 00:00:00 UTC = Unix 1704067200
        let result = unix_to_iso8601(1_704_067_200);
        assert!(result.starts_with("2024-01-01T00:00:00"));
    }

    #[test]
    fn test_unix_to_iso8601_zero() {
        let result = unix_to_iso8601(0);
        assert!(result.starts_with("1970-01-01T00:00:00"));
    }

    #[test]
    fn test_list_emails_on_mock_db() {
        let conn = mock_envelope(2);

        let result = list_emails(&conn, None, 0, 10).unwrap();
        assert_eq!(result.total_count, 2);
        assert_eq!(result.emails.len(), 2);
        // Sorted by date desc — email 2 has date_sent=200, email 1 has date_sent=100
        assert_eq!(result.emails[0].sender_name, "User 2");
        assert_eq!(result.emails[1].sender_name, "User 1");
    }

    #[test]
    fn test_list_emails_pagination() {
        let conn = mock_envelope(3);

        let page0 = list_emails(&conn, None, 0, 2).unwrap();
        assert_eq!(page0.total_count, 3);
        assert_eq!(page0.emails.len(), 2);
        assert_eq!(page0.emails[0].subject, "Subject 3");

        let page1 = list_emails(&conn, None, 1, 2).unwrap();
        assert_eq!(page1.emails.len(), 1);
        assert_eq!(page1.emails[0].subject, "Subject 1");
    }

    #[test]
    fn test_list_emails_folder_filter() {
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
             INSERT INTO mailboxes VALUES (2, 'ews://test-uuid/Sent');
             INSERT INTO subjects VALUES (1, 'Inbox msg');
             INSERT INTO subjects VALUES (2, 'Sent msg');
             INSERT INTO addresses VALUES (1, 'a@t', 'A');
             INSERT INTO addresses VALUES (2, 'b@t', 'B');
             INSERT INTO message_global_data VALUES (1, 1, 'a@test');
             INSERT INTO message_global_data VALUES (2, 2, 'b@test');
             INSERT INTO messages VALUES (1, 0, 1, '', 1, 1, 100, 0, 0, 0, 1);
             INSERT INTO messages VALUES (2, 0, 2, '', 2, 2, 200, 0, 0, 0, 2);",
        )
        .unwrap();

        let inbox = list_emails(&conn, None, 0, 10).unwrap();
        assert_eq!(inbox.total_count, 1);
        assert_eq!(inbox.emails[0].subject, "Inbox msg");

        let sent = list_emails(&conn, Some("Sent"), 0, 10).unwrap();
        assert_eq!(sent.total_count, 1);
        assert_eq!(sent.emails[0].subject, "Sent msg");
    }

    #[test]
    fn test_list_emails_message_id_from_global_data() {
        let conn = mock_envelope(1);
        let result = list_emails(&conn, None, 0, 10).unwrap();
        assert_eq!(result.emails[0].message_id, "msg1@test");
    }

    #[test]
    fn test_list_emails_subject_prefix() {
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
             INSERT INTO subjects VALUES (1, 'Hello');
             INSERT INTO addresses VALUES (1, 'a@t', 'A');
             INSERT INTO message_global_data VALUES (1, 1, 'a@test');
             INSERT INTO messages VALUES (1, 0, 1, 'Re: ', 1, 1, 100, 0, 0, 0, 1);",
        )
        .unwrap();

        let result = list_emails(&conn, None, 0, 10).unwrap();
        assert_eq!(result.emails[0].subject, "Re: Hello");
    }
}
