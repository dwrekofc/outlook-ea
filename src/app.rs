mod preferences;
use crate::cli::{self, Cli, Commands};
use crate::{actions, body, data, db, graph, labels, search};

#[path = "app/graph.rs"]
mod app_graph;
#[path = "app/mail.rs"]
mod app_mail;

pub fn run(cli_args: Cli) -> String {
    match cli_args.command {
        Commands::List {
            folder,
            page,
            page_size,
            label,
            untriaged,
            needs_reply,
        } => cmd_list(folder, page, page_size, label, untriaged, needs_reply),
        Commands::Read { id, all_folders } => cmd_read(id, all_folders),
        Commands::Thread { id } => cmd_thread(id),
        Commands::Search {
            sender,
            subject,
            date_from,
            date_to,
            body,
        } => cmd_search(sender, subject, date_from, date_to, body),
        Commands::Label { id, label } => cmd_label(id, label),
        Commands::Delete { ids, yes, force } => cmd_action(ids, yes, force, "delete"),
        Commands::Archive { ids, yes, force } => cmd_action(ids, yes, force, "archive"),
        Commands::Flag { id, unflag } => cmd_flag(id, unflag),
        Commands::MarkRead { id, unread } => cmd_mark_read(id, unread),
        Commands::Triage { dry_run } => app_mail::cmd_triage(dry_run),
        Commands::Sync => cmd_sync(),
        Commands::Rules { action } => app_mail::cmd_rules(action),
        Commands::Graph { action } => app_graph::cmd_graph(action),
    }
}

pub(crate) fn open_store() -> Result<db::Store, String> {
    preferences::bootstrap().map_err(|err| err.to_string())?;
    db::Store::open_from_vault_config().map_err(|err| err.to_string())
}

pub(crate) fn open_envelope() -> Result<db::Store, String> {
    let path = data::find_envelope_index().map_err(|err| err.to_string())?;
    data::open_envelope_index(&path).map_err(|err| err.to_string())
}

pub(crate) fn get_message_id_header(conn: &db::Store, rowid: i64) -> String {
    conn.one(
        "SELECT COALESCE(mgd.message_id_header, '')
         FROM messages m
         LEFT JOIN message_global_data mgd ON mgd.ROWID = m.global_message_id
         WHERE m.ROWID = ?1",
        libsql::params![rowid],
        |row| Ok(row.get::<String>(0)?),
    )
    .unwrap_or_default()
    .unwrap_or_default()
}

fn cmd_list(
    folder: Option<String>,
    page: usize,
    page_size: usize,
    label_filter: Option<u8>,
    untriaged: bool,
    needs_reply: bool,
) -> String {
    let envelope = match open_envelope() {
        Ok(conn) => conn,
        Err(err) => return cli::error(&err, "ENVELOPE_ERROR"),
    };
    let store = match open_store() {
        Ok(conn) => conn,
        Err(err) => return cli::error(&err, "STORAGE_ERROR"),
    };
    match data::list_emails_filtered(
        &envelope,
        &store,
        folder.as_deref(),
        page,
        page_size,
        label_filter,
        untriaged,
        needs_reply,
    ) {
        Ok(result) => cli::success(&result),
        Err(err) => cli::error(&err.to_string(), "LIST_ERROR"),
    }
}

fn cmd_thread(id: i64) -> String {
    let envelope = match open_envelope() {
        Ok(conn) => conn,
        Err(err) => return cli::error(&err, "ENVELOPE_ERROR"),
    };
    let Some(conv) = data::conversation_id_for(&envelope, id) else {
        return cli::error(
            &format!("No conversation found for email {id}"),
            "THREAD_NOT_FOUND",
        );
    };
    match data::get_thread(&envelope, conv) {
        Ok(thread) => cli::success(&thread),
        Err(err) => cli::error(&err.to_string(), "THREAD_ERROR"),
    }
}

fn cmd_read(id: i64, all_folders: bool) -> String {
    let envelope = match open_envelope() {
        Ok(conn) => conn,
        Err(err) => return cli::error(&err, "ENVELOPE_ERROR"),
    };
    let store = match open_store() {
        Ok(conn) => conn,
        Err(err) => return cli::error(&err, "STORAGE_ERROR"),
    };
    match body::read_email_body(&store, &envelope, id, !all_folders) {
        Ok(detail) => cli::success(&detail),
        Err(err) => cli::error(&err.to_string(), "READ_ERROR"),
    }
}

fn cmd_search(
    sender: Option<String>,
    subject: Option<String>,
    date_from: Option<String>,
    date_to: Option<String>,
    body_text: Option<String>,
) -> String {
    let envelope = match open_envelope() {
        Ok(conn) => conn,
        Err(err) => return cli::error(&err, "ENVELOPE_ERROR"),
    };
    let store = match open_store() {
        Ok(conn) => conn,
        Err(err) => return cli::error(&err, "STORAGE_ERROR"),
    };
    let query = search::SearchQuery {
        sender,
        subject,
        date_from,
        date_to,
        body_text,
    };
    match search::search_emails(&envelope, &query, Some(&store)) {
        Ok(result) => cli::success(&result),
        Err(err) => cli::error(&err.to_string(), "SEARCH_ERROR"),
    }
}

fn cmd_label(id: i64, label: u8) -> String {
    let store = match open_store() {
        Ok(conn) => conn,
        Err(err) => return cli::error(&err, "STORAGE_ERROR"),
    };
    let message_id = match open_envelope() {
        Ok(env) => get_message_id_header(&env, id),
        Err(err) => return cli::error(&err, "ENVELOPE_ERROR"),
    };
    match labels::assign_label(&store, id, &message_id, label) {
        Ok(()) if label == 0 => cli::success(serde_json::json!({"cleared": true, "email_id": id})),
        Ok(()) => cli::success(serde_json::json!({
            "email_id": id,
            "label": label,
            "label_name": labels::label_name(label),
        })),
        Err(err) => cli::error(&err.to_string(), "LABEL_ERROR"),
    }
}

fn cmd_action(ids: Vec<i64>, yes: bool, force: bool, action: &str) -> String {
    if !yes {
        return cli::confirm(
            &format!(
                "{} {} email(s)?",
                if action == "delete" {
                    "Delete"
                } else {
                    "Archive"
                },
                ids.len()
            ),
            action,
            ids.len(),
        );
    }
    let store = match open_store() {
        Ok(conn) => conn,
        Err(err) => return cli::error(&err, "STORAGE_ERROR"),
    };
    let envelope = match open_envelope() {
        Ok(conn) => conn,
        Err(err) => return cli::error(&err, "ENVELOPE_ERROR"),
    };
    let vip_addresses = match graph::get_vip_emails(&store) {
        Ok(vips) if !vips.is_empty() => vips,
        Ok(_) => app_mail::fallback_vips(),
        Err(err) => return cli::error(&err.to_string(), "GRAPH_ERROR"),
    };
    let mut message_ids = Vec::new();
    let mut vip_message_ids = Vec::new();
    for id in ids {
        let message_id = get_message_id_header(&envelope, id);
        if app_mail::is_vip_message(&envelope, id, &vip_addresses) && !force {
            vip_message_ids.push(message_id.clone());
        }
        message_ids.push(message_id);
    }
    let result = match action {
        "delete" => actions::bulk_action(&message_ids, "delete", &vip_message_ids),
        _ => actions::bulk_action(&message_ids, "archive", &vip_message_ids),
    };
    match result {
        Ok(resp) => cli::success(&resp),
        Err(err) => cli::error(&err.to_string(), "ACTION_ERROR"),
    }
}

fn cmd_flag(id: i64, unflag: bool) -> String {
    let envelope = match open_envelope() {
        Ok(conn) => conn,
        Err(err) => return cli::error(&err, "ENVELOPE_ERROR"),
    };
    match actions::set_flag(&get_message_id_header(&envelope, id), !unflag) {
        Ok(()) => cli::success(serde_json::json!({"email_id": id, "flagged": !unflag})),
        Err(err) => cli::error(&err.to_string(), "ACTION_ERROR"),
    }
}

fn cmd_mark_read(id: i64, unread: bool) -> String {
    let envelope = match open_envelope() {
        Ok(conn) => conn,
        Err(err) => return cli::error(&err, "ENVELOPE_ERROR"),
    };
    match actions::set_read_status(&get_message_id_header(&envelope, id), !unread) {
        Ok(()) => cli::success(serde_json::json!({"email_id": id, "read": !unread})),
        Err(err) => cli::error(&err.to_string(), "ACTION_ERROR"),
    }
}

fn cmd_sync() -> String {
    let script = r#"tell application "Mail" to check for new mail"#;
    match std::process::Command::new("osascript")
        .args(["-e", script])
        .output()
    {
        Ok(output) if output.status.success() => {
            std::thread::sleep(std::time::Duration::from_secs(2));
            cli::success(serde_json::json!({"message": "Sync initiated"}))
        }
        Ok(output) => cli::error(
            &format!(
                "AppleScript error: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
            "SYNC_ERROR",
        ),
        Err(err) => cli::error(&format!("Failed to run osascript: {err}"), "SYNC_ERROR"),
    }
}
