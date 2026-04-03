use clap::Parser;
use mea::cli::{self, Cli, Commands, RulesAction};
use mea::{actions, body, data, db, labels, rules, search, triage};

fn main() {
    let cli_args = Cli::parse();
    let output = run(cli_args);
    println!("{output}");

    // Exit with non-zero code if the response indicates an error
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&output)
        && parsed.get("status").and_then(|s| s.as_str()) == Some("error")
    {
        std::process::exit(1);
    }
}

fn run(cli_args: Cli) -> String {
    match cli_args.command {
        Commands::List {
            folder,
            page,
            page_size,
            label,
            untriaged,
        } => cmd_list(folder, page, page_size, label, untriaged),
        Commands::Read { id } => cmd_read(id),
        Commands::Search {
            sender,
            subject,
            date_from,
            date_to,
            body,
        } => cmd_search(sender, subject, date_from, date_to, body),
        Commands::Label { id, label } => cmd_label(id, label),
        Commands::Delete { ids, yes } => cmd_delete(ids, yes),
        Commands::Archive { ids, yes } => cmd_archive(ids, yes),
        Commands::Flag { id, unflag } => cmd_flag(id, unflag),
        Commands::MarkRead { id, unread } => cmd_mark_read(id, unread),
        Commands::Triage { dry_run } => cmd_triage(dry_run),
        Commands::Rules { action } => cmd_rules(action),
    }
}

fn open_overlay() -> Result<rusqlite::Connection, String> {
    let path = db::default_overlay_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        // Bootstrap PATTERNS.md if it doesn't exist
        let patterns_path = parent.join("PATTERNS.md");
        if !patterns_path.exists() {
            let _ = std::fs::write(
                &patterns_path,
                "# Learned Triage Preferences\n\n\
                 This file records patterns observed during triage sessions.\n\
                 The skill wrapper updates it as the user makes consistent decisions.\n\n\
                 ## Sender Patterns\n\n\
                 <!-- e.g., \"always trash emails from noreply@marketing.com\" -->\n\n\
                 ## Subject Patterns\n\n\
                 <!-- e.g., \"label newsletters as Read Later\" -->\n\n\
                 ## Notes\n\n\
                 <!-- General triage preferences -->\n",
            );
        }
    }
    db::open_overlay_db(&path).map_err(|e| e.to_string())
}

fn open_envelope() -> Result<rusqlite::Connection, String> {
    let path = data::find_envelope_index().map_err(|e| e.to_string())?;
    data::open_envelope_index(&path).map_err(|e| e.to_string())
}

fn cmd_list(
    folder: Option<String>,
    page: usize,
    page_size: usize,
    label_filter: Option<u8>,
    untriaged: bool,
) -> String {
    let envelope_conn = match open_envelope() {
        Ok(c) => c,
        Err(e) => return cli::error(&e, "ENVELOPE_ERROR"),
    };
    let overlay_conn = match open_overlay() {
        Ok(c) => c,
        Err(e) => return cli::error(&e, "OVERLAY_ERROR"),
    };

    let result = match data::list_emails_filtered(
        &envelope_conn,
        &overlay_conn,
        folder.as_deref(),
        page,
        page_size,
        label_filter,
        untriaged,
    ) {
        Ok(r) => r,
        Err(e) => return cli::error(&e.to_string(), "LIST_ERROR"),
    };

    cli::success(&result)
}

fn cmd_read(id: i64) -> String {
    let envelope_conn = match open_envelope() {
        Ok(c) => c,
        Err(e) => return cli::error(&e, "ENVELOPE_ERROR"),
    };
    let overlay_conn = match open_overlay() {
        Ok(c) => c,
        Err(e) => return cli::error(&e, "OVERLAY_ERROR"),
    };

    match body::read_email_body(&overlay_conn, &envelope_conn, id) {
        Ok(detail) => cli::success(&detail),
        Err(e) => cli::error(&e.to_string(), "READ_ERROR"),
    }
}

fn cmd_search(
    sender: Option<String>,
    subject: Option<String>,
    date_from: Option<String>,
    date_to: Option<String>,
    body_text: Option<String>,
) -> String {
    let envelope_conn = match open_envelope() {
        Ok(c) => c,
        Err(e) => return cli::error(&e, "ENVELOPE_ERROR"),
    };

    let query = search::SearchQuery {
        sender,
        subject,
        date_from,
        date_to,
        body_text,
    };

    match search::search_emails(&envelope_conn, &query) {
        Ok(result) => cli::success(&result),
        Err(e) => cli::error(&e.to_string(), "SEARCH_ERROR"),
    }
}

fn cmd_label(id: i64, label: u8) -> String {
    let overlay_conn = match open_overlay() {
        Ok(c) => c,
        Err(e) => return cli::error(&e, "OVERLAY_ERROR"),
    };

    // Get message_id from envelope for identity mapping
    let message_id = match open_envelope() {
        Ok(env_conn) => env_conn
            .query_row(
                "SELECT COALESCE(message_id, '') FROM messages WHERE ROWID = ?1",
                [id],
                |r| r.get::<_, String>(0),
            )
            .unwrap_or_default(),
        Err(_) => String::new(),
    };

    match labels::assign_label(&overlay_conn, id, &message_id, label) {
        Ok(()) => {
            if label == 0 {
                cli::success(serde_json::json!({"cleared": true, "email_id": id}))
            } else {
                cli::success(serde_json::json!({
                    "email_id": id,
                    "label": label,
                    "label_name": labels::label_name(label),
                }))
            }
        }
        Err(e) => cli::error(&e.to_string(), "LABEL_ERROR"),
    }
}

fn cmd_delete(ids: Vec<i64>, yes: bool) -> String {
    if !yes {
        return cli::confirm(
            &format!("Delete {} email(s)? This moves them to Trash.", ids.len()),
            "delete",
            ids.len(),
        );
    }

    let rules_config = rules::load_rules(&rules::default_rules_path()).unwrap_or_default();
    let vip_addresses: Vec<String> = rules_config
        .vip_senders
        .iter()
        .map(|v| v.address.clone())
        .collect();

    // Resolve message IDs from envelope
    let envelope_conn = match open_envelope() {
        Ok(c) => c,
        Err(e) => return cli::error(&e, "ENVELOPE_ERROR"),
    };

    let mut message_ids = vec![];
    let mut vip_message_ids = vec![];

    for &id in &ids {
        let row = envelope_conn.query_row(
            "SELECT COALESCE(message_id, ''), COALESCE(sender, '') FROM messages WHERE ROWID = ?1",
            [id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        );
        if let Ok((msg_id, sender)) = row {
            let (_, addr) = data::parse_sender(&sender);
            if vip_addresses.iter().any(|v| v.eq_ignore_ascii_case(&addr)) {
                vip_message_ids.push(msg_id.clone());
            }
            message_ids.push(msg_id);
        }
    }

    match actions::bulk_action(&message_ids, "delete", &vip_message_ids, false) {
        Ok(resp) => cli::success(&resp),
        Err(e) => cli::error(&e.to_string(), "ACTION_ERROR"),
    }
}

fn cmd_archive(ids: Vec<i64>, yes: bool) -> String {
    if !yes {
        return cli::confirm(
            &format!("Archive {} email(s)?", ids.len()),
            "archive",
            ids.len(),
        );
    }

    let rules_config = rules::load_rules(&rules::default_rules_path()).unwrap_or_default();
    let vip_addresses: Vec<String> = rules_config
        .vip_senders
        .iter()
        .map(|v| v.address.clone())
        .collect();

    let envelope_conn = match open_envelope() {
        Ok(c) => c,
        Err(e) => return cli::error(&e, "ENVELOPE_ERROR"),
    };

    let mut message_ids = vec![];
    let mut vip_message_ids = vec![];

    for &id in &ids {
        let row = envelope_conn.query_row(
            "SELECT COALESCE(message_id, ''), COALESCE(sender, '') FROM messages WHERE ROWID = ?1",
            [id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        );
        if let Ok((msg_id, sender)) = row {
            let (_, addr) = data::parse_sender(&sender);
            if vip_addresses.iter().any(|v| v.eq_ignore_ascii_case(&addr)) {
                vip_message_ids.push(msg_id.clone());
            }
            message_ids.push(msg_id);
        }
    }

    match actions::bulk_action(&message_ids, "archive", &vip_message_ids, false) {
        Ok(resp) => cli::success(&resp),
        Err(e) => cli::error(&e.to_string(), "ACTION_ERROR"),
    }
}

fn cmd_flag(id: i64, unflag: bool) -> String {
    let envelope_conn = match open_envelope() {
        Ok(c) => c,
        Err(e) => return cli::error(&e, "ENVELOPE_ERROR"),
    };

    let msg_id: String = match envelope_conn.query_row(
        "SELECT COALESCE(message_id, '') FROM messages WHERE ROWID = ?1",
        [id],
        |r| r.get(0),
    ) {
        Ok(id) => id,
        Err(e) => return cli::error(&e.to_string(), "QUERY_ERROR"),
    };

    match actions::set_flag(&msg_id, !unflag) {
        Ok(()) => cli::success(serde_json::json!({
            "email_id": id,
            "flagged": !unflag,
        })),
        Err(e) => cli::error(&e.to_string(), "ACTION_ERROR"),
    }
}

fn cmd_mark_read(id: i64, unread: bool) -> String {
    let envelope_conn = match open_envelope() {
        Ok(c) => c,
        Err(e) => return cli::error(&e, "ENVELOPE_ERROR"),
    };

    let msg_id: String = match envelope_conn.query_row(
        "SELECT COALESCE(message_id, '') FROM messages WHERE ROWID = ?1",
        [id],
        |r| r.get(0),
    ) {
        Ok(id) => id,
        Err(e) => return cli::error(&e.to_string(), "QUERY_ERROR"),
    };

    match actions::set_read_status(&msg_id, !unread) {
        Ok(()) => cli::success(serde_json::json!({
            "email_id": id,
            "read": !unread,
        })),
        Err(e) => cli::error(&e.to_string(), "ACTION_ERROR"),
    }
}

fn cmd_triage(dry_run: bool) -> String {
    let envelope_conn = match open_envelope() {
        Ok(c) => c,
        Err(e) => return cli::error(&e, "ENVELOPE_ERROR"),
    };
    let overlay_conn = match open_overlay() {
        Ok(c) => c,
        Err(e) => return cli::error(&e, "OVERLAY_ERROR"),
    };

    // Load rules
    let config = match rules::load_rules(&rules::default_rules_path()) {
        Ok(c) => c,
        Err(e) => return cli::error(&e.to_string(), "RULES_ERROR"),
    };

    // Get all inbox emails
    let list = match data::list_emails(&envelope_conn, None, 0, 10000) {
        Ok(r) => r,
        Err(e) => return cli::error(&e.to_string(), "LIST_ERROR"),
    };

    // Filter to untriaged only
    let label_map = labels::get_all_labels(&overlay_conn).unwrap_or_default();
    let untriaged: Vec<_> = list
        .emails
        .into_iter()
        .filter(|e| !label_map.contains_key(&e.id))
        .collect();

    match triage::auto_triage(&overlay_conn, &config, &untriaged, dry_run) {
        Ok(summary) => cli::success(&summary),
        Err(e) => cli::error(&e.to_string(), "TRIAGE_ERROR"),
    }
}

fn cmd_rules(action: RulesAction) -> String {
    let config = match rules::load_rules(&rules::default_rules_path()) {
        Ok(c) => c,
        Err(e) => return cli::error(&e.to_string(), "RULES_ERROR"),
    };

    match action {
        RulesAction::List => cli::success(&config.rules),
        RulesAction::Vips => cli::success(&config.vip_senders),
    }
}
