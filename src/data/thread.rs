use std::collections::HashSet;

use libsql::params;

use crate::data::{DataResult, ThreadMessage, ThreadResponse, folder_from_url, unix_to_iso8601};
use crate::db::Store;

/// A mailbox URL is an outgoing (Sent) folder if its path contains "Sent".
/// Matches both "Sent Items" (Exchange) and "Sent Messages" (IMAP).
pub(super) fn is_sent_url(url: &str) -> bool {
    url.to_lowercase().contains("/sent")
}

/// Collect the address ROWIDs the user sends *from*, self-identifying with no
/// hardcoded address. A Sent folder contains stray senders (Exchange stores
/// conversation copies, on-behalf sends, etc.), so counting by frequency and
/// keeping only senders ≥1% of the top sender cleanly isolates the account
/// owner's own address(es) (which dominate Sent) from that noise.
pub fn self_address_ids(conn: &Store) -> HashSet<i64> {
    let counts = conn
        .all(
            "SELECT m.sender, COUNT(*) c
         FROM messages m JOIN mailboxes mb ON m.mailbox = mb.ROWID
         WHERE mb.url LIKE '%/Sent%' OR mb.url LIKE '%/sent%'
         GROUP BY m.sender",
            (),
            |row| Ok((row.get::<i64>(0)?, row.get::<i64>(1)?)),
        )
        .unwrap_or_default();
    let max_count = counts.iter().map(|(_, c)| *c).max().unwrap_or(0);
    // Keep senders that are at least 1% of the top sender (and seen >1 time).
    let threshold = std::cmp::max(2, max_count / 100);
    counts
        .into_iter()
        .filter(|(_, c)| *c >= threshold)
        .map(|(id, _)| id)
        .collect()
}

/// Look up the conversation_id for a message ROWID (None if 0/missing).
pub fn conversation_id_for(conn: &Store, rowid: i64) -> Option<i64> {
    conn.one(
        "SELECT COALESCE(conversation_id, 0) FROM messages WHERE ROWID = ?1",
        params![rowid],
        |row| Ok(row.get::<i64>(0)?),
    )
    .ok()
    .flatten()
    .filter(|&c| c != 0)
}

/// A sender address that is automated / no-reply (never expects a human reply).
pub(super) fn is_automated_sender(addr: &str) -> bool {
    let a = addr.to_lowercase();
    let local = a.split('@').next().unwrap_or(&a);
    const NEEDLES: &[&str] = &[
        "no-reply",
        "noreply",
        "no_reply",
        "donotreply",
        "do_not_reply",
        "do.not.reply",
        "notification",
        "notifications",
        "notify",
        "mailer",
        "automated",
        "auto_",
        "auto-",
    ];
    if NEEDLES.iter().any(|n| local.contains(n)) {
        return true;
    }
    // Common automated/no-reply domains seen in this mailbox.
    const DOMAINS: &[&str] = &[
        "sharepointonline.com",
        "awardco.com",
        "equateplus.com",
        "vanguard.com",
        "airtable.com",
        "outlook.mail.microsoft",
        "accounts.google.com",
    ];
    DOMAINS.iter().any(|d| a.ends_with(d) || a.contains(d))
}

/// Determine whether an inbox message awaits the user's reply:
/// the conversation's latest message is inbound, from a real person (not an
/// automated/no-reply sender or calendar status notice), AND the user is a
/// direct (To, type=0) recipient — not merely CC'd.
pub fn compute_needs_reply(
    conn: &Store,
    rowid: i64,
    conv_id: Option<i64>,
    self_ids: &HashSet<i64>,
) -> bool {
    // Skip automated senders and calendar status notices — never reply-needed.
    if let Ok(Some((addr, subject))) = conn.one(
        "SELECT COALESCE(a.address,''), COALESCE(m.subject_prefix,'')||COALESCE(sub.subject,'')
         FROM messages m
         LEFT JOIN addresses a ON m.sender = a.ROWID
         LEFT JOIN subjects sub ON m.subject = sub.ROWID
         WHERE m.ROWID = ?1",
        params![rowid],
        |row| Ok((row.get::<String>(0)?, row.get::<String>(1)?)),
    ) && (is_automated_sender(&addr)
        || crate::rules::is_calendar_notice_subject(&subject)
        || subject
            .trim_start()
            .to_ascii_lowercase()
            .starts_with("automatic reply"))
    {
        return false;
    }

    // Must be a direct To recipient (type = 0) — not merely CC'd.
    let user_in_to = self_ids.iter().any(|&aid| {
        conn.one(
            "SELECT EXISTS(SELECT 1 FROM recipients WHERE message = ?1 AND type = 0 AND address = ?2)",
            params![rowid, aid],
            |row| Ok(row.get::<i64>(0)?),
        )
        .map(|value| value.unwrap_or(0))
        .map(|e| e != 0)
        .unwrap_or(false)
    });
    if !user_in_to {
        return false;
    }

    // Find the latest message in the conversation; outbound if Sent folder or self sender.
    let Some(conv) = conv_id else {
        // No conversation grouping: single inbound message addressed to user.
        return true;
    };
    let latest: Option<(String, i64)> = conn
        .one(
            "SELECT COALESCE(mb.url,''), m.sender
             FROM messages m JOIN mailboxes mb ON m.mailbox = mb.ROWID
             WHERE m.conversation_id = ?1
             ORDER BY m.date_received DESC LIMIT 1",
            params![conv],
            |row| Ok((row.get::<String>(0)?, row.get::<i64>(1)?)),
        )
        .ok()
        .flatten();
    match latest {
        Some((url, sender)) => !(is_sent_url(&url) || self_ids.contains(&sender)),
        None => true,
    }
}

/// Build a full conversation thread (all folders) ordered chronologically.
pub fn get_thread(conn: &Store, conversation_id: i64) -> DataResult<ThreadResponse> {
    let self_ids = self_address_ids(conn);
    let messages = conn.all(
        "SELECT m.ROWID,
                COALESCE(m.date_received, m.date_sent, 0),
                COALESCE(a.comment,'') || ' <' || COALESCE(a.address,'') || '>',
                COALESCE(m.subject_prefix,'') || COALESCE(sub.subject,''),
                COALESCE(mb.url,''),
                COALESCE(m.read,0),
                m.sender
         FROM messages m
         JOIN mailboxes mb ON m.mailbox = mb.ROWID
         LEFT JOIN subjects sub ON m.subject = sub.ROWID
         LEFT JOIN addresses a ON m.sender = a.ROWID
         WHERE m.conversation_id = ?1
         ORDER BY COALESCE(m.date_received, m.date_sent, 0) ASC",
        params![conversation_id],
        |row| {
            let id: i64 = row.get(0)?;
            let ts: i64 = row.get(1)?;
            let from: String = row.get(2)?;
            let subject: String = row.get(3)?;
            let url: String = row.get(4)?;
            let read: i64 = row.get(5)?;
            let sender: i64 = row.get(6)?;
            let outgoing = is_sent_url(&url) || self_ids.contains(&sender);
            Ok(ThreadMessage {
                id,
                date: unix_to_iso8601(ts),
                from,
                subject,
                folder: folder_from_url(&url),
                direction: if outgoing { "out" } else { "in" }.to_string(),
                is_read: read != 0,
            })
        },
    )?;
    let sent_count = messages.iter().filter(|m| m.direction == "out").count();
    // Pure ball-in-court status from thread structure. Whether a reply is
    // actually warranted (sender/recipient judgment) is the job of --needs-reply.
    let status = match messages.last() {
        Some(m) if m.direction == "out" => "you_replied_last",
        Some(_) => "awaiting_your_reply",
        None => "empty",
    }
    .to_string();
    Ok(ThreadResponse {
        conversation_id,
        message_count: messages.len(),
        sent_count,
        status,
        messages,
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
