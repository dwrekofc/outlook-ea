use super::*;
use crate::db::test_store;

#[test]
fn test_add_and_get_node() {
    let conn = test_store().unwrap();
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
    let conn = test_store().unwrap();
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
    let conn = test_store().unwrap();
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
    let conn = test_store().unwrap();
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
    let conn = test_store().unwrap();
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
    let conn = test_store().unwrap();
    let person_id = add_vip(&conn, "Boss", "boss@co.com", Some("CEO"), Some("important")).unwrap();

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
    let conn = test_store().unwrap();
    let rule_id = add_rule(&conn, "Trash spam", "sender", "spam@junk.com", "trash", "").unwrap();

    let rule = get_node(&conn, rule_id).unwrap();
    assert_eq!(rule.node_type, "rule");

    let edges = get_edges(&conn, rule_id, None).unwrap();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].edge.predicate, "matches_sender");
}

#[test]
fn test_get_sender_context_returns_correct_data() {
    let conn = test_store().unwrap();
    add_vip(&conn, "Boss", "boss@co.com", Some("CEO"), None).unwrap();

    let ctx = get_sender_context(&conn, "boss@co.com").unwrap().unwrap();
    assert!(ctx.is_vip);
    assert_eq!(ctx.description.as_deref(), Some("CEO"));
    assert!(!ctx.edges.is_empty());
    assert!(!ctx.rules.is_empty());
}

#[test]
fn test_get_sender_context_unknown_returns_none() {
    let conn = test_store().unwrap();
    let ctx = get_sender_context(&conn, "unknown@test.com").unwrap();
    assert!(ctx.is_none());
}

#[test]
fn test_get_vip_emails() {
    let conn = test_store().unwrap();
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
    let conn = test_store().unwrap();
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
    let conn = test_store().unwrap();
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
    let conn = test_store().unwrap();
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
    let conn = test_store().unwrap();
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
    let conn = test_store().unwrap();
    let a = add_node(&conn, "person", "A", None, None, None, false).unwrap();
    let b = add_node(&conn, "team", "B", None, None, None, false).unwrap();
    let eid = add_edge(&conn, a, b, "member_of", None, None).unwrap();

    remove_edge(&conn, eid).unwrap();
    let edges = get_edges(&conn, a, None).unwrap();
    assert!(edges.is_empty());
}

#[test]
fn test_remove_edge_not_found() {
    let conn = test_store().unwrap();
    assert!(remove_edge(&conn, 9999).is_err());
}

#[test]
fn test_get_all_rules() {
    let conn = test_store().unwrap();
    add_rule(&conn, "Rule A", "sender", "a@t.com", "label", "1").unwrap();
    add_rule(&conn, "Rule B", "subject", "newsletter", "trash", "").unwrap();

    let rules = get_all_rules(&conn).unwrap();
    assert_eq!(rules.len(), 2);
}
