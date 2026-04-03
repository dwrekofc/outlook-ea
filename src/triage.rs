use rusqlite::Connection;
use serde::Serialize;
use thiserror::Error;

use crate::actions;
use crate::data::EmailSummary;
use crate::labels;
use crate::rules::{self, ActionType, RulesConfig};

#[derive(Error, Debug)]
pub enum TriageError {
    #[error("Label error: {0}")]
    Label(#[from] labels::LabelError),
    #[error("Action error: {0}")]
    Action(#[from] actions::ActionError),
    #[error("Rules error: {0}")]
    Rules(#[from] rules::RulesError),
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

pub type TriageResult<T> = Result<T, TriageError>;

#[derive(Debug, Clone, Serialize)]
pub struct TriageSummary {
    pub labeled: Vec<TriageAction>,
    pub trashed: Vec<TriageAction>,
    pub archived: Vec<TriageAction>,
    pub untriaged: usize,
    pub total_processed: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TriageAction {
    pub email_id: i64,
    pub message_id: String,
    pub subject: String,
    pub rule_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label_number: Option<u8>,
}

/// Run auto-triage on a list of untriaged emails.
/// Evaluates each email against the rules config and applies matching actions.
pub fn auto_triage(
    overlay_conn: &Connection,
    config: &RulesConfig,
    untriaged_emails: &[EmailSummary],
    dry_run: bool,
) -> TriageResult<TriageSummary> {
    let mut summary = TriageSummary {
        labeled: vec![],
        trashed: vec![],
        archived: vec![],
        untriaged: 0,
        total_processed: untriaged_emails.len(),
        warnings: vec![],
    };

    for email in untriaged_emails {
        // Check if already labeled (idempotency)
        if let Some(_existing) = labels::get_label(overlay_conn, email.id)? {
            continue;
        }

        let result = rules::evaluate_rules(config, &email.sender_address, &email.subject);

        match result {
            Some((rule_name, action)) => {
                let triage_action = TriageAction {
                    email_id: email.id,
                    message_id: email.message_id.clone(),
                    subject: email.subject.clone(),
                    rule_name: rule_name.clone(),
                    label_number: action.label_number,
                };

                if !dry_run {
                    match action.action_type {
                        ActionType::Label => {
                            if let Some(label_num) = action.label_number {
                                labels::assign_label(
                                    overlay_conn,
                                    email.id,
                                    &email.message_id,
                                    label_num,
                                )?;
                            }
                            summary.labeled.push(triage_action);
                        }
                        ActionType::Trash => {
                            // VIP emails are never trashed
                            if !rules::is_vip(config, &email.sender_address) {
                                if let Err(e) = actions::delete_email(&email.message_id) {
                                    summary
                                        .warnings
                                        .push(format!("Could not trash email {}: {e}", email.id));
                                }
                                summary.trashed.push(triage_action);
                            }
                        }
                        ActionType::Archive => {
                            if !rules::is_vip(config, &email.sender_address) {
                                // Mark as read before archiving (e.g. SAP Appreciate)
                                if let Err(e) = actions::set_read_status(&email.message_id, true) {
                                    summary.warnings.push(format!(
                                        "Could not mark email {} as read: {e}",
                                        email.id
                                    ));
                                }
                                if let Err(e) = actions::archive_email(&email.message_id) {
                                    summary
                                        .warnings
                                        .push(format!("Could not archive email {}: {e}", email.id));
                                }
                                summary.archived.push(triage_action);
                            }
                        }
                    }
                } else {
                    match action.action_type {
                        ActionType::Label => summary.labeled.push(triage_action),
                        ActionType::Trash => {
                            if !rules::is_vip(config, &email.sender_address) {
                                summary.trashed.push(triage_action);
                            }
                        }
                        ActionType::Archive => {
                            if !rules::is_vip(config, &email.sender_address) {
                                summary.archived.push(triage_action);
                            }
                        }
                    }
                }
            }
            None => {
                summary.untriaged += 1;
            }
        }
    }

    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_overlay_db_memory;
    use crate::rules::*;

    fn sample_config() -> RulesConfig {
        RulesConfig {
            rules: vec![
                Rule {
                    name: "Receipts".to_string(),
                    match_criteria: MatchCriteria {
                        subject_contains: Some("receipt".to_string()),
                        ..Default::default()
                    },
                    action: Action {
                        action_type: ActionType::Label,
                        label_number: Some(5),
                    },
                },
                Rule {
                    name: "Food spam".to_string(),
                    match_criteria: MatchCriteria {
                        sender_contains: Some("doordash".to_string()),
                        ..Default::default()
                    },
                    action: Action {
                        action_type: ActionType::Trash,
                        label_number: None,
                    },
                },
            ],
            vip_senders: vec![VipSender {
                address: "boss@company.com".to_string(),
                name: Some("Boss".to_string()),
            }],
        }
    }

    fn make_email(id: i64, sender: &str, subject: &str) -> EmailSummary {
        EmailSummary {
            id,
            message_id: format!("msg{id}@test"),
            sender_name: String::new(),
            sender_address: sender.to_string(),
            subject: subject.to_string(),
            date: "2024-01-01T00:00:00+00:00".to_string(),
            is_read: false,
            folder: "INBOX".to_string(),
            label: None,
            sender_context: None,
        }
    }

    #[test]
    fn test_auto_triage_labels_receipts() {
        let conn = open_overlay_db_memory().unwrap();
        let config = sample_config();
        let emails = vec![make_email(1, "store@shop.com", "Your receipt #123")];

        let summary = auto_triage(&conn, &config, &emails, false).unwrap();
        assert_eq!(summary.labeled.len(), 1);
        assert_eq!(summary.labeled[0].label_number, Some(5));

        // Verify label was actually stored
        let label = labels::get_label(&conn, 1).unwrap().unwrap();
        assert_eq!(label.label_number, 5);
    }

    #[test]
    fn test_auto_triage_trashes_matching() {
        let conn = open_overlay_db_memory().unwrap();
        let config = sample_config();
        let emails = vec![make_email(2, "noreply@doordash.com", "Your order is ready")];

        // Dry run — won't actually call AppleScript
        let summary = auto_triage(&conn, &config, &emails, true).unwrap();
        assert_eq!(summary.trashed.len(), 1);
    }

    #[test]
    fn test_vip_auto_labeled_follow_up() {
        let conn = open_overlay_db_memory().unwrap();
        let config = sample_config();
        let emails = vec![make_email(3, "boss@company.com", "Important meeting")];

        let summary = auto_triage(&conn, &config, &emails, false).unwrap();
        assert_eq!(summary.labeled.len(), 1);
        assert_eq!(summary.labeled[0].rule_name, "VIP Sender");
        assert_eq!(summary.labeled[0].label_number, Some(1));
    }

    #[test]
    fn test_vip_never_trashed() {
        let conn = open_overlay_db_memory().unwrap();
        let mut config = sample_config();
        // Make boss also match the trash rule
        config.rules.insert(
            0,
            Rule {
                name: "Trash all".to_string(),
                match_criteria: MatchCriteria {
                    sender_contains: Some("boss".to_string()),
                    ..Default::default()
                },
                action: Action {
                    action_type: ActionType::Trash,
                    label_number: None,
                },
            },
        );
        let emails = vec![make_email(4, "boss@company.com", "Something")];

        let summary = auto_triage(&conn, &config, &emails, true).unwrap();
        // VIP takes priority — should be labeled, not trashed
        assert_eq!(summary.labeled.len(), 1);
        assert_eq!(summary.trashed.len(), 0);
    }

    #[test]
    fn test_no_match_stays_untriaged() {
        let conn = open_overlay_db_memory().unwrap();
        let config = sample_config();
        let emails = vec![make_email(5, "friend@gmail.com", "Dinner tonight?")];

        let summary = auto_triage(&conn, &config, &emails, true).unwrap();
        assert_eq!(summary.untriaged, 1);
        assert!(summary.labeled.is_empty());
        assert!(summary.trashed.is_empty());
    }

    #[test]
    fn test_idempotent() {
        let conn = open_overlay_db_memory().unwrap();
        let config = sample_config();
        let emails = vec![make_email(6, "store@shop.com", "Your receipt")];

        // Run twice
        auto_triage(&conn, &config, &emails, false).unwrap();
        let summary2 = auto_triage(&conn, &config, &emails, false).unwrap();

        // Second run should not re-label (already labeled)
        assert_eq!(summary2.labeled.len(), 0);
        assert_eq!(summary2.total_processed, 1);
    }

    #[test]
    fn test_triage_summary_warnings_field() {
        let conn = open_overlay_db_memory().unwrap();
        let config = sample_config();
        let emails = vec![make_email(20, "store@shop.com", "Your receipt")];

        let summary = auto_triage(&conn, &config, &emails, false).unwrap();
        // Warnings vec should exist but be empty when no AppleScript issues
        assert!(summary.warnings.is_empty());
    }

    #[test]
    fn test_triage_summary_counts() {
        let conn = open_overlay_db_memory().unwrap();
        let config = sample_config();
        let emails = vec![
            make_email(10, "store@shop.com", "Your receipt"),
            make_email(11, "noreply@doordash.com", "Delivered"),
            make_email(12, "friend@gmail.com", "Hey"),
            make_email(13, "boss@company.com", "Review this"),
        ];

        let summary = auto_triage(&conn, &config, &emails, true).unwrap();
        assert_eq!(summary.total_processed, 4);
        assert_eq!(summary.labeled.len(), 2); // receipt + VIP
        assert_eq!(summary.trashed.len(), 1); // doordash
        assert_eq!(summary.untriaged, 1); // friend
    }
}
