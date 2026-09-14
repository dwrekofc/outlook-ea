use libsql::params;
use serde_json::{Value, json};

use super::{
    Edge, GraphResult, Node, add_edge, add_node, edge_sql, get_node, list_nodes, row_to_edge,
    status_of, update_node,
};
use crate::db::Store;

pub fn add_project(store: &Store, name: &str, description: Option<&str>) -> GraphResult<i64> {
    let meta = json!({"status": "active"}).to_string();
    add_node(
        store,
        "project",
        name,
        None,
        description,
        Some(&meta),
        false,
    )
}

pub fn add_task(
    store: &Store,
    title: &str,
    description: Option<&str>,
    due_date: Option<&str>,
    project_id: Option<i64>,
) -> GraphResult<i64> {
    if let Some(project) = project_id {
        get_node(store, project)?;
    }
    let mut meta = json!({"status": "todo"});
    if let Some(due) = due_date {
        meta["due_date"] = Value::String(due.to_string());
    }
    let task_id = add_node(
        store,
        "task",
        title,
        None,
        description,
        Some(&meta.to_string()),
        false,
    )?;
    if let Some(project) = project_id {
        add_edge(store, task_id, project, "belongs_to", None, None)?;
    }
    Ok(task_id)
}

pub fn update_task_status(store: &Store, task_id: i64, status: &str) -> GraphResult<()> {
    let mut meta: Value = serde_json::from_str(&get_node(store, task_id)?.metadata)?;
    meta["status"] = Value::String(status.to_string());
    update_node(
        store,
        task_id,
        None,
        None,
        None,
        Some(&meta.to_string()),
        None,
    )
}

pub fn list_tasks(
    store: &Store,
    project_id: Option<i64>,
    status: Option<&str>,
) -> GraphResult<Vec<Node>> {
    let mut tasks = list_nodes(store, Some("task"), false)?;
    if let Some(expected) = status {
        tasks.retain(|task| status_of(task).as_deref() == Some(expected));
    }
    if let Some(project) = project_id {
        let mut matching = Vec::new();
        for task in tasks {
            if belongs_to_project(store, task.id, project)? {
                matching.push(task);
            }
        }
        tasks = matching;
    }
    Ok(tasks)
}

pub fn list_projects(store: &Store, active_only: bool) -> GraphResult<Vec<Node>> {
    let mut projects = list_nodes(store, Some("project"), false)?;
    if active_only {
        projects.retain(|project| status_of(project).as_deref() == Some("active"));
    }
    Ok(projects)
}

pub fn dump_context(store: &Store) -> GraphResult<String> {
    let mut out = String::from("# Graph Context\n\n");
    out.push_str(&format!(
        "Generated: {}\n\n",
        chrono::Utc::now().to_rfc3339()
    ));
    let vips = list_nodes(store, None, true)?;
    if !vips.is_empty() {
        out.push_str("## VIP Senders\n\n");
        for vip in &vips {
            let email = vip
                .email
                .as_deref()
                .map(|value| format!(" <{value}>"))
                .unwrap_or_default();
            let desc = vip
                .description
                .as_deref()
                .map(|value| format!(" - {value}"))
                .unwrap_or_default();
            out.push_str(&format!("- **{}**{}{}\n", vip.name, email, desc));
        }
        out.push('\n');
    }
    append_nodes(store, &mut out)?;
    append_edges(store, &mut out)?;
    Ok(out)
}

pub fn auto_dump(store: &Store) -> GraphResult<()> {
    let content = dump_context(store)?;
    let path = std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| ".".into())
        .join(".mea")
        .join("GRAPH_CONTEXT.md");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, content);
    Ok(())
}

fn append_nodes(store: &Store, out: &mut String) -> GraphResult<()> {
    let all = list_nodes(store, None, false)?;
    let mut types: Vec<_> = all.iter().map(|node| node.node_type.clone()).collect();
    types.sort();
    types.dedup();
    for node_type in types {
        let nodes: Vec<_> = all
            .iter()
            .filter(|node| node.node_type == node_type)
            .collect();
        out.push_str(&format!("## {} ({} nodes)\n\n", node_type, nodes.len()));
        for node in nodes {
            let email = node
                .email
                .as_deref()
                .map(|value| format!(" <{value}>"))
                .unwrap_or_default();
            let vip = if node.is_vip { " [VIP]" } else { "" };
            out.push_str(&format!(
                "- [{}] **{}**{}{}\n",
                node.id, node.name, email, vip
            ));
        }
        out.push('\n');
    }
    Ok(())
}

fn append_edges(store: &Store, out: &mut String) -> GraphResult<()> {
    let edges = store.all(&edge_sql("ORDER BY predicate, id"), (), row_to_edge)?;
    if edges.is_empty() {
        return Ok(());
    }
    out.push_str("## Relationships\n\n");
    let names = list_nodes(store, None, false)?
        .into_iter()
        .map(|n| (n.id, n.name))
        .collect::<std::collections::HashMap<_, _>>();
    for edge in edges {
        out.push_str(&relationship_line(&names, &edge));
    }
    out.push('\n');
    Ok(())
}

fn relationship_line(names: &std::collections::HashMap<i64, String>, edge: &Edge) -> String {
    let source = names
        .get(&edge.source_id)
        .cloned()
        .unwrap_or_else(|| format!("#{}", edge.source_id));
    let target = names
        .get(&edge.target_id)
        .cloned()
        .unwrap_or_else(|| format!("#{}", edge.target_id));
    let context = edge
        .context
        .as_deref()
        .map(|value| format!(" ({value})"))
        .unwrap_or_default();
    format!("- {source} --[{}]--> {target}{context}\n", edge.predicate)
}

fn belongs_to_project(store: &Store, task_id: i64, project_id: i64) -> GraphResult<bool> {
    Ok(store
        .one(
            "SELECT 1 FROM graph_edges
             WHERE source_id = ?1 AND target_id = ?2 AND predicate = 'belongs_to'",
            params![task_id, project_id],
            |_| Ok(1_i64),
        )?
        .is_some())
}
