use crate::graph;
use serde::Serialize;

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
    /// Apple Mail conversation/thread id linking inbox ↔ sent ↔ archive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<u8>,
    /// Set only when --needs-reply is requested: true when the conversation's
    /// latest message is inbound AND the user is a direct (To) recipient.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub needs_reply: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sender_context: Option<graph::SenderContext>,
}

/// One message in a conversation thread, with send direction.
#[derive(Debug, Clone, Serialize)]
pub struct ThreadMessage {
    pub id: i64,
    pub date: String,
    pub from: String,
    pub subject: String,
    pub folder: String,
    /// "out" if sent by the user (Sent folder or self-address), else "in".
    pub direction: String,
    pub is_read: bool,
}

/// A full conversation thread plus a derived status line.
#[derive(Debug, Clone, Serialize)]
pub struct ThreadResponse {
    pub conversation_id: i64,
    pub message_count: usize,
    pub sent_count: usize,
    /// "awaiting_your_reply" | "you_replied_last" | "no_reply_needed"
    pub status: String,
    pub messages: Vec<ThreadMessage>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListResponse {
    pub emails: Vec<EmailSummary>,
    pub total_count: usize,
    pub page: usize,
    pub page_size: usize,
}
