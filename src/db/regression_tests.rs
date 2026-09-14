use super::*;

#[test]
fn config_uses_vault_keys_and_allows_other_vault_settings() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");
    fs_write(
        &path,
        r#"{"database_url":"file:test.db","auth_token":"test-token","notes_path":"notes"}"#,
    );
    let settings = VaultSettings::load_from(&path).unwrap();
    assert_eq!(settings.database_url.as_deref(), Some("file:test.db"));
    assert_eq!(settings.auth_token.as_deref(), Some("test-token"));
}
fn fs_write(path: &Path, data: &str) {
    std::fs::write(path, data).unwrap();
}

#[test]
fn missing_schema_is_an_error_without_migration() {
    let store = Store::open_in_memory().unwrap();
    assert!(store.require_schema().is_err());
    assert_eq!(
        store
            .one(
                "SELECT count(*) FROM sqlite_master WHERE type='table'",
                (),
                |r| Ok(r.get::<i64>(0)?)
            )
            .unwrap(),
        Some(0)
    );
}

#[test]
fn exact_vault_fixture_is_idempotent_and_has_foreign_keys() {
    let store = test_store().unwrap();
    store.install_test_schema().unwrap();
    store.require_schema().unwrap();
    assert!(
        store
            .execute("INSERT INTO mail_labels VALUES (999,1,'now')", ())
            .is_err()
    );
}

#[test]
fn identities_preserve_aliases_across_rowid_reuse() {
    let store = test_store().unwrap();
    let old = ensure_identity(&store, 42, "old@test").unwrap();
    let new = ensure_identity(&store, 42, "new@test").unwrap();
    assert_ne!(old, new);
    assert_eq!(ensure_identity(&store, 77, "old@test").unwrap(), old);
    let alias = store
        .one(
            "SELECT identity_id FROM mail_machine_identity_sources WHERE source_id=42",
            (),
            |r| Ok(r.get::<i64>(0)?),
        )
        .unwrap();
    assert_eq!(alias, Some(new));
    assert!(ensure_identity(&store, 99, "").is_err());
}

#[test]
fn envelope_adapter_is_read_only_and_does_not_create_files() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("envelope.db");
    assert!(Store::open_read_only(&path).is_err());
    assert!(!path.exists());
    {
        let writable = Store::connect(path.to_str().unwrap(), "").unwrap();
        writable
            .execute("CREATE TABLE test (value INTEGER)", ())
            .unwrap();
    }
    let readonly = Store::open_read_only(&path).unwrap();
    assert!(readonly.execute("INSERT INTO test VALUES (1)", ()).is_err());
    assert_eq!(
        readonly
            .one("SELECT count(*) FROM test", (), |r| Ok(r.get::<i64>(0)?))
            .unwrap(),
        Some(0)
    );
}

#[test]
fn remote_requires_token_before_attempting_network() {
    assert!(Store::connect("libsql://unused.invalid", "").is_err());
}

#[test]
fn identity_failure_rolls_back_new_identity() {
    let store = test_store().unwrap();
    store.execute_batch("CREATE TRIGGER reject_alias BEFORE INSERT ON mail_machine_identity_sources BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    assert!(ensure_identity(&store, 1, "fail@test").is_err());
    assert_eq!(
        store
            .one("SELECT count(*) FROM mail_identities", (), |r| Ok(
                r.get::<i64>(0)?
            ))
            .unwrap(),
        Some(0)
    );
}

#[test]
fn dump_path_uses_configured_notes_root() {
    let settings = VaultSettings {
        notes_path: Some("/tmp/custom-notes".into()),
        ..Default::default()
    };
    assert_eq!(
        settings.mea_dump_path().unwrap(),
        Path::new("/tmp/custom-notes/utilities/context-profiles/mea/MEA_GRAPH_CONTEXT.md")
    );
}
