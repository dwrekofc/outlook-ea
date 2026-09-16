mod metadata;
use metadata::email_metadata;
mod support;
use chrono::Utc;
use libsql::params;
use serde::Serialize;
use std::path::PathBuf;
use thiserror::Error;

use crate::body::support::{
    deserialize_list, find_part, header_values, html_to_text, inbox_search_roots,
    is_box_drawing_line, search_messages_dirs, serialize_list, spotlight_email_file,
};
use crate::db::{self, Store};

#[derive(Error, Debug)]
pub enum BodyError {
    #[error("Database error: {0}")]
    Db(#[from] db::DbError),
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
    pub body_format: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CachedBody {
    pub body_text: String,
    pub body_format: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
}

pub fn get_cached_body(
    store: &Store,
    rowid: i64,
    message_id: &str,
) -> BodyResult<Option<CachedBody>> {
    let Some(identity_id) = identity_id_for_body(store, rowid, message_id)? else {
        return Ok(None);
    };
    Ok(store.one(
        "SELECT CAST(body_text AS BLOB), body_format, cached_to, cached_cc
         FROM mail_bodies WHERE identity_id = ?1",
        params![identity_id],
        |row| {
            let to_raw: String = row.get(2)?;
            let cc_raw: String = row.get(3)?;
            Ok(CachedBody {
                body_text: String::from_utf8(row.get::<Vec<u8>>(0)?)?,
                body_format: row.get(1)?,
                to: deserialize_list(&to_raw),
                cc: deserialize_list(&cc_raw),
            })
        },
    )?)
}

pub fn cache_body(
    store: &Store,
    rowid: i64,
    message_id: &str,
    body_text: &str,
    body_format: &str,
    to: &[String],
    cc: &[String],
) -> BodyResult<()> {
    store.transaction(|store| {
        let identity_id = db::ensure_identity(store, rowid, message_id)?;
        store.execute(
            "INSERT INTO mail_bodies
            (identity_id, body_text, body_format, cached_at, cached_to, cached_cc)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(identity_id) DO UPDATE SET
            body_text = excluded.body_text,
            body_format = excluded.body_format,
            cached_at = excluded.cached_at,
            cached_to = excluded.cached_to,
            cached_cc = excluded.cached_cc",
            params![
                identity_id,
                body_text,
                body_format,
                Utc::now().to_rfc3339(),
                serialize_list(to),
                serialize_list(cc)
            ],
        )?;
        Ok(())
    })
}

pub fn parse_email_body(raw: &[u8]) -> BodyResult<(String, String)> {
    let parsed = mailparse::parse_mail(raw).map_err(|e| BodyError::Parse(e.to_string()))?;
    if let Some(plain) = find_part(&parsed, "text/plain") {
        return Ok((plain, "plain".to_string()));
    }
    if let Some(html) = find_part(&parsed, "text/html") {
        return html_to_text(&html);
    }
    let body = parsed
        .get_body()
        .map_err(|e| BodyError::Parse(e.to_string()))?;
    if parsed.ctype.mimetype.to_lowercase().contains("html") {
        html_to_text(&body)
    } else {
        Ok((body, "plain".to_string()))
    }
}

pub fn clean_html_text(text: &str) -> String {
    let mut cleaned = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.eq_ignore_ascii_case("[image]") || is_box_drawing_line(line) {
            continue;
        }
        cleaned.push(line.trim_end().to_string());
    }
    let mut result = Vec::new();
    let mut blanks = 0;
    for line in cleaned {
        if line.trim().is_empty() {
            blanks += 1;
            if blanks <= 1 {
                result.push(line);
            }
        } else {
            blanks = 0;
            result.push(line);
        }
    }
    while result.first().is_some_and(|line| line.trim().is_empty()) {
        result.remove(0);
    }
    while result.last().is_some_and(|line| line.trim().is_empty()) {
        result.pop();
    }
    result.join("\n")
}

pub fn find_email_file(rowid: i64, inbox_only: bool) -> BodyResult<PathBuf> {
    let mail_dir =
        PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into())).join("Library/Mail");
    let roots = if inbox_only {
        let inboxes = inbox_search_roots(&mail_dir);
        if inboxes.is_empty() {
            vec![mail_dir.clone()]
        } else {
            inboxes
        }
    } else {
        vec![mail_dir.clone()]
    };
    for dir in &roots {
        if let Some(path) = spotlight_email_file(dir, rowid) {
            return Ok(path);
        }
    }
    let filenames = [format!("{rowid}.emlx"), format!("{rowid}.partial.emlx")];
    for dir in roots {
        if let Some(found) = search_messages_dirs(&dir, &filenames) {
            return Ok(found);
        }
    }
    Err(BodyError::EmailFileNotFound(rowid))
}

pub fn parse_emlx(raw: &[u8]) -> BodyResult<Vec<u8>> {
    let raw_str = String::from_utf8_lossy(raw);
    let first_newline = raw_str
        .find('\n')
        .ok_or_else(|| BodyError::Parse("Invalid emlx: no newline found".into()))?;
    let byte_count: usize = raw_str[..first_newline]
        .trim()
        .parse()
        .map_err(|_| BodyError::Parse("Invalid emlx: first line not a byte count".into()))?;
    let start = first_newline + 1;
    Ok(raw[start..(start + byte_count).min(raw.len())].to_vec())
}

pub fn read_email_body(
    store: &Store,
    envelope_conn: &Store,
    rowid: i64,
    inbox_only: bool,
) -> BodyResult<EmailDetail> {
    let metadata = email_metadata(envelope_conn, rowid)?;
    if let Some(cached) = get_cached_body(store, rowid, &metadata.message_id)? {
        return Ok(metadata.into_detail(cached));
    }
    let cached = extract_email_body(rowid, inbox_only)?;
    let CachedBody {
        body_text,
        body_format,
        to,
        cc,
    } = cached;
    if !metadata.message_id.trim().is_empty() {
        cache_body(
            store,
            rowid,
            &metadata.message_id,
            &body_text,
            &body_format,
            &to,
            &cc,
        )?;
    }
    Ok(metadata.into_detail(CachedBody {
        body_text,
        body_format,
        to,
        cc,
    }))
}

/// Shared local extraction path for reads and automatic capture.
pub fn extract_email_body(rowid: i64, inbox_only: bool) -> BodyResult<CachedBody> {
    let message = parse_emlx(&std::fs::read(find_email_file(rowid, inbox_only)?)?)?;
    let (body_text, body_format) = parse_email_body(&message)?;
    let parsed = mailparse::parse_mail(&message).map_err(|e| BodyError::Parse(e.to_string()))?;
    let to = header_values(&parsed, "to");
    let cc = header_values(&parsed, "cc");
    Ok(CachedBody {
        body_text,
        body_format,
        to,
        cc,
    })
}

fn identity_id_for_body(store: &Store, _rowid: i64, message_id: &str) -> BodyResult<Option<i64>> {
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
mod regression_tests;

#[cfg(test)]
mod migration_tests;
