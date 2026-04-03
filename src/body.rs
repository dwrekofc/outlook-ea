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
    pub to: Vec<String>,
    pub cc: Vec<String>,
}

/// Get cached body from overlay DB, if available.
pub fn get_cached_body(conn: &Connection, rowid: i64) -> BodyResult<Option<CachedBody>> {
    let mut stmt = conn.prepare(
        "SELECT body_text, body_format, cached_to, cached_cc FROM cached_bodies WHERE rowid = ?1",
    )?;
    let result = stmt
        .query_row([rowid], |row| {
            let to_raw: String = row.get(2)?;
            let cc_raw: String = row.get(3)?;
            Ok(CachedBody {
                body_text: row.get(0)?,
                body_format: row.get(1)?,
                to: deserialize_list(&to_raw),
                cc: deserialize_list(&cc_raw),
            })
        })
        .ok();
    Ok(result)
}

fn serialize_list(items: &[String]) -> String {
    serde_json::to_string(items).unwrap_or_default()
}

fn deserialize_list(raw: &str) -> Vec<String> {
    if raw.is_empty() {
        return vec![];
    }
    serde_json::from_str(raw).unwrap_or_default()
}

/// Store a parsed body in the overlay DB cache.
pub fn cache_body(
    conn: &Connection,
    rowid: i64,
    message_id: &str,
    body_text: &str,
    body_format: &str,
    to: &[String],
    cc: &[String],
) -> BodyResult<()> {
    db::ensure_identity(conn, rowid, message_id)?;
    let now = Utc::now().to_rfc3339();
    let to_json = serialize_list(to);
    let cc_json = serialize_list(cc);
    conn.execute(
        "INSERT INTO cached_bodies (rowid, body_text, body_format, cached_at, cached_to, cached_cc) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(rowid) DO UPDATE SET body_text = excluded.body_text, body_format = excluded.body_format, cached_at = excluded.cached_at, cached_to = excluded.cached_to, cached_cc = excluded.cached_cc",
        rusqlite::params![rowid, body_text, body_format, now, to_json, cc_json],
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
        return Ok((clean_html_text(&text), "markdown".to_string()));
    }

    // Single-part email
    let body = parsed
        .get_body()
        .map_err(|e| BodyError::Parse(e.to_string()))?;
    let content_type = parsed.ctype.mimetype.to_lowercase();

    if content_type.contains("html") {
        let text = html2text::from_read(body.as_bytes(), 80)
            .map_err(|e| BodyError::Parse(e.to_string()))?;
        Ok((clean_html_text(&text), "markdown".to_string()))
    } else {
        Ok((body, "plain".to_string()))
    }
}

/// Clean up noisy html2text output: collapse box-drawing separator lines,
/// remove tracking pixel artifacts, and reduce excessive blank lines.
pub fn clean_html_text(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut cleaned: Vec<String> = Vec::with_capacity(lines.len());

    for line in &lines {
        let trimmed = line.trim();

        // Remove lines that are only [Image] or [image] (tracking pixels / spacer gifs)
        if trimmed.eq_ignore_ascii_case("[image]") {
            continue;
        }

        // Skip separator-only lines entirely — they come from HTML table layout
        if is_box_drawing_line(line) {
            continue;
        }

        // Trim trailing whitespace
        cleaned.push(line.trim_end().to_string());
    }

    // Collapse 3+ consecutive blank lines into 1
    let mut final_lines: Vec<String> = Vec::with_capacity(cleaned.len());
    let mut consecutive_blanks = 0u32;

    for line in &cleaned {
        if line.trim().is_empty() {
            consecutive_blanks += 1;
            if consecutive_blanks <= 1 {
                final_lines.push(line.clone());
            }
        } else {
            consecutive_blanks = 0;
            final_lines.push(line.clone());
        }
    }

    // Trim leading/trailing blank lines
    while final_lines.first().is_some_and(|l| l.trim().is_empty()) {
        final_lines.remove(0);
    }
    while final_lines.last().is_some_and(|l| l.trim().is_empty()) {
        final_lines.pop();
    }

    final_lines.join("\n")
}

/// Returns true if the line consists only of box-drawing characters and whitespace.
fn is_box_drawing_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    trimmed.chars().all(|c| {
        matches!(
            c,
            '─' | '│'
                | '┬'
                | '┴'
                | '┼'
                | '═'
                | '║'
                | '╔'
                | '╗'
                | '╚'
                | '╝'
                | '╠'
                | '╣'
                | '╦'
                | '╩'
                | '╬'
                | '┌'
                | '┐'
                | '└'
                | '┘'
                | '├'
                | '┤'
                | '━'
                | '┃'
                | '╭'
                | '╮'
                | '╯'
                | '╰'
                | '▔'
                | '▁'
                | '▏'
                | '▕'
                | ' '
        )
    })
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

/// Collect Inbox.mbox search roots for all accounts under ~/Library/Mail/V*.
/// Returns paths like `~/Library/Mail/V10/<UUID>/Inbox.mbox/` for each account.
fn inbox_search_roots(mail_dir: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for v_entry in std::fs::read_dir(mail_dir).into_iter().flatten().flatten() {
        let v_path = v_entry.path();
        if !v_path.is_dir() {
            continue;
        }
        let v_name = match v_path.file_name().and_then(|n| n.to_str()) {
            Some(n) if n.starts_with('V') => n.to_string(),
            _ => continue,
        };
        let _ = v_name; // used only for the filter above
        // Each subdirectory of V* is an account UUID
        for acct_entry in std::fs::read_dir(&v_path).into_iter().flatten().flatten() {
            let acct_path = acct_entry.path();
            if acct_path.is_dir() {
                let inbox = acct_path.join("Inbox.mbox");
                if inbox.is_dir() {
                    roots.push(inbox);
                }
            }
        }
    }
    roots
}

/// Find the on-disk .emlx file for a given email.
/// Uses Spotlight (mdfind) for fast lookup, falling back to targeted directory search.
/// Apple Mail stores messages as `<rowid>.emlx` or `<rowid>.partial.emlx`.
///
/// When `inbox_only` is true, restricts the search to Inbox.mbox directories
/// (one per account), which is much faster and avoids hitting archive/trash.
pub fn find_email_file(rowid: i64, inbox_only: bool) -> BodyResult<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let mail_dir = PathBuf::from(&home).join("Library/Mail");

    let search_dirs: Vec<PathBuf> = if inbox_only {
        let roots = inbox_search_roots(&mail_dir);
        if roots.is_empty() {
            // No Inbox.mbox found — fall back to full mail_dir
            vec![mail_dir.clone()]
        } else {
            roots
        }
    } else {
        vec![mail_dir.clone()]
    };

    // Try Spotlight first — instant lookup
    for search_dir in &search_dirs {
        if let Ok(output) = std::process::Command::new("mdfind")
            .args([
                "-onlyin",
                search_dir.to_str().unwrap_or("."),
                &format!(
                    "kMDItemFSName == '{rowid}.emlx' || kMDItemFSName == '{rowid}.partial.emlx'"
                ),
            ])
            .output()
            && output.status.success()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some(path) = stdout.lines().next() {
                let p = PathBuf::from(path);
                if p.exists() {
                    return Ok(p);
                }
            }
        }
    }

    // Fallback: search Messages directories directly
    let filenames = [format!("{rowid}.emlx"), format!("{rowid}.partial.emlx")];
    if inbox_only {
        // Search only within the Inbox.mbox directories
        for search_dir in &search_dirs {
            if let Some(found) = search_messages_dirs(search_dir, &filenames) {
                return Ok(found);
            }
        }
    } else {
        // Search all V* directories
        for entry in std::fs::read_dir(&mail_dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir()
                && let Some(name) = path.file_name().and_then(|n| n.to_str())
                && name.starts_with('V')
                && let Some(found) = search_messages_dirs(&path, &filenames)
            {
                return Ok(found);
            }
        }
    }

    Err(BodyError::EmailFileNotFound(rowid))
}

/// Search only "Messages" subdirectories within a mail tree for the target file.
/// Much faster than a full recursive walk since it skips Attachments, etc.
fn search_messages_dirs(dir: &Path, filenames: &[String]) -> Option<PathBuf> {
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let dirname = path.file_name()?.to_str()?;
            if dirname == "Messages" {
                // Check for target files in this Messages directory
                for filename in filenames {
                    let candidate = path.join(filename);
                    if candidate.exists() {
                        return Some(candidate);
                    }
                }
            } else if dirname != "Attachments"
                && let Some(found) = search_messages_dirs(&path, filenames)
            {
                return Some(found);
            }
        }
    }
    None
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
/// When `inbox_only` is true, file search is restricted to Inbox.mbox directories.
pub fn read_email_body(
    overlay_conn: &Connection,
    envelope_conn: &Connection,
    rowid: i64,
    inbox_only: bool,
) -> BodyResult<EmailDetail> {
    // Get metadata from envelope index using V10 normalized schema
    let (message_id, from, subject, date_sent): (String, String, String, i64) = envelope_conn
        .query_row(
            "SELECT COALESCE(mgd.message_id_header, ''),
                    COALESCE(a.comment, '') || ' <' || COALESCE(a.address, '') || '>',
                    COALESCE(m.subject_prefix, '') || COALESCE(sub.subject, ''),
                    COALESCE(m.date_sent, 0)
             FROM messages m
             JOIN subjects sub ON m.subject = sub.ROWID
             JOIN addresses a ON m.sender = a.ROWID
             LEFT JOIN message_global_data mgd ON mgd.ROWID = m.global_message_id
             WHERE m.ROWID = ?1",
            [rowid],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(BodyError::Sqlite)?;

    let date = crate::data::unix_to_iso8601(date_sent);

    // Check cache first
    if let Some(cached) = get_cached_body(overlay_conn, rowid)? {
        return Ok(EmailDetail {
            id: rowid,
            message_id,
            from,
            to: cached.to,
            cc: cached.cc,
            date,
            subject,
            body_text: cached.body_text,
            body_format: cached.body_format,
        });
    }

    // Find and parse the .emlx file
    let emlx_path = find_email_file(rowid, inbox_only)?;
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

    // Cache the body (including to/cc from headers)
    cache_body(
        overlay_conn,
        rowid,
        &message_id,
        &body_text,
        &body_format,
        &to,
        &cc,
    )?;

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
        let to = vec!["alice@test.com".to_string()];
        let cc = vec!["bob@test.com".to_string()];
        cache_body(&conn, 1, "msg@test", "Hello world", "plain", &to, &cc).unwrap();

        let cached = get_cached_body(&conn, 1).unwrap().unwrap();
        assert_eq!(cached.body_text, "Hello world");
        assert_eq!(cached.body_format, "plain");
        assert_eq!(cached.to, vec!["alice@test.com"]);
        assert_eq!(cached.cc, vec!["bob@test.com"]);
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
        cache_body(&conn, 1, "msg@test", "Old body", "plain", &[], &[]).unwrap();
        cache_body(&conn, 1, "msg@test", "New body", "markdown", &[], &[]).unwrap();

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
        cache_body(&conn, 42, "msg42@test", "Cached content", "plain", &[], &[]).unwrap();

        // Verify it returns from cache
        let cached = get_cached_body(&conn, 42).unwrap().unwrap();
        assert_eq!(cached.body_text, "Cached content");
    }

    #[test]
    fn test_clean_html_text_removes_separators() {
        let input = "Hello\n────\n────\n────\n────\nWorld";
        let result = clean_html_text(input);
        assert_eq!(result, "Hello\nWorld");
    }

    #[test]
    fn test_clean_html_text_removes_image_tracking() {
        let input = "Hello\n[Image]\nWorld\n[image]\nEnd\n[Image: logo]\nKeep";
        let result = clean_html_text(input);
        assert!(result.contains("Hello"));
        assert!(result.contains("World"));
        assert!(!result.contains("[Image]\n"));
        assert!(!result.contains("[image]\n"));
        assert!(result.contains("[Image: logo]"));
    }

    #[test]
    fn test_clean_html_text_collapses_blank_lines() {
        let input = "A\n\n\n\n\nB";
        let result = clean_html_text(input);
        assert_eq!(result, "A\n\nB");
    }

    #[test]
    fn test_clean_html_text_trims_trailing_whitespace() {
        let input = "Hello   \nWorld  ";
        let result = clean_html_text(input);
        assert_eq!(result, "Hello\nWorld");
    }

    #[test]
    fn test_body_persists_across_connections() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");

        {
            let conn = crate::db::open_overlay_db(&path).unwrap();
            cache_body(
                &conn,
                1,
                "msg@persist",
                "Persistent body",
                "markdown",
                &[],
                &[],
            )
            .unwrap();
        }
        {
            let conn = crate::db::open_overlay_db(&path).unwrap();
            let cached = get_cached_body(&conn, 1).unwrap().unwrap();
            assert_eq!(cached.body_text, "Persistent body");
        }
    }
}
