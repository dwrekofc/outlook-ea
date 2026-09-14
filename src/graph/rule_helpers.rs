use libsql::params;

use crate::db::Store;
use crate::graph::RuleWithDetails;
use crate::rules::ActionType;

pub(crate) fn fill_action_from_metadata(details: &mut RuleWithDetails) {
    if details.action_type.is_some() {
        return;
    }
    let Ok(meta) = serde_json::from_str::<serde_json::Value>(&details.rule_node.metadata) else {
        return;
    };
    details.action_type = meta
        .get("action_type")
        .and_then(|value| value.as_str())
        .map(ToString::to_string);
    details.action_value = meta
        .get("action_value")
        .and_then(|value| value.as_str())
        .map(ToString::to_string);
}

pub(crate) fn parse_action(
    action_type: Option<&str>,
    action_value: Option<&str>,
    metadata: &str,
) -> (ActionType, Option<u8>) {
    if let Some(action) = action_type {
        return match action {
            "trash" => (ActionType::Trash, None),
            "archive" => (ActionType::Archive, None),
            value if value.starts_with("label") => {
                let label = action_value
                    .and_then(|raw| raw.trim_start_matches("label:").parse().ok())
                    .or_else(|| {
                        value
                            .strip_prefix("label:")
                            .and_then(|raw| raw.parse().ok())
                    });
                (ActionType::Label, label)
            }
            _ => parse_action_from_metadata(metadata),
        };
    }
    parse_action_from_metadata(metadata)
}

pub(crate) fn person_for_sender(store: &Store, email: &str) -> crate::graph::GraphResult<i64> {
    if let Some(node) = super::context::sender_node(store, email)? {
        return Ok(node.id);
    }
    crate::graph::add_node(store, "person", email, Some(email), None, None, false)
}

pub(crate) fn vip_name(store: &Store, address: &str) -> crate::graph::GraphResult<Option<String>> {
    Ok(store.one(
        "SELECT name FROM graph_nodes WHERE email = ?1 AND is_vip = 1",
        params![address],
        |row| Ok(row.get(0)?),
    )?)
}

fn parse_action_from_metadata(metadata: &str) -> (ActionType, Option<u8>) {
    let Ok(meta) = serde_json::from_str::<serde_json::Value>(metadata) else {
        return (ActionType::Label, Some(1));
    };
    let action_type = meta
        .get("action_type")
        .and_then(|value| value.as_str())
        .unwrap_or("label");
    let action_value = meta.get("action_value").and_then(|value| value.as_str());
    match action_type {
        "trash" => (ActionType::Trash, None),
        "archive" => (ActionType::Archive, None),
        "label" => (
            ActionType::Label,
            action_value.and_then(|raw| raw.parse().ok()),
        ),
        _ => (ActionType::Label, Some(1)),
    }
}
