use chrono::Utc;
use rusqlite::Connection;
use serde::Serialize;
use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::db;

#[derive(Error, Debug)]
pub enum BodyError {
    #[error("Database error: {0}")]
    Db(#[from] db::DbError),
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Email file not found for rowid {0}")]
    EmailFileNotFound(i64),
    #[error("Parse error: {0}")]
    Parse(String),
}

pub type BodyResult<T> = Result<T, BodyError>;

#[derive(Debug, Clone, Serialize)]
pub struct EmailDetail {
    pub id: i64,
    pub message_id: String,
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub date: String,
    pub subject: String,
    pub body_text: String,
    pub body_format: String, // "plain" or "markdown"
}

#[derive(Debug, Clone, Serialize)]
pub struct CachedBody {
    pub body_text: String,
    pub body_format: String,
}

/// Get cached body from overlay DB, if available.
pub fn get_cached_body(conn: &Connection, rowid: i64) -> BodyResult<Option<CachedBody>> {
    let mut stmt =
        conn.prepare("SELECT body_text, body_format FROM cached_bodies WHERE rowid = ?1")?;
    let result = stmt
        .query_row([rowid], |row| {
            Ok(CachedBody {
                body_text: row.get(0)?,
                body_format: row.get(1)?,
            })
        })
        .ok();
    Ok(result)
}

/// Store a parsed body in the overlay DB cache.
pub fn cache_body(
    conn: &Connection,
    rowid: i64,
    message_id: &str,
    body_text: &str,
    body_format: &str,
) -> BodyResult<()> {
    db::ensure_identity(conn, rowid, message_id)?;
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO cached_bodies (rowid, body_text, body_format, cached_at) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(rowid) DO UPDATE SET body_text = excluded.body_text, body_format = excluded.body_format, cached_at = excluded.cached_at",
        rusqlite::params![rowid, body_text, body_format, now],
    )?;
    Ok(())
}

/// Parse raw email content (RFC 2822) and extract the body.
/// Returns (body_text, body_format).
pub fn parse_email_body(raw: &[u8]) -> BodyResult<(String, String)> {
    let parsed = mailparse::parse_mail(raw).map_err(|e| BodyError::Parse(e.to_string()))?;

    // Try to find a plain text part first
    if let Some(plain) = find_part(&parsed, "text/plain") {
        return Ok((plain, "plain".to_string()));
    }

    // Fall back to HTML, convert to markdown-ish text
    if let Some(html) = find_part(&parsed, "text/html") {
        let text = html2text::from_read(html.as_bytes(), 80)
            .map_err(|e| BodyError::Parse(e.to_string()))?;
        return Ok((text, "markdown".to_string()));
    }

    // Single-part email
    let body = parsed
        .get_body()
        .map_err(|e| BodyError::Parse(e.to_string()))?;
    let content_type = parsed.ctype.mimetype.to_lowercase();

    if content_type.contains("html") {
        let text = html2text::from_read(body.as_bytes(), 80)
            .map_err(|e| BodyError::Parse(e.to_string()))?;
        Ok((text, "markdown".to_string()))
    } else {
        Ok((body, "plain".to_string()))
    }
}

/// Recursively search MIME parts for a specific content type.
fn find_part(mail: &mailparse::ParsedMail, target_type: &str) -> Option<String> {
    if mail.subparts.is_empty() {
        if mail.ctype.mimetype.to_lowercase() == target_type {
            return mail.get_body().ok();
        }
        return None;
    }

    for part in &mail.subparts {
        if let Some(body) = find_part(part, target_type) {
            return Some(body);
        }
    }
    None
}

/// Find the on-disk .emlx file for a given email.
/// Apple Mail stores messages as .emlx files under ~/Library/Mail/V*/
pub fn find_email_file(rowid: i64) -> BodyResult<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let mail_dir = PathBuf::from(&home).join("Library/Mail");

    // Search for the .emlx file matching the rowid
    // Apple Mail names files as <rowid>.emlx or <rowid>.partial.emlx
    let filename = format!("{rowid}.emlx");

    fn search_dir(dir: &Path, filename: &str) -> Option<PathBuf> {
        let entries = std::fs::read_dir(dir).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(found) = search_dir(&path, filename) {
                    return Some(found);
                }
            } else if path.file_name().and_then(|f| f.to_str()) == Some(filename) {
                return Some(path);
            }
        }
        None
    }

    search_dir(&mail_dir, &filename).ok_or(BodyError::EmailFileNotFound(rowid))
}

/// Parse an .emlx file. Apple .emlx format has a byte count on the first line,
/// followed by the RFC 2822 message, then Apple metadata XML.
pub fn parse_emlx(raw: &[u8]) -> BodyResult<Vec<u8>> {
    // First line is the byte count of the message
    let raw_str = String::from_utf8_lossy(raw);
    let first_newline = raw_str
        .find('\n')
        .ok_or_else(|| BodyError::Parse("Invalid emlx: no newline found".into()))?;

    let byte_count: usize = raw_str[..first_newline]
        .trim()
        .parse()
        .map_err(|_| BodyError::Parse("Invalid emlx: first line not a byte count".into()))?;

    let message_start = first_newline + 1;
    let message_end = (message_start + byte_count).min(raw.len());

    Ok(raw[message_start..message_end].to_vec())
}

/// Read and parse an email body, using cache if available.
pub fn read_email_body(
    overlay_conn: &Connection,
    envelope_conn: &Connection,
    rowid: i64,
) -> BodyResult<EmailDetail> {
    // Get metadata from envelope index
    let (message_id, from, _to_raw, subject, date_sent): (String, String, String, String, f64) =
        envelope_conn
            .query_row(
                "SELECT COALESCE(message_id, ''), COALESCE(sender, ''),
                        COALESCE(subject, ''), COALESCE(subject, ''), COALESCE(date_sent, 0)
                 FROM messages WHERE ROWID = ?1",
                [rowid],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        String::new(),
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .map_err(BodyError::Sqlite)?;

    let date = crate::data::nsdate_to_iso8601(date_sent);

    // Check cache first
    if let Some(cached) = get_cached_body(overlay_conn, rowid)? {
        return Ok(EmailDetail {
            id: rowid,
            message_id,
            from,
            to: vec![],
            cc: vec![],
            date,
            subject,
            body_text: cached.body_text,
            body_format: cached.body_format,
        });
    }

    // Find and parse the .emlx file
    let emlx_path = find_email_file(rowid)?;
    let raw = std::fs::read(&emlx_path)?;
    let message = parse_emlx(&raw)?;
    let (body_text, body_format) = parse_email_body(&message)?;

    // Parse headers for to/cc
    let parsed = mailparse::parse_mail(&message).map_err(|e| BodyError::Parse(e.to_string()))?;
    let to: Vec<String> = parsed
        .headers
        .iter()
        .filter(|h| h.get_key().eq_ignore_ascii_case("to"))
        .map(|h| h.get_value())
        .collect();
    let cc: Vec<String> = parsed
        .headers
        .iter()
        .filter(|h| h.get_key().eq_ignore_ascii_case("cc"))
        .map(|h| h.get_value())
        .collect();

    // Cache the body
    cache_body(overlay_conn, rowid, &message_id, &body_text, &body_format)?;

    Ok(EmailDetail {
        id: rowid,
        message_id,
        from,
        to,
        cc,
        date,
        subject,
        body_text,
        body_format,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_overlay_db_memory;

    #[test]
    fn test_cache_and_retrieve_body() {
        let conn = open_overlay_db_memory().unwrap();
        cache_body(&conn, 1, "msg@test", "Hello world", "plain").unwrap();

        let cached = get_cached_body(&conn, 1).unwrap().unwrap();
        assert_eq!(cached.body_text, "Hello world");
        assert_eq!(cached.body_format, "plain");
    }

    #[test]
    fn test_cache_miss() {
        let conn = open_overlay_db_memory().unwrap();
        let cached = get_cached_body(&conn, 999).unwrap();
        assert!(cached.is_none());
    }

    #[test]
    fn test_cache_upsert() {
        let conn = open_overlay_db_memory().unwrap();
        cache_body(&conn, 1, "msg@test", "Old body", "plain").unwrap();
        cache_body(&conn, 1, "msg@test", "New body", "markdown").unwrap();

        let cached = get_cached_body(&conn, 1).unwrap().unwrap();
        assert_eq!(cached.body_text, "New body");
        assert_eq!(cached.body_format, "markdown");
    }

    #[test]
    fn test_parse_plain_email() {
        let raw = b"From: test@example.com\r\nSubject: Test\r\nContent-Type: text/plain\r\n\r\nHello, world!";
        let (body, format) = parse_email_body(raw).unwrap();
        assert_eq!(format, "plain");
        assert!(body.contains("Hello, world!"));
    }

    #[test]
    fn test_parse_html_email() {
        let raw = b"From: test@example.com\r\nSubject: Test\r\nContent-Type: text/html\r\n\r\n<html><body><h1>Hello</h1><p>World</p></body></html>";
        let (body, format) = parse_email_body(raw).unwrap();
        assert_eq!(format, "markdown");
        assert!(body.contains("Hello"));
        assert!(body.contains("World"));
    }

    #[test]
    fn test_parse_emlx() {
        let emlx = b"15\nFrom: a@b\r\nX: y\n<?xml version=\"1.0\"?><plist></plist>";
        let message = parse_emlx(emlx).unwrap();
        assert_eq!(message, b"From: a@b\r\nX: y");
    }

    #[test]
    fn test_parse_emlx_invalid() {
        let result = parse_emlx(b"notanumber\nstuff");
        assert!(result.is_err());
    }

    #[test]
    fn test_second_read_from_cache() {
        let conn = open_overlay_db_memory().unwrap();
        // Pre-populate cache
        cache_body(&conn, 42, "msg42@test", "Cached content", "plain").unwrap();

        // Verify it returns from cache
        let cached = get_cached_body(&conn, 42).unwrap().unwrap();
        assert_eq!(cached.body_text, "Cached content");
    }

    #[test]
    fn test_body_persists_across_connections() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");

        {
            let conn = crate::db::open_overlay_db(&path).unwrap();
            cache_body(&conn, 1, "msg@persist", "Persistent body", "markdown").unwrap();
        }
        {
            let conn = crate::db::open_overlay_db(&path).unwrap();
            let cached = get_cached_body(&conn, 1).unwrap().unwrap();
            assert_eq!(cached.body_text, "Persistent body");
        }
    }
}
