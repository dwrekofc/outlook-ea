#[test]
fn label_write_through() -> anyhow::Result<()> {
    if std::env::var("VAULT_REPLICA_PROOF").as_deref() != Ok("1") {
        return Ok(());
    }
    anyhow::ensure!(
        std::env::var_os("VAULT_AUTH_TOKEN").is_some(),
        "proof requires VAULT_AUTH_TOKEN"
    );
    let store = crate::db::Store::open_from_vault_config()?;
    let stamp = chrono::Utc::now().timestamp_millis();
    let message = format!("replica-proof-{stamp}@vault.invalid");
    let result: anyhow::Result<()> = (|| {
        crate::labels::assign_label(&store, -stamp, &message, 3)?;
        let label = crate::labels::get_label(&store, -stamp, &message)?
            .ok_or_else(|| anyhow::anyhow!("missing proof label"))?;
        anyhow::ensure!(label.label_number == 3, "wrong label");
        Ok(())
    })();
    // Always cleanup by this test's unique identity, even after a failed assertion.
    store.transaction::<_, anyhow::Error>(|store| {
        for table in ["mail_labels", "mail_machine_identity_sources", "mail_identity_sources"] {
            store.execute(&format!("DELETE FROM {table} WHERE identity_id IN (SELECT id FROM mail_identities WHERE message_id=?1)"), [message.as_str()])?;
        }
        store.execute("DELETE FROM mail_identities WHERE message_id=?1", [message.as_str()])?;
        Ok(())
    })?;
    result
}
