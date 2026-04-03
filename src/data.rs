use chrono::DateTime;
use rusqlite::Connection;
use serde::Serialize;
use std::path::{Path, PathBuf};
use thiserror::Error;

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

/// List emails from the Envelope Index.
/// `folder` filters by mailbox URL pattern if provided.
/// Results sorted by date descending.
pub fn list_emails(
    envelope_conn: &Connection,
    folder: Option<&str>,
    page: usize,
    page_size: usize,
) -> DataResult<ListResponse> {
    // The Envelope Index schema has `messages` and `mailboxes` tables.
    // messages: ROWID, message_id, subject, sender, date_sent, date_received,
    //           read, flagged, mailbox
    // mailboxes: ROWID, url

    let (where_clause, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) =
        if let Some(f) = folder {
            (
                "WHERE mb.url LIKE ?1".to_string(),
                vec![Box::new(format!("%{f}%"))],
            )
        } else {
            // Default: INBOX
            ("WHERE mb.url LIKE '%INBOX%'".to_string(), vec![])
        };

    let count_sql = format!(
        "SELECT COUNT(*) FROM messages m JOIN mailboxes mb ON m.mailbox = mb.ROWID {where_clause}"
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
        "SELECT m.ROWID, COALESCE(m.message_id, ''), COALESCE(m.sender, ''),
                COALESCE(m.subject, ''), COALESCE(m.date_sent, 0),
                COALESCE(m.read, 0), COALESCE(mb.url, '')
         FROM messages m
         JOIN mailboxes mb ON m.mailbox = mb.ROWID
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
        let sender_raw: String = row.get(2)?;
        let subject: String = row.get(3)?;
        let date_sent: f64 = row.get(4)?;
        let read: i32 = row.get(5)?;
        let folder_url: String = row.get(6)?;

        // Parse sender into name + address
        let (name, addr) = parse_sender(&sender_raw);

        // Apple Mail stores dates as NSDate (seconds since 2001-01-01)
        let date_str = nsdate_to_iso8601(date_sent);

        // Extract folder name from URL
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

    let emails: Vec<EmailSummary> = rows.filter_map(|r| r.ok()).collect();

    Ok(ListResponse {
        emails,
        total_count,
        page,
        page_size,
    })
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

/// Convert Apple's NSDate (seconds since 2001-01-01 00:00:00 UTC) to ISO 8601.
pub fn nsdate_to_iso8601(nsdate: f64) -> String {
    // NSDate epoch: 2001-01-01 00:00:00 UTC = Unix epoch + 978307200
    const NS_EPOCH_OFFSET: i64 = 978_307_200;
    let unix_ts = nsdate as i64 + NS_EPOCH_OFFSET;
    DateTime::from_timestamp(unix_ts, 0)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_nsdate_to_iso8601() {
        // 2024-01-01 00:00:00 UTC = 725760000 seconds since NSDate epoch
        let result = nsdate_to_iso8601(725_760_000.0);
        assert!(result.starts_with("2024-01-01T00:00:00"));
    }

    #[test]
    fn test_nsdate_zero() {
        // NSDate 0 = 2001-01-01 00:00:00 UTC
        let result = nsdate_to_iso8601(0.0);
        assert!(result.starts_with("2001-01-01T00:00:00"));
    }

    #[test]
    fn test_list_emails_on_mock_db() {
        // Create a mock Envelope Index in memory
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE mailboxes (ROWID INTEGER PRIMARY KEY, url TEXT);
             CREATE TABLE messages (
                ROWID INTEGER PRIMARY KEY, message_id TEXT, sender TEXT,
                subject TEXT, date_sent REAL, read INTEGER, flagged INTEGER, mailbox INTEGER
             );
             INSERT INTO mailboxes VALUES (1, 'imap://user@server/INBOX');
             INSERT INTO messages VALUES (1, 'msg1@test', 'Alice <alice@test.com>', 'Hello', 725760000.0, 0, 0, 1);
             INSERT INTO messages VALUES (2, 'msg2@test', 'Bob <bob@test.com>', 'World', 725760100.0, 1, 0, 1);",
        ).unwrap();

        let result = list_emails(&conn, None, 0, 10).unwrap();
        assert_eq!(result.total_count, 2);
        assert_eq!(result.emails.len(), 2);
        // Sorted by date desc — Bob's email is newer
        assert_eq!(result.emails[0].sender_name, "Bob");
        assert_eq!(result.emails[1].sender_name, "Alice");
        assert!(result.emails[0].is_read);
        assert!(!result.emails[1].is_read);
    }

    #[test]
    fn test_list_emails_pagination() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE mailboxes (ROWID INTEGER PRIMARY KEY, url TEXT);
             CREATE TABLE messages (
                ROWID INTEGER PRIMARY KEY, message_id TEXT, sender TEXT,
                subject TEXT, date_sent REAL, read INTEGER, flagged INTEGER, mailbox INTEGER
             );
             INSERT INTO mailboxes VALUES (1, 'imap://user@server/INBOX');
             INSERT INTO messages VALUES (1, 'a@t', 'A <a@t>', 'S1', 100.0, 0, 0, 1);
             INSERT INTO messages VALUES (2, 'b@t', 'B <b@t>', 'S2', 200.0, 0, 0, 1);
             INSERT INTO messages VALUES (3, 'c@t', 'C <c@t>', 'S3', 300.0, 0, 0, 1);",
        )
        .unwrap();

        let page0 = list_emails(&conn, None, 0, 2).unwrap();
        assert_eq!(page0.total_count, 3);
        assert_eq!(page0.emails.len(), 2);
        assert_eq!(page0.emails[0].sender_name, "C");

        let page1 = list_emails(&conn, None, 1, 2).unwrap();
        assert_eq!(page1.emails.len(), 1);
        assert_eq!(page1.emails[0].sender_name, "A");
    }

    #[test]
    fn test_list_emails_folder_filter() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE mailboxes (ROWID INTEGER PRIMARY KEY, url TEXT);
             CREATE TABLE messages (
                ROWID INTEGER PRIMARY KEY, message_id TEXT, sender TEXT,
                subject TEXT, date_sent REAL, read INTEGER, flagged INTEGER, mailbox INTEGER
             );
             INSERT INTO mailboxes VALUES (1, 'imap://user@server/INBOX');
             INSERT INTO mailboxes VALUES (2, 'imap://user@server/Sent');
             INSERT INTO messages VALUES (1, 'a@t', 'A <a@t>', 'Inbox msg', 100.0, 0, 0, 1);
             INSERT INTO messages VALUES (2, 'b@t', 'B <b@t>', 'Sent msg', 200.0, 0, 0, 2);",
        )
        .unwrap();

        let inbox = list_emails(&conn, None, 0, 10).unwrap();
        assert_eq!(inbox.total_count, 1);
        assert_eq!(inbox.emails[0].subject, "Inbox msg");

        let sent = list_emails(&conn, Some("Sent"), 0, 10).unwrap();
        assert_eq!(sent.total_count, 1);
        assert_eq!(sent.emails[0].subject, "Sent msg");
    }
}
