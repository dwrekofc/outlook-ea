use super::*;
use crate::db::test_store;

#[test]
fn test_assign_and_get_label() {
    let conn = test_store().unwrap();
    assign_label(&conn, 1, "msg@test", 3).unwrap();
    let label = get_label(&conn, 1, "msg@test").unwrap().unwrap();
    assert_eq!(label.label_number, 3);
    assert_eq!(label.label_name, "Reference");
}

#[test]
fn test_assign_replaces_existing() {
    let conn = test_store().unwrap();
    assign_label(&conn, 1, "msg@test", 1).unwrap();
    assign_label(&conn, 1, "msg@test", 4).unwrap();
    let label = get_label(&conn, 1, "msg@test").unwrap().unwrap();
    assert_eq!(label.label_number, 4);
    assert_eq!(label.label_name, "Read Later");
}

#[test]
fn test_clear_label() {
    let conn = test_store().unwrap();
    assign_label(&conn, 1, "msg@test", 2).unwrap();
    assign_label(&conn, 1, "msg@test", 0).unwrap();
    let label = get_label(&conn, 1, "msg@test").unwrap();
    assert!(label.is_none());
}

#[test]
fn test_invalid_label() {
    let conn = test_store().unwrap();
    let err = assign_label(&conn, 1, "msg@test", 6);
    assert!(err.is_err());
}

#[test]
fn test_get_emails_by_label() {
    let conn = test_store().unwrap();
    assign_label(&conn, 10, "a@t", 1).unwrap();
    assign_label(&conn, 20, "b@t", 1).unwrap();
    assign_label(&conn, 30, "c@t", 2).unwrap();

    let follow_ups = get_emails_by_label(&conn, 1).unwrap();
    assert_eq!(follow_ups.len(), 2);
    assert!(follow_ups.contains(&10));
    assert!(follow_ups.contains(&20));
}

#[test]
fn test_get_untriaged() {
    let conn = test_store().unwrap();
    assign_label(&conn, 1, "a@t", 1).unwrap();
    // rowid 2 and 3 have no label
    db::ensure_identity(&conn, 2, "b@t").unwrap();
    db::ensure_identity(&conn, 3, "c@t").unwrap();

    let messages = [(1, "a@t".into()), (2, "b@t".into()), (3, "c@t".into())];
    let labels = get_labels_for_messages(&conn, &messages).unwrap();
    let untriaged: Vec<_> = messages
        .iter()
        .map(|(id, _)| *id)
        .filter(|id| !labels.contains_key(id))
        .collect();
    assert_eq!(untriaged.len(), 2);
    assert!(untriaged.contains(&2));
    assert!(untriaged.contains(&3));
}

#[test]
fn test_get_no_label_returns_none() {
    let conn = test_store().unwrap();
    let label = get_label(&conn, 999, "missing@test").unwrap();
    assert!(label.is_none());
}

#[test]
fn test_labels_persist() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.db");

    {
        let conn = crate::db::Store::connect(path.to_str().unwrap(), "").unwrap();
        conn.install_test_schema().unwrap();
        assign_label(&conn, 5, "persist@test", 2).unwrap();
    }
    {
        let conn = crate::db::Store::connect(path.to_str().unwrap(), "").unwrap();
        conn.install_test_schema().unwrap();
        let label = get_label(&conn, 5, "persist@test").unwrap().unwrap();
        assert_eq!(label.label_number, 2);
    }
}

#[test]
fn test_get_all_labels() {
    let conn = test_store().unwrap();
    assign_label(&conn, 1, "a@t", 1).unwrap();
    assign_label(&conn, 2, "b@t", 3).unwrap();
    let map = get_labels_for_messages(&conn, &[(1, "a@t".into()), (2, "b@t".into())]).unwrap();
    assert_eq!(map.len(), 2);
    assert_eq!(map[&1], 1);
    assert_eq!(map[&2], 3);
}

#[test]
fn missing_message_id_is_unlabeled_and_reads_do_not_insert() {
    let store = test_store().unwrap();
    assign_label(&store, 1, "old@test", 2).unwrap();
    assert!(get_label(&store, 1, "").unwrap().is_none());
    let labels = get_labels_for_messages(
        &store,
        &[
            (1, "".into()),
            (2, "new@test".into()),
            (999, "old@test".into()),
        ],
    )
    .unwrap();
    assert_eq!(labels.len(), 1);
    assert_eq!(labels[&999], 2);
    assert_eq!(
        store
            .one("SELECT count(*) FROM mail_identities", (), |r| Ok(
                r.get::<i64>(0)?
            ))
            .unwrap(),
        Some(1)
    );
}
