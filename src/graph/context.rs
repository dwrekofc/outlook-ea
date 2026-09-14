use super::{GraphResult, Node, SenderContext, get_edges, list_nodes, node_sql, row_to_node};
use crate::db::Store;
use libsql::params;

// D16 preserves secondary addresses in metadata.emails when people are merged.
pub(super) fn sender_node(store: &Store, email: &str) -> GraphResult<Option<Node>> {
    if email.is_empty() {
        return Ok(None);
    }
    Ok(store.one(&node_sql("WHERE node_type != 'email' AND (email = ?1 COLLATE NOCASE OR EXISTS (SELECT 1 FROM json_each(metadata, '$.emails') WHERE value = ?1 COLLATE NOCASE)) ORDER BY email = ?1 DESC, id"), params![email], row_to_node)?)
}

pub fn get_sender_context(store: &Store, email: &str) -> GraphResult<Option<SenderContext>> {
    let Some(node) = sender_node(store, email)? else {
        return Ok(None);
    };
    let edges = get_edges(store, node.id, None)?;
    let mut summaries = Vec::new();
    let mut rules = Vec::new();
    for edge in edges {
        let other = if edge.edge.source_id == node.id {
            edge.target
        } else {
            edge.source
        };
        summaries.push(format!("{} -> {}", edge.edge.predicate, other.name));
        if other.node_type == "rule" {
            rules.push(other.name);
        }
    }
    Ok(Some(SenderContext {
        node_id: node.id,
        is_vip: node.is_vip,
        description: node.description,
        edges: summaries,
        rules,
    }))
}

pub fn get_vip_emails(store: &Store) -> GraphResult<Vec<String>> {
    let mut emails = Vec::<String>::new();
    for node in list_nodes(store, None, true)? {
        let metadata: serde_json::Value = serde_json::from_str(&node.metadata)?;
        let aliases = metadata
            .get("emails")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str);
        for email in node.email.as_deref().into_iter().chain(aliases) {
            if !emails
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(email))
            {
                emails.push(email.to_owned());
            }
        }
    }
    Ok(emails)
}
