use clap::Parser;
use mea::cli::{self, Cli, Commands, GraphAction, RulesAction};
use mea::{actions, body, data, db, graph, labels, rules, search, triage};

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
        Commands::Read { id, all_folders } => cmd_read(id, all_folders),
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
        Commands::Sync => cmd_sync(),
        Commands::Rules { action } => cmd_rules(action),
        Commands::Graph { action } => cmd_graph(action),
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

/// Get RFC822 Message-ID header from V10 envelope for a given rowid.
fn get_message_id_header(envelope_conn: &rusqlite::Connection, rowid: i64) -> String {
    envelope_conn
        .query_row(
            "SELECT COALESCE(mgd.message_id_header, '')
             FROM messages m
             LEFT JOIN message_global_data mgd ON mgd.ROWID = m.global_message_id
             WHERE m.ROWID = ?1",
            [rowid],
            |r| r.get::<_, String>(0),
        )
        .unwrap_or_default()
}

/// Get sender address from V10 envelope for a given rowid.
fn get_sender_address(envelope_conn: &rusqlite::Connection, rowid: i64) -> String {
    envelope_conn
        .query_row(
            "SELECT COALESCE(a.address, '')
             FROM messages m
             JOIN addresses a ON m.sender = a.ROWID
             WHERE m.ROWID = ?1",
            [rowid],
            |r| r.get::<_, String>(0),
        )
        .unwrap_or_default()
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

fn cmd_read(id: i64, all_folders: bool) -> String {
    let envelope_conn = match open_envelope() {
        Ok(c) => c,
        Err(e) => return cli::error(&e, "ENVELOPE_ERROR"),
    };
    let overlay_conn = match open_overlay() {
        Ok(c) => c,
        Err(e) => return cli::error(&e, "OVERLAY_ERROR"),
    };

    let inbox_only = !all_folders;
    match body::read_email_body(&overlay_conn, &envelope_conn, id, inbox_only) {
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

    let overlay_conn = open_overlay().ok();

    let query = search::SearchQuery {
        sender,
        subject,
        date_from,
        date_to,
        body_text,
    };

    match search::search_emails(&envelope_conn, &query, overlay_conn.as_ref()) {
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
        Ok(env_conn) => get_message_id_header(&env_conn, id),
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

    // Prefer graph VIP emails; fall back to rules.toml
    let vip_addresses: Vec<String> = match open_overlay()
        .ok()
        .and_then(|c| graph::get_vip_emails(&c).ok())
    {
        Some(graph_vips) if !graph_vips.is_empty() => graph_vips,
        _ => {
            let rules_config = rules::load_rules(&rules::default_rules_path()).unwrap_or_default();
            rules_config
                .vip_senders
                .iter()
                .map(|v| v.address.clone())
                .collect()
        }
    };

    // Resolve message IDs from envelope
    let envelope_conn = match open_envelope() {
        Ok(c) => c,
        Err(e) => return cli::error(&e, "ENVELOPE_ERROR"),
    };

    let mut message_ids = vec![];
    let mut vip_message_ids = vec![];

    for &id in &ids {
        let msg_id = get_message_id_header(&envelope_conn, id);
        let addr = get_sender_address(&envelope_conn, id);
        if vip_addresses.iter().any(|v| v.eq_ignore_ascii_case(&addr)) {
            vip_message_ids.push(msg_id.clone());
        }
        message_ids.push(msg_id);
    }

    match actions::bulk_action(&message_ids, "delete", &vip_message_ids) {
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

    // Prefer graph VIP emails; fall back to rules.toml
    let vip_addresses: Vec<String> = match open_overlay()
        .ok()
        .and_then(|c| graph::get_vip_emails(&c).ok())
    {
        Some(graph_vips) if !graph_vips.is_empty() => graph_vips,
        _ => {
            let rules_config = rules::load_rules(&rules::default_rules_path()).unwrap_or_default();
            rules_config
                .vip_senders
                .iter()
                .map(|v| v.address.clone())
                .collect()
        }
    };

    let envelope_conn = match open_envelope() {
        Ok(c) => c,
        Err(e) => return cli::error(&e, "ENVELOPE_ERROR"),
    };

    let mut message_ids = vec![];
    let mut vip_message_ids = vec![];

    for &id in &ids {
        let msg_id = get_message_id_header(&envelope_conn, id);
        let addr = get_sender_address(&envelope_conn, id);
        if vip_addresses.iter().any(|v| v.eq_ignore_ascii_case(&addr)) {
            vip_message_ids.push(msg_id.clone());
        }
        message_ids.push(msg_id);
    }

    match actions::bulk_action(&message_ids, "archive", &vip_message_ids) {
        Ok(resp) => cli::success(&resp),
        Err(e) => cli::error(&e.to_string(), "ACTION_ERROR"),
    }
}

fn cmd_flag(id: i64, unflag: bool) -> String {
    let envelope_conn = match open_envelope() {
        Ok(c) => c,
        Err(e) => return cli::error(&e, "ENVELOPE_ERROR"),
    };

    let msg_id = get_message_id_header(&envelope_conn, id);

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

    let msg_id = get_message_id_header(&envelope_conn, id);

    match actions::set_read_status(&msg_id, !unread) {
        Ok(()) => cli::success(serde_json::json!({
            "email_id": id,
            "read": !unread,
        })),
        Err(e) => cli::error(&e.to_string(), "ACTION_ERROR"),
    }
}

fn cmd_sync() -> String {
    let script = r#"tell application "Mail" to check for new mail"#;
    match std::process::Command::new("osascript")
        .args(["-e", script])
        .output()
    {
        Ok(output) if output.status.success() => {
            // Brief pause to let Mail.app begin the sync
            std::thread::sleep(std::time::Duration::from_secs(2));
            cli::success(serde_json::json!({"message": "Sync initiated"}))
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            cli::error(&format!("AppleScript error: {stderr}"), "SYNC_ERROR")
        }
        Err(e) => cli::error(&format!("Failed to run osascript: {e}"), "SYNC_ERROR"),
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

    // Load rules: prefer graph-based rules, fall back to rules.toml
    let config = match graph::graph_rules_to_config(&overlay_conn) {
        Ok(gc) if !gc.rules.is_empty() || !gc.vip_senders.is_empty() => {
            // Merge with rules.toml as fallback for any rules not in graph
            let file_config = rules::load_rules(&rules::default_rules_path()).unwrap_or_default();
            let mut merged = gc;
            for rule in file_config.rules {
                if !merged.rules.iter().any(|r| r.name == rule.name) {
                    merged.rules.push(rule);
                }
            }
            for vip in file_config.vip_senders {
                if !merged
                    .vip_senders
                    .iter()
                    .any(|v| v.address.eq_ignore_ascii_case(&vip.address))
                {
                    merged.vip_senders.push(vip);
                }
            }
            merged
        }
        _ => match rules::load_rules(&rules::default_rules_path()) {
            Ok(c) => c,
            Err(e) => return cli::error(&e.to_string(), "RULES_ERROR"),
        },
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

fn cmd_graph(action: GraphAction) -> String {
    let overlay_conn = match open_overlay() {
        Ok(c) => c,
        Err(e) => return cli::error(&e, "OVERLAY_ERROR"),
    };

    match action {
        GraphAction::Add {
            r#type,
            name,
            email,
            description,
            vip,
        } => {
            match graph::add_node(
                &overlay_conn,
                &r#type,
                &name,
                email.as_deref(),
                description.as_deref(),
                None,
                vip,
            ) {
                Ok(id) => {
                    let _ = graph::auto_dump(&overlay_conn);
                    cli::success(serde_json::json!({"id": id, "node_type": r#type, "name": name}))
                }
                Err(e) => cli::error(&e.to_string(), "GRAPH_ERROR"),
            }
        }
        GraphAction::Link {
            from,
            to,
            predicate,
            context,
        } => match graph::add_edge(
            &overlay_conn,
            from,
            to,
            &predicate,
            context.as_deref(),
            None,
        ) {
            Ok(id) => {
                let _ = graph::auto_dump(&overlay_conn);
                cli::success(
                    serde_json::json!({"edge_id": id, "from": from, "to": to, "predicate": predicate}),
                )
            }
            Err(e) => cli::error(&e.to_string(), "GRAPH_ERROR"),
        },
        GraphAction::Show { id } => match graph::get_node(&overlay_conn, id) {
            Ok(node) => {
                let edges = graph::get_edges(&overlay_conn, id, None).unwrap_or_default();
                cli::success(serde_json::json!({"node": node, "edges": edges}))
            }
            Err(e) => cli::error(&e.to_string(), "GRAPH_ERROR"),
        },
        GraphAction::List { r#type, vip } => {
            match graph::list_nodes(&overlay_conn, r#type.as_deref(), vip) {
                Ok(nodes) => {
                    cli::success(serde_json::json!({"nodes": nodes, "count": nodes.len()}))
                }
                Err(e) => cli::error(&e.to_string(), "GRAPH_ERROR"),
            }
        }
        GraphAction::Find { query } => match graph::find_nodes(&overlay_conn, &query) {
            Ok(nodes) => cli::success(serde_json::json!({"nodes": nodes, "count": nodes.len()})),
            Err(e) => cli::error(&e.to_string(), "GRAPH_ERROR"),
        },
        GraphAction::Edges { id, predicate } => {
            match graph::get_edges(&overlay_conn, id, predicate.as_deref()) {
                Ok(edges) => {
                    cli::success(serde_json::json!({"edges": edges, "count": edges.len()}))
                }
                Err(e) => cli::error(&e.to_string(), "GRAPH_ERROR"),
            }
        }
        GraphAction::Traverse {
            id,
            predicate,
            depth,
        } => match graph::traverse(&overlay_conn, id, predicate.as_deref(), depth) {
            Ok(results) => {
                cli::success(serde_json::json!({"results": results, "count": results.len()}))
            }
            Err(e) => cli::error(&e.to_string(), "GRAPH_ERROR"),
        },
        GraphAction::Remove { id } => match graph::remove_node(&overlay_conn, id) {
            Ok(()) => {
                let _ = graph::auto_dump(&overlay_conn);
                cli::success(serde_json::json!({"removed": id}))
            }
            Err(e) => cli::error(&e.to_string(), "GRAPH_ERROR"),
        },
        GraphAction::Unlink { edge_id } => match graph::remove_edge(&overlay_conn, edge_id) {
            Ok(()) => {
                let _ = graph::auto_dump(&overlay_conn);
                cli::success(serde_json::json!({"unlinked": edge_id}))
            }
            Err(e) => cli::error(&e.to_string(), "GRAPH_ERROR"),
        },
        GraphAction::AddVip {
            email,
            name,
            description,
            context,
        } => match graph::add_vip(
            &overlay_conn,
            &name,
            &email,
            description.as_deref(),
            context.as_deref(),
        ) {
            Ok(id) => {
                let _ = graph::auto_dump(&overlay_conn);
                cli::success(
                    serde_json::json!({"person_id": id, "name": name, "email": email, "is_vip": true}),
                )
            }
            Err(e) => cli::error(&e.to_string(), "GRAPH_ERROR"),
        },
        GraphAction::AddRule {
            name,
            match_sender,
            match_subject,
            action,
        } => {
            let (match_type, match_value) = if let Some(ref sender) = match_sender {
                ("sender", sender.as_str())
            } else if let Some(ref subject) = match_subject {
                ("subject", subject.as_str())
            } else {
                return cli::error(
                    "Must specify --match-sender or --match-subject",
                    "INVALID_ARGS",
                );
            };

            // Parse action string (e.g. "label:1", "trash", "archive")
            let (action_type, action_value) = if let Some(rest) = action.strip_prefix("label:") {
                ("label", rest)
            } else {
                (action.as_str(), "")
            };

            match graph::add_rule(
                &overlay_conn,
                &name,
                match_type,
                match_value,
                action_type,
                action_value,
            ) {
                Ok(id) => {
                    let _ = graph::auto_dump(&overlay_conn);
                    cli::success(serde_json::json!({"rule_id": id, "name": name}))
                }
                Err(e) => cli::error(&e.to_string(), "GRAPH_ERROR"),
            }
        }
        GraphAction::Rules => match graph::get_all_rules(&overlay_conn) {
            Ok(rules) => cli::success(serde_json::json!({"rules": rules, "count": rules.len()})),
            Err(e) => cli::error(&e.to_string(), "GRAPH_ERROR"),
        },
        GraphAction::Dump => match graph::dump_context(&overlay_conn) {
            Ok(content) => {
                let _ = graph::auto_dump(&overlay_conn);
                cli::success(serde_json::json!({"content": content}))
            }
            Err(e) => cli::error(&e.to_string(), "GRAPH_ERROR"),
        },
        GraphAction::AddProject { name, description } => {
            match graph::add_project(&overlay_conn, &name, description.as_deref()) {
                Ok(id) => {
                    let _ = graph::auto_dump(&overlay_conn);
                    cli::success(serde_json::json!({"project_id": id, "name": name}))
                }
                Err(e) => cli::error(&e.to_string(), "GRAPH_ERROR"),
            }
        }
        GraphAction::AddTask {
            title,
            description,
            due,
            project,
        } => match graph::add_task(
            &overlay_conn,
            &title,
            description.as_deref(),
            due.as_deref(),
            project,
        ) {
            Ok(id) => {
                let _ = graph::auto_dump(&overlay_conn);
                cli::success(serde_json::json!({"task_id": id, "title": title}))
            }
            Err(e) => cli::error(&e.to_string(), "GRAPH_ERROR"),
        },
        GraphAction::Tasks { project, status } => {
            match graph::list_tasks(&overlay_conn, project, status.as_deref()) {
                Ok(tasks) => {
                    cli::success(serde_json::json!({"tasks": tasks, "count": tasks.len()}))
                }
                Err(e) => cli::error(&e.to_string(), "GRAPH_ERROR"),
            }
        }
        GraphAction::Projects { active } => match graph::list_projects(&overlay_conn, active) {
            Ok(projects) => {
                cli::success(serde_json::json!({"projects": projects, "count": projects.len()}))
            }
            Err(e) => cli::error(&e.to_string(), "GRAPH_ERROR"),
        },
        GraphAction::TaskStatus { id, status } => {
            match graph::update_task_status(&overlay_conn, id, &status) {
                Ok(()) => {
                    let _ = graph::auto_dump(&overlay_conn);
                    cli::success(serde_json::json!({"task_id": id, "status": status}))
                }
                Err(e) => cli::error(&e.to_string(), "GRAPH_ERROR"),
            }
        }
    }
}
