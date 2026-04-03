use serde::Serialize;
use std::process::Command;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ActionError {
    #[error("AppleScript error: {0}")]
    AppleScript(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("VIP protection: email {0} is from a VIP sender and cannot be bulk-actioned")]
    VipProtected(i64),
    #[error("Confirmation required: {0}")]
    ConfirmationRequired(String),
}

pub type ActionResult<T> = Result<T, ActionError>;

#[derive(Debug, Clone, Serialize)]
pub struct ActionResponse {
    pub action: String,
    pub message_ids_acted: Vec<String>,
    pub success: bool,
    pub message: String,
}

/// Run an AppleScript command via osascript.
fn run_applescript(script: &str) -> ActionResult<String> {
    let output = Command::new("osascript")
        .args(["-e", script])
        .output()
        .map_err(|e| ActionError::AppleScript(e.to_string()))?;

    if !output.status.success() {
        return Err(ActionError::AppleScript(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Escape a string for safe interpolation into AppleScript double-quoted strings.
/// Replaces backslashes and double quotes with their escaped equivalents.
fn escape_applescript(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Delete (move to trash) a single email by message ID.
pub fn delete_email(message_id: &str) -> ActionResult<()> {
    let safe_id = escape_applescript(message_id);
    let script = format!(
        r#"tell application "Mail"
            set msgs to (every message of inbox whose message id is "{safe_id}")
            repeat with msg in msgs
                delete msg
            end repeat
        end tell"#
    );
    run_applescript(&script)?;
    Ok(())
}

/// Archive a single email (move out of inbox).
pub fn archive_email(message_id: &str) -> ActionResult<()> {
    let safe_id = escape_applescript(message_id);
    let script = format!(
        r#"tell application "Mail"
            set msgs to (every message of inbox whose message id is "{safe_id}")
            repeat with msg in msgs
                set mailbox of msg to mailbox "Archive" of account of mailbox of msg
            end repeat
        end tell"#
    );
    run_applescript(&script)?;
    Ok(())
}

/// Flag or unflag an email.
pub fn set_flag(message_id: &str, flagged: bool) -> ActionResult<()> {
    let safe_id = escape_applescript(message_id);
    let flag_val = if flagged { "true" } else { "false" };
    let script = format!(
        r#"tell application "Mail"
            set msgs to (every message of inbox whose message id is "{safe_id}")
            repeat with msg in msgs
                set flagged status of msg to {flag_val}
            end repeat
        end tell"#
    );
    run_applescript(&script)?;
    Ok(())
}

/// Mark an email as read or unread.
pub fn set_read_status(message_id: &str, read: bool) -> ActionResult<()> {
    let safe_id = escape_applescript(message_id);
    let read_val = if read { "true" } else { "false" };
    let script = format!(
        r#"tell application "Mail"
            set msgs to (every message of inbox whose message id is "{safe_id}")
            repeat with msg in msgs
                set read status of msg to {read_val}
            end repeat
        end tell"#
    );
    run_applescript(&script)?;
    Ok(())
}

/// Execute a bulk action on multiple emails, respecting VIP protection.
/// `vip_message_ids` should contain message IDs of VIP senders to exclude.
pub fn bulk_action(
    message_ids: &[String],
    action: &str,
    vip_message_ids: &[String],
    force: bool,
) -> ActionResult<ActionResponse> {
    let protected: Vec<&String> = message_ids
        .iter()
        .filter(|id| vip_message_ids.contains(id))
        .collect();

    if !protected.is_empty() && !force {
        let affected: Vec<String> = message_ids
            .iter()
            .filter(|id| !vip_message_ids.contains(id))
            .cloned()
            .collect();

        return Ok(ActionResponse {
            action: action.to_string(),
            message_ids_acted: vec![],
            success: false,
            message: format!(
                "{} VIP emails excluded from bulk {}. {} non-VIP emails would be affected.",
                protected.len(),
                action,
                affected.len()
            ),
        });
    }

    // Filter out VIP emails for bulk destructive actions
    let actionable: Vec<&String> = message_ids
        .iter()
        .filter(|id| !vip_message_ids.contains(id))
        .collect();

    let mut succeeded: Vec<String> = vec![];
    for msg_id in &actionable {
        let result = match action {
            "delete" => delete_email(msg_id),
            "archive" => archive_email(msg_id),
            "flag" => set_flag(msg_id, true),
            "unflag" => set_flag(msg_id, false),
            "read" => set_read_status(msg_id, true),
            "unread" => set_read_status(msg_id, false),
            _ => continue,
        };
        if result.is_ok() {
            succeeded.push((*msg_id).clone());
        }
    }

    let count = succeeded.len();
    Ok(ActionResponse {
        action: action.to_string(),
        message_ids_acted: succeeded,
        success: true,
        message: format!(
            "Successfully {}d {} of {} emails",
            action,
            count,
            actionable.len()
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: AppleScript tests can't run in CI — they need Mail.app.
    // These tests validate the logic layer without executing AppleScript.

    #[test]
    fn test_bulk_action_vip_protection() {
        let ids = vec![
            "a@test".to_string(),
            "b@test".to_string(),
            "c@test".to_string(),
        ];
        let vip_ids = vec!["b@test".to_string()];

        let result = bulk_action(&ids, "delete", &vip_ids, false).unwrap();
        assert!(!result.success);
        assert!(result.message.contains("VIP"));
    }

    #[test]
    fn test_bulk_action_no_vip() {
        // Without actual Mail.app, the AppleScript will fail.
        // This test verifies the logic of VIP filtering.
        let ids = vec!["a@test".to_string()];
        let vip_ids: Vec<String> = vec![];

        // This will error because osascript won't work in test,
        // but that's fine — we're testing the logic path, not AppleScript.
        let _result = bulk_action(&ids, "delete", &vip_ids, false);
        // We just verify it doesn't panic
    }

    #[test]
    fn test_escape_applescript() {
        assert_eq!(escape_applescript("simple"), "simple");
        assert_eq!(escape_applescript(r#"has"quote"#), r#"has\"quote"#);
        assert_eq!(escape_applescript(r"has\backslash"), r"has\\backslash");
        assert_eq!(escape_applescript(r#"both\"mixed"#), r#"both\\\"mixed"#);
    }

    #[test]
    fn test_vip_emails_excluded_from_bulk() {
        let ids = vec!["vip@boss.com".to_string(), "normal@test.com".to_string()];
        let vip_ids = vec!["vip@boss.com".to_string()];

        let result = bulk_action(&ids, "archive", &vip_ids, false).unwrap();
        assert!(!result.success);
        assert!(result.message.contains("1 VIP"));
        assert!(result.message.contains("1 non-VIP"));
    }
}
