use chrono::Utc;
use rusqlite::Connection;
use serde::Serialize;
use thiserror::Error;

use crate::rules::{Action, ActionType, MatchCriteria, Rule, RulesConfig, VipSender};

#[derive(Error, Debug)]
pub enum GraphError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Node not found: {0}")]
    NodeNotFound(i64),
    #[error("Edge not found: {0}")]
    EdgeNotFound(i64),
}

pub type GraphResult<T> = Result<T, GraphError>;

#[derive(Debug, Clone, Serialize)]
pub struct Node {
    pub id: i64,
    pub node_type: String,
    pub name: String,
    pub email: Option<String>,
    pub description: Option<String>,
    pub metadata: String,
    pub is_vip: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Edge {
    pub id: i64,
    pub source_id: i64,
    pub target_id: i64,
    pub predicate: String,
    pub context: Option<String>,
    pub weight: f64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct EdgeWithNodes {
    pub edge: Edge,
    pub source: Node,
    pub target: Node,
}

#[derive(Debug, Clone, Serialize)]
pub struct SenderContext {
    pub node_id: i64,
    pub is_vip: bool,
    pub description: Option<String>,
    pub edges: Vec<String>,
    pub rules: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TraversalResult {
    pub node: Node,
    pub depth: usize,
    pub path: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuleWithDetails {
    pub rule_node: Node,
    pub match_type: Option<String>,
    pub match_value: Option<String>,
    pub action_type: Option<String>,
    pub action_value: Option<String>,
}

// ---------------------------------------------------------------------------
// Node CRUD
// ---------------------------------------------------------------------------

pub fn add_node(
    conn: &Connection,
    node_type: &str,
    name: &str,
    email: Option<&str>,
    description: Option<&str>,
    metadata: Option<&str>,
    is_vip: bool,
) -> GraphResult<i64> {
    let now = Utc::now().to_rfc3339();
    let meta = metadata.unwrap_or("{}");
    conn.execute(
        "INSERT INTO nodes (node_type, name, email, description, metadata, is_vip, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![node_type, name, email, description, meta, is_vip as i32, now, now],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get_node(conn: &Connection, id: i64) -> GraphResult<Node> {
    conn.query_row(
        "SELECT id, node_type, name, email, description, metadata, is_vip, created_at, updated_at
         FROM nodes WHERE id = ?1",
        [id],
        row_to_node,
    )
    .map_err(|_| GraphError::NodeNotFound(id))
}

pub fn find_nodes(conn: &Connection, query: &str) -> GraphResult<Vec<Node>> {
    let pattern = format!("%{query}%");
    let mut stmt = conn.prepare(
        "SELECT id, node_type, name, email, description, metadata, is_vip, created_at, updated_at
         FROM nodes
         WHERE name LIKE ?1 OR email LIKE ?1 OR description LIKE ?1
         ORDER BY name",
    )?;
    let rows = stmt
        .query_map([&pattern], row_to_node)?
        .filter_map(|r| r.ok())
        .collect();
    Ok(rows)
}

pub fn list_nodes(
    conn: &Connection,
    node_type: Option<&str>,
    vip_only: bool,
) -> GraphResult<Vec<Node>> {
    let sql = match (node_type, vip_only) {
        (Some(_), true) => {
            "SELECT id, node_type, name, email, description, metadata, is_vip, created_at, updated_at
             FROM nodes WHERE node_type = ?1 AND is_vip = 1 ORDER BY name"
        }
        (Some(_), false) => {
            "SELECT id, node_type, name, email, description, metadata, is_vip, created_at, updated_at
             FROM nodes WHERE node_type = ?1 ORDER BY name"
        }
        (None, true) => {
            "SELECT id, node_type, name, email, description, metadata, is_vip, created_at, updated_at
             FROM nodes WHERE is_vip = 1 ORDER BY name"
        }
        (None, false) => {
            "SELECT id, node_type, name, email, description, metadata, is_vip, created_at, updated_at
             FROM nodes ORDER BY name"
        }
    };

    let mut stmt = conn.prepare(sql)?;
    let rows = match node_type {
        Some(nt) => stmt
            .query_map([nt], row_to_node)?
            .filter_map(|r| r.ok())
            .collect(),
        None => stmt
            .query_map([], row_to_node)?
            .filter_map(|r| r.ok())
            .collect(),
    };
    Ok(rows)
}

pub fn update_node(
    conn: &Connection,
    id: i64,
    name: Option<&str>,
    email: Option<&str>,
    description: Option<&str>,
    metadata: Option<&str>,
    is_vip: Option<bool>,
) -> GraphResult<()> {
    // Verify exists
    get_node(conn, id)?;

    let now = Utc::now().to_rfc3339();
    if let Some(v) = name {
        conn.execute(
            "UPDATE nodes SET name = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![v, now, id],
        )?;
    }
    if let Some(v) = email {
        conn.execute(
            "UPDATE nodes SET email = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![v, now, id],
        )?;
    }
    if let Some(v) = description {
        conn.execute(
            "UPDATE nodes SET description = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![v, now, id],
        )?;
    }
    if let Some(v) = metadata {
        conn.execute(
            "UPDATE nodes SET metadata = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![v, now, id],
        )?;
    }
    if let Some(v) = is_vip {
        conn.execute(
            "UPDATE nodes SET is_vip = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![v as i32, now, id],
        )?;
    }
    Ok(())
}

pub fn remove_node(conn: &Connection, id: i64) -> GraphResult<()> {
    get_node(conn, id)?;
    // Edges cascade via ON DELETE CASCADE + foreign_keys=ON
    conn.execute("DELETE FROM nodes WHERE id = ?1", [id])?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Edge CRUD
// ---------------------------------------------------------------------------

pub fn add_edge(
    conn: &Connection,
    source_id: i64,
    target_id: i64,
    predicate: &str,
    context: Option<&str>,
    weight: Option<f64>,
) -> GraphResult<i64> {
    // Verify both nodes exist
    get_node(conn, source_id)?;
    get_node(conn, target_id)?;

    let now = Utc::now().to_rfc3339();
    let w = weight.unwrap_or(1.0);
    conn.execute(
        "INSERT INTO edges (source_id, target_id, predicate, context, weight, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![source_id, target_id, predicate, context, w, now],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get_edges(
    conn: &Connection,
    node_id: i64,
    predicate: Option<&str>,
) -> GraphResult<Vec<EdgeWithNodes>> {
    let sql = match predicate {
        Some(_) => {
            "SELECT e.id, e.source_id, e.target_id, e.predicate, e.context, e.weight, e.created_at
             FROM edges e
             WHERE (e.source_id = ?1 OR e.target_id = ?1) AND e.predicate = ?2
             ORDER BY e.created_at"
        }
        None => {
            "SELECT e.id, e.source_id, e.target_id, e.predicate, e.context, e.weight, e.created_at
             FROM edges e
             WHERE e.source_id = ?1 OR e.target_id = ?1
             ORDER BY e.created_at"
        }
    };

    let mut stmt = conn.prepare(sql)?;
    let edges: Vec<Edge> = match predicate {
        Some(p) => stmt
            .query_map(rusqlite::params![node_id, p], row_to_edge)?
            .filter_map(|r| r.ok())
            .collect(),
        None => stmt
            .query_map([node_id], row_to_edge)?
            .filter_map(|r| r.ok())
            .collect(),
    };

    let mut result = Vec::with_capacity(edges.len());
    for edge in edges {
        let source = get_node(conn, edge.source_id)?;
        let target = get_node(conn, edge.target_id)?;
        result.push(EdgeWithNodes {
            edge,
            source,
            target,
        });
    }
    Ok(result)
}

pub fn remove_edge(conn: &Connection, edge_id: i64) -> GraphResult<()> {
    let affected = conn.execute("DELETE FROM edges WHERE id = ?1", [edge_id])?;
    if affected == 0 {
        return Err(GraphError::EdgeNotFound(edge_id));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Traversal
// ---------------------------------------------------------------------------

pub fn traverse(
    conn: &Connection,
    start_id: i64,
    predicate: Option<&str>,
    depth: usize,
) -> GraphResult<Vec<TraversalResult>> {
    let start = get_node(conn, start_id)?;
    let mut results = vec![TraversalResult {
        node: start,
        depth: 0,
        path: vec![],
    }];

    let mut visited = std::collections::HashSet::new();
    visited.insert(start_id);

    let mut frontier = vec![(start_id, 0usize, Vec::<String>::new())];

    while let Some((current_id, current_depth, current_path)) = frontier.pop() {
        if current_depth >= depth {
            continue;
        }

        let edges = get_edges_raw(conn, current_id, predicate)?;
        for (edge_pred, neighbor_id) in edges {
            if visited.contains(&neighbor_id) {
                continue;
            }
            visited.insert(neighbor_id);

            let mut path = current_path.clone();
            path.push(edge_pred);

            let node = get_node(conn, neighbor_id)?;
            results.push(TraversalResult {
                node,
                depth: current_depth + 1,
                path: path.clone(),
            });
            frontier.push((neighbor_id, current_depth + 1, path));
        }
    }

    Ok(results)
}

/// Raw edge query returning (predicate, neighbor_id) pairs for traversal.
fn get_edges_raw(
    conn: &Connection,
    node_id: i64,
    predicate: Option<&str>,
) -> GraphResult<Vec<(String, i64)>> {
    let sql = match predicate {
        Some(_) => {
            "SELECT predicate, CASE WHEN source_id = ?1 THEN target_id ELSE source_id END
             FROM edges
             WHERE (source_id = ?1 OR target_id = ?1) AND predicate = ?2"
        }
        None => {
            "SELECT predicate, CASE WHEN source_id = ?1 THEN target_id ELSE source_id END
             FROM edges
             WHERE source_id = ?1 OR target_id = ?1"
        }
    };

    let mut stmt = conn.prepare(sql)?;
    let rows: Vec<(String, i64)> = match predicate {
        Some(p) => stmt
            .query_map(rusqlite::params![node_id, p], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?
            .filter_map(|r| r.ok())
            .collect(),
        None => stmt
            .query_map([node_id], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(|r| r.ok())
            .collect(),
    };
    Ok(rows)
}

// ---------------------------------------------------------------------------
// Context helpers
// ---------------------------------------------------------------------------

pub fn get_sender_context(conn: &Connection, email: &str) -> GraphResult<Option<SenderContext>> {
    let node = match conn.query_row(
        "SELECT id, node_type, name, email, description, metadata, is_vip, created_at, updated_at
         FROM nodes WHERE email = ?1",
        [email],
        row_to_node,
    ) {
        Ok(n) => n,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
        Err(e) => return Err(e.into()),
    };

    let edges_with_nodes = get_edges(conn, node.id, None)?;
    let edge_summaries: Vec<String> = edges_with_nodes
        .iter()
        .map(|ewn| {
            let other = if ewn.edge.source_id == node.id {
                &ewn.target
            } else {
                &ewn.source
            };
            format!("{} -> {}", ewn.edge.predicate, other.name)
        })
        .collect();

    // Find rules connected to this person
    let rules: Vec<String> = edges_with_nodes
        .iter()
        .filter(|ewn| {
            let other = if ewn.edge.source_id == node.id {
                &ewn.target
            } else {
                &ewn.source
            };
            other.node_type == "rule"
        })
        .map(|ewn| {
            let rule_node = if ewn.edge.source_id == node.id {
                &ewn.target
            } else {
                &ewn.source
            };
            rule_node.name.clone()
        })
        .collect();

    Ok(Some(SenderContext {
        node_id: node.id,
        is_vip: node.is_vip,
        description: node.description.clone(),
        edges: edge_summaries,
        rules,
    }))
}

pub fn get_vip_emails(conn: &Connection) -> GraphResult<Vec<String>> {
    let mut stmt = conn
        .prepare("SELECT email FROM nodes WHERE is_vip = 1 AND email IS NOT NULL ORDER BY name")?;
    let rows: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();
    Ok(rows)
}

pub fn get_all_rules(conn: &Connection) -> GraphResult<Vec<RuleWithDetails>> {
    let mut stmt = conn.prepare(
        "SELECT id, node_type, name, email, description, metadata, is_vip, created_at, updated_at
         FROM nodes WHERE node_type = 'rule' ORDER BY name",
    )?;
    let rule_nodes: Vec<Node> = stmt
        .query_map([], row_to_node)?
        .filter_map(|r| r.ok())
        .collect();

    let mut results = Vec::with_capacity(rule_nodes.len());
    for rule_node in rule_nodes {
        let edges = get_edges(conn, rule_node.id, None)?;

        let mut match_type = None;
        let mut match_value = None;
        let mut action_type = None;
        let mut action_value = None;

        for ewn in &edges {
            let target = if ewn.edge.source_id == rule_node.id {
                &ewn.target
            } else {
                &ewn.source
            };

            match ewn.edge.predicate.as_str() {
                "matches_sender" => {
                    match_type = Some("sender".to_string());
                    match_value = target.email.clone().or_else(|| Some(target.name.clone()));
                }
                "matches_subject" => {
                    match_type = Some("subject".to_string());
                    match_value = Some(target.name.clone());
                }
                pred if pred.starts_with("applies_action") => {
                    action_type =
                        Some(ewn.edge.context.clone().unwrap_or_else(|| pred.to_string()));
                    action_value = ewn.edge.context.clone();
                }
                "protects" => {
                    if action_type.is_none() {
                        action_type = Some("protect".to_string());
                    }
                }
                _ => {}
            }
        }

        // Fallback: parse metadata for action info
        if action_type.is_none()
            && let Ok(meta) = serde_json::from_str::<serde_json::Value>(&rule_node.metadata)
        {
            if let Some(at) = meta.get("action_type").and_then(|v| v.as_str()) {
                action_type = Some(at.to_string());
            }
            if let Some(av) = meta.get("action_value").and_then(|v| v.as_str()) {
                action_value = Some(av.to_string());
            }
        }

        results.push(RuleWithDetails {
            rule_node,
            match_type,
            match_value,
            action_type,
            action_value,
        });
    }

    Ok(results)
}

/// Convert graph rules and VIP senders into a `RulesConfig` for the triage engine.
pub fn graph_rules_to_config(conn: &Connection) -> GraphResult<RulesConfig> {
    let graph_rules = get_all_rules(conn)?;
    let vip_emails = get_vip_emails(conn)?;

    let mut rules = Vec::with_capacity(graph_rules.len());
    for r in &graph_rules {
        let match_criteria = match (r.match_type.as_deref(), r.match_value.as_deref()) {
            (Some("sender"), Some(val)) => MatchCriteria {
                sender_contains: Some(val.to_string()),
                ..Default::default()
            },
            (Some("subject"), Some(val)) => MatchCriteria {
                subject_contains: Some(val.to_string()),
                ..Default::default()
            },
            _ => continue,
        };

        // Determine action from metadata or edge context
        let (action_type, label_number) = parse_graph_action(
            r.action_type.as_deref(),
            r.action_value.as_deref(),
            &r.rule_node.metadata,
        );

        rules.push(Rule {
            name: r.rule_node.name.clone(),
            match_criteria,
            action: Action {
                action_type,
                label_number,
            },
        });
    }

    let vip_senders: Vec<VipSender> = vip_emails
        .into_iter()
        .map(|addr| {
            // Try to find a name for this VIP
            let name = conn
                .query_row(
                    "SELECT name FROM nodes WHERE email = ?1 AND is_vip = 1",
                    [&addr],
                    |row| row.get::<_, String>(0),
                )
                .ok();
            VipSender {
                address: addr,
                name,
            }
        })
        .collect();

    Ok(RulesConfig { rules, vip_senders })
}

/// Parse action type and label number from graph rule metadata.
fn parse_graph_action(
    action_type: Option<&str>,
    action_value: Option<&str>,
    metadata: &str,
) -> (ActionType, Option<u8>) {
    // Try action_type/action_value from edges first
    if let Some(at) = action_type {
        return match at {
            "trash" => (ActionType::Trash, None),
            "archive" => (ActionType::Archive, None),
            s if s.starts_with("label") => {
                let num = action_value
                    .and_then(|v| v.trim_start_matches("label:").parse::<u8>().ok())
                    .or_else(|| s.strip_prefix("label:").and_then(|n| n.parse().ok()));
                (ActionType::Label, num)
            }
            _ => {
                // Try parsing from metadata
                parse_action_from_metadata(metadata)
            }
        };
    }
    parse_action_from_metadata(metadata)
}

fn parse_action_from_metadata(metadata: &str) -> (ActionType, Option<u8>) {
    if let Ok(meta) = serde_json::from_str::<serde_json::Value>(metadata) {
        let at = meta
            .get("action_type")
            .and_then(|v| v.as_str())
            .unwrap_or("label");
        let av = meta.get("action_value").and_then(|v| v.as_str());
        match at {
            "trash" => return (ActionType::Trash, None),
            "archive" => return (ActionType::Archive, None),
            "label" => {
                let num = av.and_then(|v| v.parse::<u8>().ok());
                return (ActionType::Label, num);
            }
            _ => {}
        }
    }
    (ActionType::Label, Some(1)) // default fallback
}

// ---------------------------------------------------------------------------
// Convenience: add_vip
// ---------------------------------------------------------------------------

pub fn add_vip(
    conn: &Connection,
    name: &str,
    email: &str,
    description: Option<&str>,
    context: Option<&str>,
) -> GraphResult<i64> {
    let person_id = add_node(conn, "person", name, Some(email), description, None, true)?;

    let rule_name = format!("VIP: {name}");
    let rule_meta = serde_json::json!({
        "action_type": "label",
        "action_value": "1",
    })
    .to_string();
    let rule_id = add_node(
        conn,
        "rule",
        &rule_name,
        None,
        Some("Auto-generated VIP rule"),
        Some(&rule_meta),
        false,
    )?;

    add_edge(conn, rule_id, person_id, "matches_sender", context, None)?;
    add_edge(
        conn,
        rule_id,
        person_id,
        "applies_action",
        Some("label:1"),
        None,
    )?;
    add_edge(conn, rule_id, person_id, "protects", None, None)?;

    Ok(person_id)
}

// ---------------------------------------------------------------------------
// Convenience: add_rule
// ---------------------------------------------------------------------------

pub fn add_rule(
    conn: &Connection,
    name: &str,
    match_type: &str,
    match_value: &str,
    action_type: &str,
    action_value: &str,
) -> GraphResult<i64> {
    let rule_meta = serde_json::json!({
        "action_type": action_type,
        "action_value": action_value,
    })
    .to_string();

    let rule_id = add_node(conn, "rule", name, None, None, Some(&rule_meta), false)?;

    // Create or find the match target
    match match_type {
        "sender" => {
            // Try to find existing person node by email
            let target_id = match conn.query_row(
                "SELECT id FROM nodes WHERE email = ?1",
                [match_value],
                |r| r.get::<_, i64>(0),
            ) {
                Ok(id) => id,
                Err(_) => add_node(
                    conn,
                    "person",
                    match_value,
                    Some(match_value),
                    None,
                    None,
                    false,
                )?,
            };
            add_edge(conn, rule_id, target_id, "matches_sender", None, None)?;
        }
        "subject" => {
            let target_id = add_node(conn, "topic", match_value, None, None, None, false)?;
            add_edge(conn, rule_id, target_id, "matches_subject", None, None)?;
        }
        _ => {}
    }

    Ok(rule_id)
}

// ---------------------------------------------------------------------------
// Project & Task management
// ---------------------------------------------------------------------------

/// Create a project node with status="active" in metadata.
pub fn add_project(conn: &Connection, name: &str, description: Option<&str>) -> GraphResult<i64> {
    let meta = serde_json::json!({"status": "active"}).to_string();
    add_node(conn, "project", name, None, description, Some(&meta), false)
}

/// Create a task node. Optionally link to a project via "belongs_to" edge.
pub fn add_task(
    conn: &Connection,
    title: &str,
    description: Option<&str>,
    due_date: Option<&str>,
    project_id: Option<i64>,
) -> GraphResult<i64> {
    let mut meta = serde_json::json!({"status": "todo"});
    if let Some(due) = due_date {
        meta["due_date"] = serde_json::Value::String(due.to_string());
    }
    let task_id = add_node(
        conn,
        "task",
        title,
        None,
        description,
        Some(&meta.to_string()),
        false,
    )?;

    if let Some(pid) = project_id {
        add_edge(conn, task_id, pid, "belongs_to", None, None)?;
    }

    Ok(task_id)
}

/// Update a task's status field in metadata. Valid: todo, in_progress, done, blocked.
pub fn update_task_status(conn: &Connection, task_id: i64, status: &str) -> GraphResult<()> {
    let node = get_node(conn, task_id)?;
    let mut meta: serde_json::Value =
        serde_json::from_str(&node.metadata).unwrap_or(serde_json::json!({}));
    meta["status"] = serde_json::Value::String(status.to_string());
    update_node(
        conn,
        task_id,
        None,
        None,
        None,
        Some(&meta.to_string()),
        None,
    )
}

/// List tasks, optionally filtered by project and/or status.
pub fn list_tasks(
    conn: &Connection,
    project_id: Option<i64>,
    status: Option<&str>,
) -> GraphResult<Vec<Node>> {
    let all_tasks = list_nodes(conn, Some("task"), false)?;

    let filtered: Vec<Node> = all_tasks
        .into_iter()
        .filter(|t| {
            // Filter by status if specified
            if let Some(s) = status {
                let meta: serde_json::Value =
                    serde_json::from_str(&t.metadata).unwrap_or(serde_json::json!({}));
                if meta.get("status").and_then(|v| v.as_str()) != Some(s) {
                    return false;
                }
            }
            true
        })
        .filter(|t| {
            // Filter by project if specified
            if let Some(pid) = project_id {
                let edges = get_edges_raw(conn, t.id, Some("belongs_to")).unwrap_or_default();
                return edges.iter().any(|(_, neighbor_id)| *neighbor_id == pid);
            }
            true
        })
        .collect();

    Ok(filtered)
}

/// List projects, optionally only active ones.
pub fn list_projects(conn: &Connection, active_only: bool) -> GraphResult<Vec<Node>> {
    let all_projects = list_nodes(conn, Some("project"), false)?;
    if !active_only {
        return Ok(all_projects);
    }
    Ok(all_projects
        .into_iter()
        .filter(|p| {
            let meta: serde_json::Value =
                serde_json::from_str(&p.metadata).unwrap_or(serde_json::json!({}));
            meta.get("status").and_then(|v| v.as_str()) == Some("active")
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Dump
// ---------------------------------------------------------------------------

pub fn dump_context(conn: &Connection) -> GraphResult<String> {
    let mut out = String::from("# Graph Context\n\n");
    out.push_str(&format!("Generated: {}\n\n", Utc::now().to_rfc3339()));

    // VIP senders
    let vips = list_nodes(conn, None, true)?;
    if !vips.is_empty() {
        out.push_str("## VIP Senders\n\n");
        for v in &vips {
            let email_str = v
                .email
                .as_deref()
                .map(|e| format!(" <{e}>"))
                .unwrap_or_default();
            let desc_str = v
                .description
                .as_deref()
                .map(|d| format!(" — {d}"))
                .unwrap_or_default();
            out.push_str(&format!("- **{}**{}{}\n", v.name, email_str, desc_str));
        }
        out.push('\n');
    }

    // All nodes by type
    let all_nodes = list_nodes(conn, None, false)?;
    let mut types: Vec<String> = all_nodes.iter().map(|n| n.node_type.clone()).collect();
    types.sort();
    types.dedup();

    for nt in &types {
        let nodes: Vec<&Node> = all_nodes.iter().filter(|n| &n.node_type == nt).collect();
        out.push_str(&format!("## {} ({} nodes)\n\n", nt, nodes.len()));
        for n in &nodes {
            let email_str = n
                .email
                .as_deref()
                .map(|e| format!(" <{e}>"))
                .unwrap_or_default();
            let vip_str = if n.is_vip { " [VIP]" } else { "" };
            out.push_str(&format!(
                "- [{}] **{}**{}{}\n",
                n.id, n.name, email_str, vip_str
            ));
        }
        out.push('\n');
    }

    // Edges
    let mut edge_stmt = conn.prepare(
        "SELECT e.id, e.source_id, e.target_id, e.predicate, e.context, e.weight, e.created_at
         FROM edges e ORDER BY e.predicate, e.id",
    )?;
    let edges: Vec<Edge> = edge_stmt
        .query_map([], row_to_edge)?
        .filter_map(|r| r.ok())
        .collect();

    if !edges.is_empty() {
        out.push_str("## Relationships\n\n");
        for e in &edges {
            let source_name = get_node(conn, e.source_id)
                .map(|n| n.name)
                .unwrap_or_else(|_| format!("#{}", e.source_id));
            let target_name = get_node(conn, e.target_id)
                .map(|n| n.name)
                .unwrap_or_else(|_| format!("#{}", e.target_id));
            let ctx = e
                .context
                .as_deref()
                .map(|c| format!(" ({c})"))
                .unwrap_or_default();
            out.push_str(&format!(
                "- {} --[{}]--> {}{}\n",
                source_name, e.predicate, target_name, ctx
            ));
        }
        out.push('\n');
    }

    Ok(out)
}

/// Write the graph context dump to ~/.mea/GRAPH_CONTEXT.md
pub fn auto_dump(conn: &Connection) -> GraphResult<()> {
    let content = dump_context(conn)?;
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let path = std::path::PathBuf::from(home)
        .join(".mea")
        .join("GRAPH_CONTEXT.md");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, content);
    Ok(())
}

// ---------------------------------------------------------------------------
// Row mappers
// ---------------------------------------------------------------------------

fn row_to_node(row: &rusqlite::Row) -> rusqlite::Result<Node> {
    Ok(Node {
        id: row.get(0)?,
        node_type: row.get(1)?,
        name: row.get(2)?,
        email: row.get(3)?,
        description: row.get(4)?,
        metadata: row.get::<_, String>(5).unwrap_or_else(|_| "{}".to_string()),
        is_vip: row.get::<_, i32>(6).unwrap_or(0) != 0,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

fn row_to_edge(row: &rusqlite::Row) -> rusqlite::Result<Edge> {
    Ok(Edge {
        id: row.get(0)?,
        source_id: row.get(1)?,
        target_id: row.get(2)?,
        predicate: row.get(3)?,
        context: row.get(4)?,
        weight: row.get::<_, f64>(5).unwrap_or(1.0),
        created_at: row.get(6)?,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_overlay_db_memory;

    #[test]
    fn test_add_and_get_node() {
        let conn = open_overlay_db_memory().unwrap();
        let id = add_node(
            &conn,
            "person",
            "Alice",
            Some("alice@test.com"),
            Some("Engineer"),
            None,
            false,
        )
        .unwrap();
        let node = get_node(&conn, id).unwrap();
        assert_eq!(node.name, "Alice");
        assert_eq!(node.email.as_deref(), Some("alice@test.com"));
        assert_eq!(node.description.as_deref(), Some("Engineer"));
        assert!(!node.is_vip);
    }

    #[test]
    fn test_add_edge_between_nodes() {
        let conn = open_overlay_db_memory().unwrap();
        let a = add_node(&conn, "person", "Alice", Some("a@t"), None, None, false).unwrap();
        let b = add_node(&conn, "team", "Engineering", None, None, None, false).unwrap();
        let eid = add_edge(&conn, a, b, "member_of", Some("core team"), None).unwrap();
        assert!(eid > 0);

        let edges = get_edges(&conn, a, None).unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].edge.predicate, "member_of");
        assert_eq!(edges[0].target.name, "Engineering");
    }

    #[test]
    fn test_find_nodes_by_name_email() {
        let conn = open_overlay_db_memory().unwrap();
        add_node(
            &conn,
            "person",
            "Alice Smith",
            Some("alice@corp.com"),
            None,
            None,
            false,
        )
        .unwrap();
        add_node(
            &conn,
            "person",
            "Bob Jones",
            Some("bob@corp.com"),
            None,
            None,
            false,
        )
        .unwrap();

        let found = find_nodes(&conn, "alice").unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "Alice Smith");

        let found = find_nodes(&conn, "corp.com").unwrap();
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn test_list_nodes_filtered_by_type_and_vip() {
        let conn = open_overlay_db_memory().unwrap();
        add_node(&conn, "person", "Alice", Some("a@t"), None, None, true).unwrap();
        add_node(&conn, "person", "Bob", Some("b@t"), None, None, false).unwrap();
        add_node(&conn, "team", "Eng", None, None, None, false).unwrap();

        let all = list_nodes(&conn, None, false).unwrap();
        assert_eq!(all.len(), 3);

        let persons = list_nodes(&conn, Some("person"), false).unwrap();
        assert_eq!(persons.len(), 2);

        let vips = list_nodes(&conn, None, true).unwrap();
        assert_eq!(vips.len(), 1);
        assert_eq!(vips[0].name, "Alice");

        let vip_persons = list_nodes(&conn, Some("person"), true).unwrap();
        assert_eq!(vip_persons.len(), 1);
    }

    #[test]
    fn test_remove_node_cascades_edges() {
        let conn = open_overlay_db_memory().unwrap();
        let a = add_node(&conn, "person", "Alice", None, None, None, false).unwrap();
        let b = add_node(&conn, "team", "Eng", None, None, None, false).unwrap();
        add_edge(&conn, a, b, "member_of", None, None).unwrap();

        remove_node(&conn, a).unwrap();

        // Node gone
        assert!(get_node(&conn, a).is_err());
        // Edges gone
        let edges = get_edges(&conn, b, None).unwrap();
        assert!(edges.is_empty());
    }

    #[test]
    fn test_add_vip_creates_node_rule_edges() {
        let conn = open_overlay_db_memory().unwrap();
        let person_id =
            add_vip(&conn, "Boss", "boss@co.com", Some("CEO"), Some("important")).unwrap();

        let person = get_node(&conn, person_id).unwrap();
        assert!(person.is_vip);
        assert_eq!(person.email.as_deref(), Some("boss@co.com"));

        let edges = get_edges(&conn, person_id, None).unwrap();
        assert_eq!(edges.len(), 3); // matches_sender, applies_action, protects

        let predicates: Vec<&str> = edges.iter().map(|e| e.edge.predicate.as_str()).collect();
        assert!(predicates.contains(&"matches_sender"));
        assert!(predicates.contains(&"applies_action"));
        assert!(predicates.contains(&"protects"));
    }

    #[test]
    fn test_add_rule_creates_correct_structure() {
        let conn = open_overlay_db_memory().unwrap();
        let rule_id =
            add_rule(&conn, "Trash spam", "sender", "spam@junk.com", "trash", "").unwrap();

        let rule = get_node(&conn, rule_id).unwrap();
        assert_eq!(rule.node_type, "rule");

        let edges = get_edges(&conn, rule_id, None).unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].edge.predicate, "matches_sender");
    }

    #[test]
    fn test_get_sender_context_returns_correct_data() {
        let conn = open_overlay_db_memory().unwrap();
        add_vip(&conn, "Boss", "boss@co.com", Some("CEO"), None).unwrap();

        let ctx = get_sender_context(&conn, "boss@co.com").unwrap().unwrap();
        assert!(ctx.is_vip);
        assert_eq!(ctx.description.as_deref(), Some("CEO"));
        assert!(!ctx.edges.is_empty());
        assert!(!ctx.rules.is_empty());
    }

    #[test]
    fn test_get_sender_context_unknown_returns_none() {
        let conn = open_overlay_db_memory().unwrap();
        let ctx = get_sender_context(&conn, "unknown@test.com").unwrap();
        assert!(ctx.is_none());
    }

    #[test]
    fn test_get_vip_emails() {
        let conn = open_overlay_db_memory().unwrap();
        add_vip(&conn, "Boss", "boss@co.com", None, None).unwrap();
        add_node(
            &conn,
            "person",
            "Regular",
            Some("reg@co.com"),
            None,
            None,
            false,
        )
        .unwrap();

        let vips = get_vip_emails(&conn).unwrap();
        assert_eq!(vips.len(), 1);
        assert_eq!(vips[0], "boss@co.com");
    }

    #[test]
    fn test_traverse_follows_edges_to_correct_depth() {
        let conn = open_overlay_db_memory().unwrap();
        let a = add_node(&conn, "person", "A", None, None, None, false).unwrap();
        let b = add_node(&conn, "team", "B", None, None, None, false).unwrap();
        let c = add_node(&conn, "project", "C", None, None, None, false).unwrap();
        add_edge(&conn, a, b, "member_of", None, None).unwrap();
        add_edge(&conn, b, c, "owns", None, None).unwrap();

        // Depth 1: should reach B but not C
        let results = traverse(&conn, a, None, 1).unwrap();
        assert_eq!(results.len(), 2); // A + B
        assert_eq!(results[0].depth, 0);
        assert_eq!(results[1].depth, 1);

        // Depth 2: should reach C
        let results = traverse(&conn, a, None, 2).unwrap();
        assert_eq!(results.len(), 3); // A + B + C
        assert_eq!(results[2].depth, 2);
        assert_eq!(results[2].path.len(), 2);
    }

    #[test]
    fn test_traverse_with_predicate_filter() {
        let conn = open_overlay_db_memory().unwrap();
        let a = add_node(&conn, "person", "A", None, None, None, false).unwrap();
        let b = add_node(&conn, "team", "B", None, None, None, false).unwrap();
        let c = add_node(&conn, "project", "C", None, None, None, false).unwrap();
        add_edge(&conn, a, b, "member_of", None, None).unwrap();
        add_edge(&conn, a, c, "owns", None, None).unwrap();

        let results = traverse(&conn, a, Some("member_of"), 2).unwrap();
        assert_eq!(results.len(), 2); // A + B only
    }

    #[test]
    fn test_dump_context_produces_readable_markdown() {
        let conn = open_overlay_db_memory().unwrap();
        add_vip(&conn, "Boss", "boss@co.com", Some("CEO"), None).unwrap();
        add_rule(&conn, "Spam filter", "sender", "spam@junk.com", "trash", "").unwrap();

        let md = dump_context(&conn).unwrap();
        assert!(md.contains("# Graph Context"));
        assert!(md.contains("## VIP Senders"));
        assert!(md.contains("Boss"));
        assert!(md.contains("boss@co.com"));
        assert!(md.contains("## Relationships"));
    }

    #[test]
    fn test_update_node() {
        let conn = open_overlay_db_memory().unwrap();
        let id = add_node(&conn, "person", "Alice", None, None, None, false).unwrap();

        update_node(
            &conn,
            id,
            Some("Alice Updated"),
            None,
            Some("New desc"),
            None,
            Some(true),
        )
        .unwrap();
        let node = get_node(&conn, id).unwrap();
        assert_eq!(node.name, "Alice Updated");
        assert_eq!(node.description.as_deref(), Some("New desc"));
        assert!(node.is_vip);
    }

    #[test]
    fn test_remove_edge() {
        let conn = open_overlay_db_memory().unwrap();
        let a = add_node(&conn, "person", "A", None, None, None, false).unwrap();
        let b = add_node(&conn, "team", "B", None, None, None, false).unwrap();
        let eid = add_edge(&conn, a, b, "member_of", None, None).unwrap();

        remove_edge(&conn, eid).unwrap();
        let edges = get_edges(&conn, a, None).unwrap();
        assert!(edges.is_empty());
    }

    #[test]
    fn test_remove_edge_not_found() {
        let conn = open_overlay_db_memory().unwrap();
        assert!(remove_edge(&conn, 9999).is_err());
    }

    #[test]
    fn test_get_all_rules() {
        let conn = open_overlay_db_memory().unwrap();
        add_rule(&conn, "Rule A", "sender", "a@t.com", "label", "1").unwrap();
        add_rule(&conn, "Rule B", "subject", "newsletter", "trash", "").unwrap();

        let rules = get_all_rules(&conn).unwrap();
        assert_eq!(rules.len(), 2);
    }
}
