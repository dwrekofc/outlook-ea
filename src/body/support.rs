use std::path::{Path, PathBuf};

use crate::body::{BodyError, BodyResult};

pub fn serialize_list(items: &[String]) -> String {
    serde_json::to_string(items).unwrap_or_default()
}

pub fn deserialize_list(raw: &str) -> Vec<String> {
    if raw.is_empty() {
        return vec![];
    }
    serde_json::from_str(raw).unwrap_or_default()
}

pub fn html_to_text(html: &str) -> BodyResult<(String, String)> {
    let text =
        html2text::from_read(html.as_bytes(), 80).map_err(|e| BodyError::Parse(e.to_string()))?;
    Ok((crate::body::clean_html_text(&text), "markdown".to_string()))
}

pub fn find_part(mail: &mailparse::ParsedMail, target_type: &str) -> Option<String> {
    if mail.subparts.is_empty() {
        if mail.ctype.mimetype.to_lowercase() == target_type {
            return mail.get_body().ok();
        }
        return None;
    }
    mail.subparts
        .iter()
        .find_map(|part| find_part(part, target_type))
}

pub fn header_values(parsed: &mailparse::ParsedMail, key: &str) -> Vec<String> {
    parsed
        .headers
        .iter()
        .filter(|header| header.get_key().eq_ignore_ascii_case(key))
        .map(|header| header.get_value())
        .collect()
}

pub fn inbox_search_roots(mail_dir: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for v_entry in std::fs::read_dir(mail_dir).into_iter().flatten().flatten() {
        let v_path = v_entry.path();
        let Some(name) = v_path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !v_path.is_dir() || !name.starts_with('V') {
            continue;
        }
        for acct_entry in std::fs::read_dir(&v_path).into_iter().flatten().flatten() {
            let inbox = acct_entry.path().join("Inbox.mbox");
            if inbox.is_dir() {
                roots.push(inbox);
            }
        }
    }
    roots
}

pub fn spotlight_email_file(search_dir: &Path, rowid: i64) -> Option<PathBuf> {
    let output = std::process::Command::new("mdfind")
        .args([
            "-onlyin",
            search_dir.to_str().unwrap_or("."),
            &format!("kMDItemFSName == '{rowid}.emlx' || kMDItemFSName == '{rowid}.partial.emlx'"),
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(PathBuf::from)
        .find(|path| path.exists())
}

pub fn search_messages_dirs(dir: &Path, filenames: &[String]) -> Option<PathBuf> {
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let dirname = path.file_name()?.to_str()?;
        if dirname == "Messages" {
            if let Some(candidate) = filenames
                .iter()
                .map(|filename| path.join(filename))
                .find(|candidate| candidate.exists())
            {
                return Some(candidate);
            }
        } else if dirname != "Attachments"
            && let Some(found) = search_messages_dirs(&path, filenames)
        {
            return Some(found);
        }
    }
    None
}

pub fn is_box_drawing_line(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty()
        && trimmed.chars().all(|c| {
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
