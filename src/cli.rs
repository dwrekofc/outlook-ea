use clap::{Parser, Subcommand};
use serde::Serialize;
use serde_json::json;

#[derive(Parser)]
#[command(
    name = "mea",
    about = "Mail Email Assistant — CLI for email management"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// List emails in inbox or a specific folder
    List {
        /// Filter by folder name
        #[arg(long)]
        folder: Option<String>,
        /// Page number (0-indexed)
        #[arg(long, default_value = "0")]
        page: usize,
        /// Page size
        #[arg(long, default_value = "20")]
        page_size: usize,
        /// Filter by label (1-5)
        #[arg(long)]
        label: Option<u8>,
        /// Show only untriaged emails
        #[arg(long)]
        untriaged: bool,
    },
    /// Read an email's body content
    Read {
        /// Email rowid
        id: i64,
    },
    /// Search emails
    Search {
        /// Search by sender
        #[arg(long)]
        sender: Option<String>,
        /// Search by subject
        #[arg(long)]
        subject: Option<String>,
        /// Search from date (ISO 8601)
        #[arg(long)]
        date_from: Option<String>,
        /// Search to date (ISO 8601)
        #[arg(long)]
        date_to: Option<String>,
        /// Search body text (uses Spotlight)
        #[arg(long)]
        body: Option<String>,
    },
    /// Assign a triage label to an email
    Label {
        /// Email rowid
        id: i64,
        /// Label number (1-5, or 0 to clear)
        label: u8,
    },
    /// Delete (trash) an email
    Delete {
        /// Email rowid(s)
        ids: Vec<i64>,
        /// Skip confirmation
        #[arg(long)]
        yes: bool,
    },
    /// Archive an email
    Archive {
        /// Email rowid(s)
        ids: Vec<i64>,
        /// Skip confirmation
        #[arg(long)]
        yes: bool,
    },
    /// Flag or unflag an email
    Flag {
        /// Email rowid
        id: i64,
        /// Unflag instead of flag
        #[arg(long)]
        unflag: bool,
    },
    /// Mark email as read or unread
    MarkRead {
        /// Email rowid
        id: i64,
        /// Mark as unread instead
        #[arg(long)]
        unread: bool,
    },
    /// Run auto-triage on untriaged emails
    Triage {
        /// Dry run — show what would happen without acting
        #[arg(long)]
        dry_run: bool,
    },
    /// List or manage rules
    Rules {
        #[command(subcommand)]
        action: RulesAction,
    },
}

#[derive(Subcommand)]
pub enum RulesAction {
    /// List all rules
    List,
    /// List VIP senders
    Vips,
}

/// Format a success response as JSON.
pub fn success<T: Serialize>(data: T) -> String {
    serde_json::to_string_pretty(&json!({
        "status": "ok",
        "data": data,
    }))
    .unwrap_or_else(|_| {
        r#"{"status":"error","error":"serialization failed","code":"SERIALIZE_ERROR"}"#.to_string()
    })
}

/// Format an error response as JSON.
pub fn error(message: &str, code: &str) -> String {
    serde_json::to_string_pretty(&json!({
        "status": "error",
        "error": message,
        "code": code,
    }))
    .unwrap_or_else(|_| format!(r#"{{"status":"error","error":"{message}","code":"{code}"}}"#))
}

/// Format a confirmation response as JSON.
pub fn confirm(message: &str, action: &str, count: usize) -> String {
    serde_json::to_string_pretty(&json!({
        "status": "confirm",
        "message": message,
        "action": action,
        "count": count,
    }))
    .unwrap_or_else(|_| {
        r#"{"status":"error","error":"serialization failed","code":"SERIALIZE_ERROR"}"#.to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_success_response() {
        let output = success(serde_json::json!({"count": 5}));
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["status"], "ok");
        assert_eq!(parsed["data"]["count"], 5);
    }

    #[test]
    fn test_error_response() {
        let output = error("not found", "NOT_FOUND");
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["status"], "error");
        assert_eq!(parsed["error"], "not found");
        assert_eq!(parsed["code"], "NOT_FOUND");
    }

    #[test]
    fn test_confirm_response() {
        let output = confirm("Delete 3 emails?", "delete", 3);
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["status"], "confirm");
        assert_eq!(parsed["count"], 3);
    }

    #[test]
    fn test_cli_parse_list() {
        use clap::Parser;
        let cli = Cli::parse_from(["mea", "list"]);
        assert!(matches!(cli.command, Commands::List { .. }));
    }

    #[test]
    fn test_cli_parse_label() {
        use clap::Parser;
        let cli = Cli::parse_from(["mea", "label", "42", "3"]);
        if let Commands::Label { id, label } = cli.command {
            assert_eq!(id, 42);
            assert_eq!(label, 3);
        } else {
            panic!("Expected Label command");
        }
    }

    #[test]
    fn test_cli_parse_search() {
        use clap::Parser;
        let cli = Cli::parse_from(["mea", "search", "--sender", "alice", "--subject", "hello"]);
        if let Commands::Search {
            sender, subject, ..
        } = cli.command
        {
            assert_eq!(sender.unwrap(), "alice");
            assert_eq!(subject.unwrap(), "hello");
        } else {
            panic!("Expected Search command");
        }
    }

    #[test]
    fn test_cli_parse_delete_with_yes() {
        use clap::Parser;
        let cli = Cli::parse_from(["mea", "delete", "--yes", "1", "2", "3"]);
        if let Commands::Delete { ids, yes } = cli.command {
            assert_eq!(ids, vec![1, 2, 3]);
            assert!(yes);
        } else {
            panic!("Expected Delete command");
        }
    }

    #[test]
    fn test_cli_parse_triage() {
        use clap::Parser;
        let cli = Cli::parse_from(["mea", "triage", "--dry-run"]);
        if let Commands::Triage { dry_run } = cli.command {
            assert!(dry_run);
        } else {
            panic!("Expected Triage command");
        }
    }

    #[test]
    fn test_all_output_valid_json() {
        // Test that all response formatters produce valid JSON
        let s = success("test");
        assert!(serde_json::from_str::<serde_json::Value>(&s).is_ok());

        let e = error("msg", "CODE");
        assert!(serde_json::from_str::<serde_json::Value>(&e).is_ok());

        let c = confirm("msg", "action", 0);
        assert!(serde_json::from_str::<serde_json::Value>(&c).is_ok());
    }
}
