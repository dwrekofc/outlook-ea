use crate::app::open_store;
use crate::cli::{self, GraphAction};
use crate::graph;

pub(crate) fn cmd_graph(action: GraphAction) -> String {
    let store = match open_store() {
        Ok(conn) => conn,
        Err(err) => return cli::error(&err, "STORAGE_ERROR"),
    };
    match action {
        GraphAction::Add {
            r#type,
            name,
            email,
            description,
            vip,
        } => match graph::add_node(
            &store,
            &r#type,
            &name,
            email.as_deref(),
            description.as_deref(),
            None,
            vip,
        ) {
            Ok(id) => success_dump(
                &store,
                serde_json::json!({"id": id, "node_type": r#type, "name": name}),
            ),
            Err(err) => cli::error(&err.to_string(), "GRAPH_ERROR"),
        },
        GraphAction::Link {
            from,
            to,
            predicate,
            context,
        } => match graph::add_edge(&store, from, to, &predicate, context.as_deref(), None) {
            Ok(id) => success_dump(
                &store,
                serde_json::json!({"edge_id": id, "from": from, "to": to, "predicate": predicate}),
            ),
            Err(err) => cli::error(&err.to_string(), "GRAPH_ERROR"),
        },
        GraphAction::Show { id } => match graph::get_node(&store, id) {
            Ok(node) => {
                let edges = match graph::get_edges(&store, id, None) {
                    Ok(edges) => edges,
                    Err(err) => return cli::error(&err.to_string(), "GRAPH_ERROR"),
                };
                cli::success(serde_json::json!({"node": node, "edges": edges}))
            }
            Err(err) => cli::error(&err.to_string(), "GRAPH_ERROR"),
        },
        GraphAction::List { r#type, vip } => {
            match graph::list_nodes(&store, r#type.as_deref(), vip) {
                Ok(nodes) => {
                    cli::success(serde_json::json!({"nodes": nodes, "count": nodes.len()}))
                }
                Err(err) => cli::error(&err.to_string(), "GRAPH_ERROR"),
            }
        }
        GraphAction::Find { query } => match graph::find_nodes(&store, &query) {
            Ok(nodes) => cli::success(serde_json::json!({"nodes": nodes, "count": nodes.len()})),
            Err(err) => cli::error(&err.to_string(), "GRAPH_ERROR"),
        },
        GraphAction::Edges { id, predicate } => {
            match graph::get_edges(&store, id, predicate.as_deref()) {
                Ok(edges) => {
                    cli::success(serde_json::json!({"edges": edges, "count": edges.len()}))
                }
                Err(err) => cli::error(&err.to_string(), "GRAPH_ERROR"),
            }
        }
        GraphAction::Traverse {
            id,
            predicate,
            depth,
        } => match graph::traverse(&store, id, predicate.as_deref(), depth) {
            Ok(results) => {
                cli::success(serde_json::json!({"results": results, "count": results.len()}))
            }
            Err(err) => cli::error(&err.to_string(), "GRAPH_ERROR"),
        },
        GraphAction::Remove { id } => match graph::remove_node(&store, id) {
            Ok(()) => success_dump(&store, serde_json::json!({"removed": id})),
            Err(err) => cli::error(&err.to_string(), "GRAPH_ERROR"),
        },
        GraphAction::Unlink { edge_id } => match graph::remove_edge(&store, edge_id) {
            Ok(()) => success_dump(&store, serde_json::json!({"unlinked": edge_id})),
            Err(err) => cli::error(&err.to_string(), "GRAPH_ERROR"),
        },
        GraphAction::AddVip {
            email,
            name,
            description,
            context,
        } => match graph::add_vip(
            &store,
            &name,
            &email,
            description.as_deref(),
            context.as_deref(),
        ) {
            Ok(id) => success_dump(
                &store,
                serde_json::json!({"person_id": id, "name": name, "email": email, "is_vip": true}),
            ),
            Err(err) => cli::error(&err.to_string(), "GRAPH_ERROR"),
        },
        GraphAction::AddRule {
            name,
            match_sender,
            match_subject,
            action,
        } => add_rule(&store, name, match_sender, match_subject, action),
        GraphAction::Rules => match graph::get_all_rules(&store) {
            Ok(rules) => cli::success(serde_json::json!({"rules": rules, "count": rules.len()})),
            Err(err) => cli::error(&err.to_string(), "GRAPH_ERROR"),
        },
        GraphAction::Dump => match graph::dump_context(&store) {
            Ok(content) => success_dump(&store, serde_json::json!({"content": content})),
            Err(err) => cli::error(&err.to_string(), "GRAPH_ERROR"),
        },
        other => cmd_graph_tasks(&store, other),
    }
}

fn add_rule(
    store: &crate::db::Store,
    name: String,
    match_sender: Option<String>,
    match_subject: Option<String>,
    action: String,
) -> String {
    let (match_type, match_value) = match (match_sender.as_deref(), match_subject.as_deref()) {
        (Some(sender), _) => ("sender", sender),
        (_, Some(subject)) => ("subject", subject),
        _ => {
            return cli::error(
                "Must specify --match-sender or --match-subject",
                "INVALID_ARGS",
            );
        }
    };
    let (action_type, action_value) = action
        .strip_prefix("label:")
        .map(|label| ("label", label))
        .unwrap_or((action.as_str(), ""));
    match graph::add_rule(
        store,
        &name,
        match_type,
        match_value,
        action_type,
        action_value,
    ) {
        Ok(id) => success_dump(store, serde_json::json!({"rule_id": id, "name": name})),
        Err(err) => cli::error(&err.to_string(), "GRAPH_ERROR"),
    }
}

fn success_dump(store: &crate::db::Store, value: serde_json::Value) -> String {
    if let Err(err) = graph::auto_dump(store) {
        return cli::error(&err.to_string(), "GRAPH_ERROR");
    }
    cli::success(value)
}

fn cmd_graph_tasks(store: &crate::db::Store, action: GraphAction) -> String {
    match action {
        GraphAction::AddProject { name, description } => {
            match graph::add_project(store, &name, description.as_deref()) {
                Ok(id) => success_dump(store, serde_json::json!({"project_id": id, "name": name})),
                Err(err) => cli::error(&err.to_string(), "GRAPH_ERROR"),
            }
        }
        GraphAction::AddTask {
            title,
            description,
            due,
            project,
        } => match graph::add_task(
            store,
            &title,
            description.as_deref(),
            due.as_deref(),
            project,
        ) {
            Ok(id) => success_dump(store, serde_json::json!({"task_id": id, "title": title})),
            Err(err) => cli::error(&err.to_string(), "GRAPH_ERROR"),
        },
        GraphAction::Tasks { project, status } => {
            match graph::list_tasks(store, project, status.as_deref()) {
                Ok(tasks) => {
                    cli::success(serde_json::json!({"tasks": tasks, "count": tasks.len()}))
                }
                Err(err) => cli::error(&err.to_string(), "GRAPH_ERROR"),
            }
        }
        GraphAction::Projects { active } => match graph::list_projects(store, active) {
            Ok(projects) => {
                cli::success(serde_json::json!({"projects": projects, "count": projects.len()}))
            }
            Err(err) => cli::error(&err.to_string(), "GRAPH_ERROR"),
        },
        GraphAction::TaskStatus { id, status } => {
            match graph::update_task_status(store, id, &status) {
                Ok(()) => success_dump(store, serde_json::json!({"task_id": id, "status": status})),
                Err(err) => cli::error(&err.to_string(), "GRAPH_ERROR"),
            }
        }
        _ => cli::error("Unsupported graph action", "INVALID_ARGS"),
    }
}
