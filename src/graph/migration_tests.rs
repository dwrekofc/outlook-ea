use super::*;
use serde_json::json;

#[test]
fn imported_ids_fields_profiles_and_provenance_remain_canonical() {
    let store = crate::db::test_store().unwrap();
    let mea_id = add_node(&store, "person", "MEA", None, None, None, false).unwrap();
    let provenance = json!({"sources":[{"profile":"mea","source_id":mea_id}], "source_records":[],"status":"todo"});
    store.execute("INSERT INTO graph_nodes (id,node_type,name,status,due_date,metadata,created_at,updated_at,profile,source_id) VALUES (9000,'task','Personal task','done','2026-09-20',?1,'now','now','personal',?2)", params![provenance.to_string(),mea_id]).unwrap();
    assert_eq!(get_node(&store, mea_id).unwrap().name, "MEA");
    assert_eq!(find_nodes(&store, "Personal task").unwrap()[0].id, 9000);
    assert_eq!(list_tasks(&store, None, Some("done")).unwrap()[0].id, 9000);
    update_node(&store, 9000, Some("Updated"), None, None, None, None).unwrap();
    let node = get_node(&store, 9000).unwrap();
    let meta: Value = serde_json::from_str(&node.metadata).unwrap();
    assert_eq!(meta["status"], "done");
    assert_eq!(meta["due_date"], "2026-09-20");
    update_node(
        &store,
        9000,
        None,
        None,
        None,
        Some(r#"{"sources":[],"source_records":[]}"#),
        None,
    )
    .unwrap();
    let meta: Value = serde_json::from_str(&get_node(&store, 9000).unwrap().metadata).unwrap();
    assert_eq!(meta["sources"], provenance["sources"]);
    assert_eq!(
        store
            .one("SELECT profile FROM graph_nodes WHERE id=9000", (), |r| Ok(
                r.get::<String>(0)?
            ))
            .unwrap()
            .as_deref(),
        Some("personal")
    );
    assert_eq!(
        store
            .one(
                "SELECT count(*) FROM graph_history WHERE profile != 'mea'",
                (),
                |r| Ok(r.get::<i64>(0)?)
            )
            .unwrap(),
        Some(0)
    );
    let edge = add_edge(&store, mea_id, 9000, "owns", None, None).unwrap();
    assert_eq!(get_edges(&store, mea_id, None).unwrap()[0].target.id, 9000);
    remove_node(&store, 9000).unwrap();
    assert!(get_edges(&store, mea_id, None).unwrap().is_empty());
    assert!(remove_edge(&store, edge).is_err());
}

#[test]
fn merged_email_aliases_share_context_and_vip_protection() {
    let store = crate::db::test_store().unwrap();
    let id = add_node(
        &store,
        "person",
        "Merged",
        Some("primary@test"),
        None,
        Some(r#"{"emails":["primary@test","work@test"]}"#),
        true,
    )
    .unwrap();
    let context = get_sender_context(&store, "WORK@test").unwrap().unwrap();
    assert_eq!(context.node_id, id);
    assert!(context.is_vip);
    assert!(
        get_vip_emails(&store)
            .unwrap()
            .contains(&"work@test".into())
    );
    let rule = add_rule(&store, "Alias rule", "sender", "work@test", "label", "2").unwrap();
    assert_eq!(get_edges(&store, rule, None).unwrap()[0].target.id, id);
    let config = graph_rules_to_config(&store).unwrap();
    assert!(crate::rules::evaluate_rules(&config, "work@test", "Hello").is_some());
}

#[test]
fn invalid_metadata_and_missing_project_do_not_create_nodes() {
    let store = crate::db::test_store().unwrap();
    assert!(add_node(&store, "person", "Bad", None, None, Some("[]"), false).is_err());
    assert!(add_task(&store, "Bad task", None, None, Some(999)).is_err());
    assert!(list_nodes(&store, None, false).unwrap().is_empty());
}
