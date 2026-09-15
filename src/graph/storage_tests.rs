#[test]
fn multi_statement_writes_roll_back_when_history_fails() {
    let store = crate::db::test_store().unwrap();
    let person =
        super::add_node(&store, "person", "Ada", Some("ada@test"), None, None, false).unwrap();
    store.execute_batch("CREATE TRIGGER reject_history BEFORE INSERT ON graph_history BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    assert!(super::add_vip(&store, "Ada", "ada@test", None, None).is_err());
    assert!(!super::get_node(&store, person).unwrap().is_vip);
    assert!(super::remove_node(&store, person).is_err());
    assert!(super::get_node(&store, person).is_ok());
    assert!(super::add_task(&store, "Fail", None, None, None).is_err());
    assert_eq!(super::list_nodes(&store, None, false).unwrap().len(), 1);
}

#[test]
fn existing_sender_is_promoted_and_archived_nodes_are_hidden() {
    let store = crate::db::test_store().unwrap();
    let person =
        super::add_node(&store, "person", "Ada", Some("ada@test"), None, None, false).unwrap();
    assert_eq!(
        super::add_vip(&store, "Ada", "ada@test", None, None).unwrap(),
        person
    );
    let node = super::get_node(&store, person).unwrap();
    assert!(node.is_vip);
    assert_eq!(node.profile, "mea");
    assert!(!node.archived);
    store
        .execute(
            "UPDATE graph_nodes SET archived=1 WHERE id=?1",
            libsql::params![person],
        )
        .unwrap();
    assert!(
        super::context::sender_node(&store, "ada@test")
            .unwrap()
            .is_none()
    );
    assert!(super::find_nodes(&store, "ada@test").unwrap().is_empty());
    assert!(
        super::list_nodes(&store, Some("person"), false)
            .unwrap()
            .is_empty()
    );
    assert!(!super::dump_context(&store).unwrap().contains("<ada@test>"));
    assert!(super::get_all_rules(&store).is_ok());
}

#[test]
fn task_and_rule_failures_roll_back_all_nested_writes() {
    let store = crate::db::test_store().unwrap();
    let project = super::add_project(&store, "Project", None).unwrap();
    store.execute_batch("CREATE TRIGGER reject_edge BEFORE INSERT ON graph_edges BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    assert!(super::add_task(&store, "Fail", None, None, Some(project)).is_err());
    assert!(super::add_rule(&store, "Fail rule", "sender", "new@test", "label", "1").is_err());
    assert_eq!(super::list_nodes(&store, None, false).unwrap().len(), 1);
    assert_eq!(
        store
            .one("SELECT count(*) FROM graph_history", (), |r| Ok(
                r.get::<i64>(0)?
            ))
            .unwrap(),
        Some(1)
    );
}
