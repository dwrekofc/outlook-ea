mod context;
pub use context::{get_sender_context, get_vip_emails};
mod model;
mod predicates;
use predicates::canonical_predicate;
mod rule_helpers;
use chrono::Utc;
use libsql::params;
use serde_json::Value;

use crate::db::Store;
pub use crate::graph::model::{
    Edge, EdgeWithNodes, GraphError, GraphResult, Node, RuleWithDetails, SenderContext,
    TraversalResult,
};

#[path = "graph/edges_rules.rs"]
mod ext;
pub use ext::*;
#[path = "graph/tasks.rs"]
mod tasks;
pub use tasks::*;

pub fn add_node(
    store: &Store,
    node_type: &str,
    name: &str,
    email: Option<&str>,
    description: Option<&str>,
    metadata: Option<&str>,
    is_vip: bool,
) -> GraphResult<i64> {
    store.transaction(|store| {
        let now = Utc::now().to_rfc3339();
        let meta = normalize_metadata(metadata)?;
        let id = store
            .one(
                "INSERT INTO graph_nodes
             (node_type,name,email,description,status,due_date,metadata,is_vip,archived,
              created_at,updated_at,profile)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,0,?9,?9,'mea')
             RETURNING id",
                params![
                    node_type,
                    name,
                    email,
                    description,
                    scalar(&meta, "status"),
                    scalar(&meta, "due_date"),
                    meta.to_string(),
                    is_vip as i64,
                    now
                ],
                |row| Ok(row.get::<i64>(0)?),
            )?
            .expect("INSERT RETURNING yielded no row");
        record_history(store, Some(id), "node_created", name)?;
        Ok(id)
    })
}

pub fn get_node(store: &Store, id: i64) -> GraphResult<Node> {
    store
        .one(&node_sql("WHERE id = ?1"), params![id], row_to_node)?
        .ok_or(GraphError::NodeNotFound(id))
}

pub fn find_nodes(store: &Store, query: &str) -> GraphResult<Vec<Node>> {
    let pattern = format!("%{query}%");
    Ok(store.all(
        &node_sql("WHERE name LIKE ?1 OR email LIKE ?1 OR description LIKE ?1 ORDER BY name"),
        params![pattern],
        row_to_node,
    )?)
}

pub fn list_nodes(
    store: &Store,
    node_type: Option<&str>,
    vip_only: bool,
) -> GraphResult<Vec<Node>> {
    let where_sql = match (node_type, vip_only) {
        (Some(_), true) => "WHERE node_type = ?1 AND is_vip = 1 ORDER BY name",
        (Some(_), false) => "WHERE node_type = ?1 ORDER BY name",
        (None, true) => "WHERE is_vip = 1 ORDER BY name",
        (None, false) => "ORDER BY name",
    };
    let rows = match node_type {
        Some(nt) => store.all(&node_sql(where_sql), params![nt], row_to_node)?,
        None => store.all(&node_sql(where_sql), (), row_to_node)?,
    };
    Ok(rows)
}

pub fn update_node(
    store: &Store,
    id: i64,
    name: Option<&str>,
    email: Option<&str>,
    description: Option<&str>,
    metadata: Option<&str>,
    is_vip: Option<bool>,
) -> GraphResult<()> {
    store.transaction(|store| {
        let current = node_record(store, id)?;
        let meta = match metadata {
            Some(value) => merge_metadata(&current.metadata, value)?,
            None => current.metadata,
        };
        store.execute(
            "UPDATE graph_nodes SET
            name = COALESCE(?1, name),
            email = COALESCE(?2, email),
            description = COALESCE(?3, description),
            metadata = ?4,
            status = COALESCE(?5, status),
            due_date = COALESCE(?6, due_date),
            is_vip = COALESCE(?7, is_vip),
            updated_at = ?8
         WHERE id = ?9",
            params![
                name,
                email,
                description,
                meta.to_string(),
                scalar(&meta, "status"),
                scalar(&meta, "due_date"),
                is_vip.map(|value| value as i64),
                Utc::now().to_rfc3339(),
                id
            ],
        )?;
        record_history(store, Some(id), "node_updated", &id.to_string())
    })
}

pub fn remove_node(store: &Store, id: i64) -> GraphResult<()> {
    store.transaction(|store| {
        get_node(store, id)?;
        store.execute("DELETE FROM graph_nodes WHERE id = ?1", params![id])?;
        record_history(store, None, "node_removed", &id.to_string())
    })
}

pub fn add_edge(
    store: &Store,
    source_id: i64,
    target_id: i64,
    predicate: &str,
    context: Option<&str>,
    weight: Option<f64>,
) -> GraphResult<i64> {
    if predicate.contains('_') || predicate.chars().any(char::is_uppercase) {
        return Err(GraphError::InvalidPredicate(predicate.to_owned()));
    }
    store.transaction(|store| {
        get_node(store, source_id)?;
        get_node(store, target_id)?;
        let id = store
            .one(
                "INSERT INTO graph_edges
             (source_id,target_id,predicate,context,weight,created_at,metadata)
             VALUES (?1,?2,?3,?4,?5,?6,'{}') RETURNING id",
                params![
                    source_id,
                    target_id,
                    predicate,
                    context,
                    weight.unwrap_or(1.0),
                    Utc::now().to_rfc3339()
                ],
                |row| Ok(row.get::<i64>(0)?),
            )?
            .expect("INSERT RETURNING yielded no row");
        record_history(store, Some(source_id), "edge_created", predicate)?;
        Ok(id)
    })
}

fn node_sql(tail: &str) -> String {
    format!(
        "SELECT id,node_type,name,email,description,metadata,is_vip,created_at,updated_at,status,due_date,profile,archived
         FROM (SELECT * FROM graph_nodes WHERE archived=0) {tail}"
    )
}

fn row_to_node(row: &libsql::Row) -> anyhow::Result<Node> {
    let mut metadata: Value = serde_json::from_str(&row.get::<String>(5)?)?;
    for (column, key) in [(9, "status"), (10, "due_date")] {
        if let Some(value) = row.get::<Option<String>>(column)? {
            metadata[key] = Value::String(value);
        }
    }
    Ok(Node {
        id: row.get(0)?,
        node_type: row.get(1)?,
        name: row.get(2)?,
        email: row.get(3)?,
        description: row.get(4)?,
        metadata: metadata.to_string(),
        is_vip: row.get::<i64>(6)? != 0,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        profile: row.get(11)?,
        archived: row.get::<i64>(12)? != 0,
    })
}

fn row_to_edge(row: &libsql::Row) -> anyhow::Result<Edge> {
    Ok(Edge {
        id: row.get(0)?,
        source_id: row.get(1)?,
        target_id: row.get(2)?,
        predicate: canonical_predicate(&row.get::<String>(3)?).into_owned(),
        context: row.get(4)?,
        weight: row.get::<Option<f64>>(5)?.unwrap_or(1.0),
        created_at: row.get(6)?,
    })
}

fn edge_sql(tail: &str) -> String {
    format!(
        "SELECT id,source_id,target_id,predicate,context,weight,created_at
         FROM (SELECT * FROM graph_edges WHERE source_id IN (SELECT id FROM graph_nodes WHERE archived=0) AND target_id IN (SELECT id FROM graph_nodes WHERE archived=0)) {tail}"
    )
}

struct NodeRecord {
    metadata: Value,
}

fn node_record(store: &Store, id: i64) -> GraphResult<NodeRecord> {
    Ok(NodeRecord {
        metadata: serde_json::from_str(&get_node(store, id)?.metadata)?,
    })
}

fn normalize_metadata(metadata: Option<&str>) -> GraphResult<Value> {
    let value: Value = serde_json::from_str(metadata.unwrap_or("{}"))?;
    if !value.is_object() {
        return Err(anyhow::anyhow!("metadata must be a JSON object").into());
    }
    Ok(value)
}

fn merge_metadata(existing: &Value, incoming: &str) -> GraphResult<Value> {
    let mut next = normalize_metadata(Some(incoming))?;
    for key in ["sources", "source_records", "merged_nodes"] {
        if let Some(value) = existing.get(key) {
            next[key] = value.clone();
        }
    }
    Ok(next)
}

fn scalar(metadata: &Value, key: &str) -> Option<String> {
    metadata
        .get(key)
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn status_of(node: &Node) -> Option<String> {
    store_field(&node.metadata, "status")
}

fn store_field(metadata: &str, key: &str) -> Option<String> {
    serde_json::from_str::<Value>(metadata)
        .ok()
        .and_then(|value| scalar(&value, key))
}

fn record_history(
    store: &Store,
    node_id: Option<i64>,
    event: &str,
    detail: &str,
) -> GraphResult<()> {
    store.execute(
        "INSERT INTO graph_history (ts,node_id,event,detail,profile)
         VALUES (?1,?2,?3,?4,'mea')",
        params![Utc::now().to_rfc3339(), node_id, event, detail],
    )?;
    Ok(())
}

#[cfg(test)]
mod regression_tests;

#[cfg(test)]
mod migration_tests;

#[cfg(test)]
mod storage_tests;
