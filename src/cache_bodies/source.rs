use super::{Args, Message};
use crate::db::Store;
use anyhow::Result;
use libsql::params;
use std::collections::HashSet;

pub(super) fn messages(envelope: &Store, args: &Args) -> Result<Vec<Message>> {
    let since = args
        .since
        .map(|d| d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp())
        .unwrap_or(i64::MIN);
    envelope.all(
        "SELECT m.ROWID, COALESCE(g.message_id_header, ''), COALESCE(m.date_sent, 0)
         FROM messages m JOIN mailboxes mb ON mb.ROWID=m.mailbox
         LEFT JOIN message_global_data g ON g.ROWID=m.global_message_id
         WHERE (?1 OR mb.url LIKE '%/Inbox') AND COALESCE(m.date_sent, 0)>=?2
         ORDER BY m.date_sent DESC, m.ROWID DESC",
        params![args.all, since],
        |row| {
            Ok(Message {
                id: row.get(0)?,
                message_id: row.get(1)?,
                sent: row.get(2)?,
            })
        },
    )
}
pub(super) fn existing(store: &Store) -> Result<HashSet<String>> {
    Ok(store
        .all(
            "SELECT i.message_id FROM mail_identities i JOIN mail_bodies b ON b.identity_id=i.id",
            (),
            |row| Ok(row.get::<String>(0)?),
        )?
        .into_iter()
        .collect())
}
