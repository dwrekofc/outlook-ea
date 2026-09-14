use crate::app::{open_envelope, open_store};
use crate::cli::{self, RulesAction};
use crate::{data, graph, labels, rules, triage};

pub(crate) fn cmd_triage(dry_run: bool) -> String {
    let envelope = match open_envelope() {
        Ok(conn) => conn,
        Err(err) => return cli::error(&err, "ENVELOPE_ERROR"),
    };
    let store = match open_store() {
        Ok(conn) => conn,
        Err(err) => return cli::error(&err, "STORAGE_ERROR"),
    };
    let config = match graph::graph_rules_to_config(&store) {
        Ok(config) if !config.rules.is_empty() || !config.vip_senders.is_empty() => {
            merge_rules(config)
        }
        Ok(_) => match rules::load_rules(&rules::default_rules_path()) {
            Ok(config) => config,
            Err(err) => return cli::error(&err.to_string(), "RULES_ERROR"),
        },
        Err(err) => return cli::error(&err.to_string(), "GRAPH_ERROR"),
    };
    let list = match data::list_emails(&envelope, None, 0, 10000) {
        Ok(result) => result,
        Err(err) => return cli::error(&err.to_string(), "LIST_ERROR"),
    };
    let identities: Vec<_> = list
        .emails
        .iter()
        .map(|email| (email.id, email.message_id.clone()))
        .collect();
    let labels = match labels::get_labels_for_messages(&store, &identities) {
        Ok(labels) => labels,
        Err(err) => return cli::error(&err.to_string(), "LABEL_ERROR"),
    };
    let untriaged: Vec<_> = list
        .emails
        .into_iter()
        .filter(|email| !labels.contains_key(&email.id))
        .collect();
    match triage::auto_triage(&store, &config, &untriaged, dry_run) {
        Ok(summary) => cli::success(&summary),
        Err(err) => cli::error(&err.to_string(), "TRIAGE_ERROR"),
    }
}

pub(crate) fn cmd_rules(action: RulesAction) -> String {
    let config = match rules::load_rules(&rules::default_rules_path()) {
        Ok(config) => config,
        Err(err) => return cli::error(&err.to_string(), "RULES_ERROR"),
    };
    match action {
        RulesAction::List => cli::success(&config.rules),
        RulesAction::Vips => cli::success(&config.vip_senders),
    }
}

pub(crate) fn fallback_vips() -> Vec<String> {
    rules::load_rules(&rules::default_rules_path())
        .unwrap_or_default()
        .vip_senders
        .into_iter()
        .map(|vip| vip.address)
        .collect()
}

pub(crate) fn is_vip_message(
    envelope: &crate::db::Store,
    rowid: i64,
    vip_addresses: &[String],
) -> bool {
    let addr = get_sender_address(envelope, rowid);
    let subject = get_subject(envelope, rowid);
    vip_addresses
        .iter()
        .any(|vip| vip.eq_ignore_ascii_case(&addr))
        && !rules::is_calendar_notice_subject(&subject)
}

fn merge_rules(mut graph_config: rules::RulesConfig) -> rules::RulesConfig {
    let file_config = rules::load_rules(&rules::default_rules_path()).unwrap_or_default();
    for rule in file_config.rules {
        if !graph_config.rules.iter().any(|item| item.name == rule.name) {
            graph_config.rules.push(rule);
        }
    }
    for vip in file_config.vip_senders {
        if !graph_config
            .vip_senders
            .iter()
            .any(|item| item.address.eq_ignore_ascii_case(&vip.address))
        {
            graph_config.vip_senders.push(vip);
        }
    }
    graph_config
}

fn get_sender_address(conn: &crate::db::Store, rowid: i64) -> String {
    conn.one(
        "SELECT COALESCE(a.address, '')
         FROM messages m JOIN addresses a ON m.sender = a.ROWID
         WHERE m.ROWID = ?1",
        libsql::params![rowid],
        |row| Ok(row.get::<String>(0)?),
    )
    .unwrap_or_default()
    .unwrap_or_default()
}

fn get_subject(conn: &crate::db::Store, rowid: i64) -> String {
    conn.one(
        "SELECT COALESCE(m.subject_prefix, '') || COALESCE(sub.subject, '')
         FROM messages m LEFT JOIN subjects sub ON m.subject = sub.ROWID
         WHERE m.ROWID = ?1",
        libsql::params![rowid],
        |row| Ok(row.get::<String>(0)?),
    )
    .unwrap_or_default()
    .unwrap_or_default()
}
