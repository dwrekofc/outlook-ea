use super::*;
#[test]
fn cache_follows_message_id_and_never_an_unverified_alias() {
    let store = db::test_store().unwrap();
    cache_body(&store, 1, "old@test", "body\0after nul", "plain", &[], &[]).unwrap();
    assert!(get_cached_body(&store, 1, "new@test").unwrap().is_none());
    assert!(get_cached_body(&store, 1, "").unwrap().is_none());
    assert_eq!(
        get_cached_body(&store, 999, "old@test")
            .unwrap()
            .unwrap()
            .body_text,
        "body\0after nul"
    );
}
