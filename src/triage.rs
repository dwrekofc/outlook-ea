use serde::Serialize;
use thiserror::Error;

use crate::actions;
use crate::data::EmailSummary;
use crate::db::Store;
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
    store: &Store,
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
        if labels::get_label(store, email.id, &email.message_id)?.is_some() {
            continue;
        }

        let result = rules::evaluate_rules(config, &email.sender_address, &email.subject);

        // VIP protection shields human correspondence from auto trash/archive,
        // but NOT auto-generated calendar status notices (e.g. "Canceled: ..."),
        // which are noise even from a VIP and should be cleared by the
        // Auto-trash Calendar rules.
        let vip_protected = rules::is_vip(config, &email.sender_address)
            && !rules::is_calendar_notice_subject(&email.subject);

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
                                    store,
                                    email.id,
                                    &email.message_id,
                                    label_num,
                                )?;
                            }
                            summary.labeled.push(triage_action);
                        }
                        ActionType::Trash => {
                            // VIP emails are never trashed
                            if !vip_protected {
                                if let Err(e) = actions::delete_email(&email.message_id) {
                                    summary
                                        .warnings
                                        .push(format!("Could not trash email {}: {e}", email.id));
                                }
                                summary.trashed.push(triage_action);
                            }
                        }
                        ActionType::Archive => {
                            if !vip_protected {
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
                            if !vip_protected {
                                summary.trashed.push(triage_action);
                            }
                        }
                        ActionType::Archive => {
                            if !vip_protected {
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
mod regression_tests;
