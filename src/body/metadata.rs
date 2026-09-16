use super::*;
pub(super) struct EmailMetadata {
    id: i64,
    pub(super) message_id: String,
    from: String,
    date: String,
    subject: String,
    conversation_id: Option<i64>,
}

impl EmailMetadata {
    pub(super) fn into_detail(self, cached: CachedBody) -> EmailDetail {
        EmailDetail {
            id: self.id,
            message_id: self.message_id,
            from: self.from,
            to: cached.to,
            cc: cached.cc,
            date: self.date,
            subject: self.subject,
            body_text: cached.body_text,
            body_format: cached.body_format,
            conversation_id: self.conversation_id,
        }
    }
}

pub(super) fn email_metadata(conn: &Store, rowid: i64) -> BodyResult<EmailMetadata> {
    conn.one(
        "SELECT COALESCE(mgd.message_id_header, ''),
            COALESCE(a.comment, '') || ' <' || COALESCE(a.address, '') || '>',
            COALESCE(m.subject_prefix, '') || COALESCE(sub.subject, ''),
            COALESCE(m.date_sent, 0), COALESCE(m.conversation_id, 0)
         FROM messages m
         JOIN subjects sub ON m.subject = sub.ROWID
         JOIN addresses a ON m.sender = a.ROWID
         LEFT JOIN message_global_data mgd ON mgd.ROWID = m.global_message_id
         WHERE m.ROWID = ?1",
        params![rowid],
        |row| {
            let conv: i64 = row.get(4)?;
            Ok(EmailMetadata {
                id: rowid,
                message_id: row.get::<String>(0)?,
                from: row.get::<String>(1)?,
                subject: row.get::<String>(2)?,
                date: crate::data::unix_to_iso8601(row.get::<i64>(3)?),
                conversation_id: (conv != 0).then_some(conv),
            })
        },
    )
    .map_err(BodyError::Db)?
    .ok_or(BodyError::EmailFileNotFound(rowid))
}
