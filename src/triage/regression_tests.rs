use super::*;
use crate::db::test_store;
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
        conversation_id: None,
        label: None,
        needs_reply: None,
        sender_context: None,
    }
}

#[test]
fn test_auto_triage_labels_receipts() {
    let conn = test_store().unwrap();
    let config = sample_config();
    let emails = vec![make_email(1, "store@shop.com", "Your receipt #123")];

    let summary = auto_triage(&conn, &config, &emails, false).unwrap();
    assert_eq!(summary.labeled.len(), 1);
    assert_eq!(summary.labeled[0].label_number, Some(5));

    // Verify label was actually stored
    let label = labels::get_label(&conn, 1, "msg1@test").unwrap().unwrap();
    assert_eq!(label.label_number, 5);
}

#[test]
fn test_auto_triage_trashes_matching() {
    let conn = test_store().unwrap();
    let config = sample_config();
    let emails = vec![make_email(2, "noreply@doordash.com", "Your order is ready")];

    // Dry run — won't actually call AppleScript
    let summary = auto_triage(&conn, &config, &emails, true).unwrap();
    assert_eq!(summary.trashed.len(), 1);
}

#[test]
fn test_vip_auto_labeled_follow_up() {
    let conn = test_store().unwrap();
    let config = sample_config();
    let emails = vec![make_email(3, "boss@company.com", "Important meeting")];

    let summary = auto_triage(&conn, &config, &emails, false).unwrap();
    assert_eq!(summary.labeled.len(), 1);
    assert_eq!(summary.labeled[0].rule_name, "VIP Sender");
    assert_eq!(summary.labeled[0].label_number, Some(1));
}

#[test]
fn test_vip_never_trashed() {
    let conn = test_store().unwrap();
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
    let conn = test_store().unwrap();
    let config = sample_config();
    let emails = vec![make_email(5, "friend@gmail.com", "Dinner tonight?")];

    let summary = auto_triage(&conn, &config, &emails, true).unwrap();
    assert_eq!(summary.untriaged, 1);
    assert!(summary.labeled.is_empty());
    assert!(summary.trashed.is_empty());
}

#[test]
fn test_idempotent() {
    let conn = test_store().unwrap();
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
    let conn = test_store().unwrap();
    let config = sample_config();
    let emails = vec![make_email(20, "store@shop.com", "Your receipt")];

    let summary = auto_triage(&conn, &config, &emails, false).unwrap();
    // Warnings vec should exist but be empty when no AppleScript issues
    assert!(summary.warnings.is_empty());
}

#[test]
fn test_triage_summary_counts() {
    let conn = test_store().unwrap();
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
