use std::collections::HashSet;

use libsql::params;
use serde_json::json;

use super::{
    Edge, EdgeWithNodes, GraphError, GraphResult, Node, RuleWithDetails, TraversalResult, add_edge,
    add_node, edge_sql, get_node, get_vip_emails, record_history, row_to_edge, row_to_node,
};
use crate::db::Store;
use crate::graph::rule_helpers::{
    fill_action_from_metadata, parse_action, person_for_sender, vip_name,
};
use crate::rules::{Action, MatchCriteria, Rule, RulesConfig, VipSender};

pub fn get_edges(
    store: &Store,
    node_id: i64,
    predicate: Option<&str>,
) -> GraphResult<Vec<EdgeWithNodes>> {
    let edges = raw_edges(store, node_id, predicate)?;
    edges
        .into_iter()
        .map(|edge| {
            Ok(EdgeWithNodes {
                source: get_node(store, edge.source_id)?,
                target: get_node(store, edge.target_id)?,
                edge,
            })
        })
        .collect()
}

pub fn remove_edge(store: &Store, edge_id: i64) -> GraphResult<()> {
    store.transaction(|store| {
        let affected = store.execute("DELETE FROM graph_edges WHERE id = ?1", params![edge_id])?;
        if affected == 0 {
            return Err(GraphError::EdgeNotFound(edge_id));
        }
        record_history(store, None, "edge_removed", &edge_id.to_string())
    })
}

pub fn traverse(
    store: &Store,
    start_id: i64,
    predicate: Option<&str>,
    depth: usize,
) -> GraphResult<Vec<TraversalResult>> {
    let mut results = vec![TraversalResult {
        node: get_node(store, start_id)?,
        depth: 0,
        path: vec![],
    }];
    let mut visited = HashSet::from([start_id]);
    let mut frontier = vec![(start_id, 0usize, Vec::<String>::new())];
    while let Some((current, current_depth, path)) = frontier.pop() {
        if current_depth >= depth {
            continue;
        }
        for (edge_pred, neighbor) in neighbor_edges(store, current, predicate)? {
            if visited.insert(neighbor) {
                let mut next_path = path.clone();
                next_path.push(edge_pred);
                results.push(TraversalResult {
                    node: get_node(store, neighbor)?,
                    depth: current_depth + 1,
                    path: next_path.clone(),
                });
                frontier.push((neighbor, current_depth + 1, next_path));
            }
        }
    }
    Ok(results)
}

pub fn get_all_rules(store: &Store) -> GraphResult<Vec<RuleWithDetails>> {
    let rules = store.all(
        &super::node_sql("WHERE node_type = 'rule' ORDER BY name"),
        (),
        row_to_node,
    )?;
    rules
        .into_iter()
        .map(|rule| rule_details(store, rule))
        .collect()
}

pub fn graph_rules_to_config(store: &Store) -> GraphResult<RulesConfig> {
    let mut rules = Vec::new();
    for detail in get_all_rules(store)? {
        let Some(mut match_criteria) = match_criteria(&detail) else {
            continue;
        };
        if let Some(address) = match_criteria.sender_contains.as_deref()
            && let Some(node) = super::context::sender_node(store, address)?
        {
            let meta: serde_json::Value = serde_json::from_str(&node.metadata)?;
            if let Some(aliases) = meta.get("emails").and_then(serde_json::Value::as_array) {
                let mut criteria = vec![match_criteria.clone()];
                criteria.extend(aliases.iter().filter_map(serde_json::Value::as_str).map(
                    |email| MatchCriteria {
                        sender_contains: Some(email.to_owned()),
                        ..Default::default()
                    },
                ));
                match_criteria = MatchCriteria {
                    any_of: Some(criteria),
                    ..Default::default()
                };
            }
        }
        let (action_type, label_number) = parse_action(
            detail.action_type.as_deref(),
            detail.action_value.as_deref(),
            &detail.rule_node.metadata,
        );
        rules.push(Rule {
            name: detail.rule_node.name,
            match_criteria,
            action: Action {
                action_type,
                label_number,
            },
        });
    }
    let vip_senders = get_vip_emails(store)?
        .into_iter()
        .map(|address| VipSender {
            name: vip_name(store, &address).ok().flatten(),
            address,
        })
        .collect();
    Ok(RulesConfig { rules, vip_senders })
}

pub fn add_vip(
    store: &Store,
    name: &str,
    email: &str,
    description: Option<&str>,
    context: Option<&str>,
) -> GraphResult<i64> {
    store.transaction(|store| {
        let person_id = if let Some(person) = super::context::sender_node(store, email)? {
            super::update_node(store, person.id, None, None, description, None, Some(true))?;
            person.id
        } else {
            add_node(store, "person", name, Some(email), description, None, true)?
        };
        let rule_meta = json!({"action_type": "label", "action_value": "1"}).to_string();
        let rule_id = add_node(
            store,
            "rule",
            &format!("VIP: {name}"),
            None,
            Some("Auto-generated VIP rule"),
            Some(&rule_meta),
            false,
        )?;
        add_edge(store, rule_id, person_id, "matches_sender", context, None)?;
        add_edge(
            store,
            rule_id,
            person_id,
            "applies_action",
            Some("label:1"),
            None,
        )?;
        add_edge(store, rule_id, person_id, "protects", None, None)?;
        Ok(person_id)
    })
}

pub fn add_rule(
    store: &Store,
    name: &str,
    match_type: &str,
    match_value: &str,
    action_type: &str,
    action_value: &str,
) -> GraphResult<i64> {
    store.transaction(|store| {
        let meta = json!({"action_type": action_type, "action_value": action_value}).to_string();
        let rule_id = add_node(store, "rule", name, None, None, Some(&meta), false)?;
        let target_id = match match_type {
            "sender" => person_for_sender(store, match_value)?,
            "subject" => add_node(store, "topic", match_value, None, None, None, false)?,
            _ => return Ok(rule_id),
        };
        add_edge(
            store,
            rule_id,
            target_id,
            &format!("matches_{match_type}"),
            None,
            None,
        )?;
        Ok(rule_id)
    })
}

fn raw_edges(store: &Store, node_id: i64, predicate: Option<&str>) -> GraphResult<Vec<Edge>> {
    match predicate {
        Some(value) => Ok(store.all(
            &edge_sql(
                "WHERE (source_id = ?1 OR target_id = ?1) AND predicate = ?2 ORDER BY created_at",
            ),
            params![node_id, value],
            row_to_edge,
        )?),
        None => Ok(store.all(
            &edge_sql("WHERE source_id = ?1 OR target_id = ?1 ORDER BY created_at"),
            params![node_id],
            row_to_edge,
        )?),
    }
}

fn neighbor_edges(
    store: &Store,
    node_id: i64,
    predicate: Option<&str>,
) -> GraphResult<Vec<(String, i64)>> {
    let edges = raw_edges(store, node_id, predicate)?;
    Ok(edges
        .into_iter()
        .map(|edge| {
            let neighbor = if edge.source_id == node_id {
                edge.target_id
            } else {
                edge.source_id
            };
            (edge.predicate, neighbor)
        })
        .collect())
}

fn rule_details(store: &Store, rule: Node) -> GraphResult<RuleWithDetails> {
    let mut details = RuleWithDetails {
        rule_node: rule.clone(),
        match_type: None,
        match_value: None,
        action_type: None,
        action_value: None,
    };
    for edge in get_edges(store, rule.id, None)? {
        let target = if edge.edge.source_id == rule.id {
            &edge.target
        } else {
            &edge.source
        };
        match edge.edge.predicate.as_str() {
            "matches_sender" => {
                details.match_type = Some("sender".to_string());
                details.match_value = target.email.clone().or_else(|| Some(target.name.clone()));
            }
            "matches_subject" => {
                details.match_type = Some("subject".to_string());
                details.match_value = Some(target.name.clone());
            }
            pred if pred.starts_with("applies_action") => {
                details.action_type = edge.edge.context.clone().or_else(|| Some(pred.to_string()));
                details.action_value = edge.edge.context.clone();
            }
            "protects" if details.action_type.is_none() => {
                details.action_type = Some("protect".to_string());
            }
            _ => {}
        }
    }
    fill_action_from_metadata(&mut details);
    Ok(details)
}

fn match_criteria(detail: &RuleWithDetails) -> Option<MatchCriteria> {
    match (detail.match_type.as_deref(), detail.match_value.as_deref()) {
        (Some("sender"), Some(value)) => Some(MatchCriteria {
            sender_contains: Some(value.to_string()),
            ..Default::default()
        }),
        (Some("subject"), Some(value)) => Some(MatchCriteria {
            subject_contains: Some(value.to_string()),
            ..Default::default()
        }),
        _ => None,
    }
}
