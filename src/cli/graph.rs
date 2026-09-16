use clap::Subcommand;

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
