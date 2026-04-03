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
        /// Search all mail folders, not just Inbox
        #[arg(long)]
        all_folders: bool,
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
    /// Force Apple Mail to check for new mail
    Sync,
    /// List or manage rules
    Rules {
        #[command(subcommand)]
        action: RulesAction,
    },
    /// Manage the context graph (nodes, edges, traversal)
    Graph {
        #[command(subcommand)]
        action: GraphAction,
    },
}

#[derive(Subcommand)]
pub enum RulesAction {
    /// List all rules
    List,
    /// List VIP senders
    Vips,
}

#[derive(Subcommand)]
pub enum GraphAction {
    /// Add a node to the graph
    Add {
        /// Node type (person, team, project, topic, vendor, rule, action)
        #[arg(long, rename_all = "verbatim")]
        r#type: String,
        /// Node name
        #[arg(long)]
        name: String,
        /// Email address (optional)
        #[arg(long)]
        email: Option<String>,
        /// Description (optional)
        #[arg(long)]
        description: Option<String>,
        /// Mark as VIP
        #[arg(long)]
        vip: bool,
    },
    /// Create an edge between two nodes
    Link {
        /// Source node ID
        #[arg(long)]
        from: i64,
        /// Target node ID
        #[arg(long)]
        to: i64,
        /// Relationship predicate
        #[arg(long)]
        predicate: String,
        /// Optional context
        #[arg(long)]
        context: Option<String>,
    },
    /// Show a node and its edges
    Show {
        /// Node ID
        id: i64,
    },
    /// List nodes
    List {
        /// Filter by node type
        #[arg(long, rename_all = "verbatim")]
        r#type: Option<String>,
        /// Show only VIP nodes
        #[arg(long)]
        vip: bool,
    },
    /// Find nodes by name/email/description
    Find {
        /// Search query
        query: String,
    },
    /// Show edges for a node
    Edges {
        /// Node ID
        id: i64,
        /// Filter by predicate
        #[arg(long)]
        predicate: Option<String>,
    },
    /// Traverse the graph from a starting node
    Traverse {
        /// Starting node ID
        id: i64,
        /// Filter by predicate
        #[arg(long)]
        predicate: Option<String>,
        /// Max traversal depth
        #[arg(long, default_value = "2")]
        depth: usize,
    },
    /// Remove a node (cascades edges)
    Remove {
        /// Node ID
        id: i64,
    },
    /// Remove an edge
    Unlink {
        /// Edge ID
        edge_id: i64,
    },
    /// Add a VIP person with auto-generated rule
    AddVip {
        /// Email address
        #[arg(long)]
        email: String,
        /// Display name
        #[arg(long)]
        name: String,
        /// Description
        #[arg(long)]
        description: Option<String>,
        /// Context
        #[arg(long)]
        context: Option<String>,
    },
    /// Add a rule with match criteria and action
    AddRule {
        /// Rule name
        #[arg(long)]
        name: String,
        /// Match by sender pattern
        #[arg(long)]
        match_sender: Option<String>,
        /// Match by subject pattern
        #[arg(long)]
        match_subject: Option<String>,
        /// Action to apply (e.g. "label:1", "trash", "archive")
        #[arg(long)]
        action: String,
    },
    /// List all rules from the graph
    Rules,
    /// Dump graph context as markdown
    Dump,
    /// Add a project
    AddProject {
        /// Project name
        #[arg(long)]
        name: String,
        /// Description
        #[arg(long)]
        description: Option<String>,
    },
    /// Add a task
    AddTask {
        /// Task title
        #[arg(long)]
        title: String,
        /// Description
        #[arg(long)]
        description: Option<String>,
        /// Due date (ISO 8601, e.g. 2026-04-15)
        #[arg(long)]
        due: Option<String>,
        /// Link to a project by ID
        #[arg(long)]
        project: Option<i64>,
    },
    /// List tasks
    Tasks {
        /// Filter by project ID
        #[arg(long)]
        project: Option<i64>,
        /// Filter by status (todo, in_progress, done, blocked)
        #[arg(long)]
        status: Option<String>,
    },
    /// List projects
    Projects {
        /// Show only active projects
        #[arg(long)]
        active: bool,
    },
    /// Update a task's status
    TaskStatus {
        /// Task node ID
        id: i64,
        /// New status (todo, in_progress, done, blocked)
        #[arg(long)]
        status: String,
    },
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
