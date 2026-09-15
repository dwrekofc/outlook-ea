//! Port of vault-cli's small replica throttle; each tool owns its own file.
use anyhow::{Context, Result};
use libsql::{Connection, Database};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Default, Deserialize, Serialize)]
struct SyncState {
    last_sync_unix: u64,
    frame_no: Option<u64>,
    frames_synced: Option<usize>,
}
pub(super) fn path() -> Result<PathBuf> {
    Ok(super::config::config_path()?
        .parent()
        .context("config path has no parent")?
        .join("replica/mea.db"))
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn due(last: Option<u64>, now: u64, force: bool) -> bool {
    force || last.is_none_or(|last| now < last || now - last >= 30)
}
fn state(path: &Path) -> Option<SyncState> {
    serde_json::from_slice(&std::fs::read(path.with_extension("sync.json")).ok()?).ok()
}
pub(super) async fn sync(database: &Database, path: &Path) -> Result<()> {
    let result = tokio::time::timeout(Duration::from_secs(15), database.sync())
        .await
        .context("replica_sync_failed: primary timed out")?
        .context("replica_sync_failed: cannot synchronize primary")?;
    let state = SyncState {
        last_sync_unix: now(),
        frame_no: result.frame_no(),
        frames_synced: Some(result.frames_synced()),
    };
    let temporary = path.with_extension(format!("sync.{}.tmp", std::process::id()));
    std::fs::write(&temporary, serde_json::to_vec(&state)?)?;
    std::fs::rename(temporary, path.with_extension("sync.json"))?;
    Ok(())
}
pub(super) async fn on_connect(database: &Database, connection: &Connection) -> Result<()> {
    let path = path()?;
    let last = state(&path).map(|s| s.last_sync_unix);
    if due(
        last,
        now(),
        std::env::var("VAULT_SYNC").as_deref() == Ok("always"),
    ) && let Err(error) = sync(database, &path).await
    {
        connection
            .query("SELECT version FROM schema_migrations LIMIT 1", ())
            .await
            .map_err(|_| error)?;
        let age = last
            .map(|t| format!("{}s", now().saturating_sub(t)))
            .unwrap_or_else(|| "unknown time".into());
        eprintln!("replica: last synced {age} ago, offline");
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    #[test]
    fn throttle_boundaries() {
        assert!(super::due(None, 100, false));
        assert!(!super::due(Some(100), 129, false));
        assert!(super::due(Some(100), 130, false));
        assert!(super::due(Some(100), 101, true));
        assert!(super::due(Some(100), 99, false));
    }
}

#[cfg(test)]
mod integration {
    #[test]
    fn label_write_through() -> anyhow::Result<()> {
        if std::env::var_os("VAULT_AUTH_TOKEN").is_none() {
            eprintln!("skipped: VAULT_AUTH_TOKEN is not set");
            return Ok(());
        }
        let store = crate::db::Store::open_from_vault_config()?;
        let stamp = chrono::Utc::now().timestamp_millis();
        let message = format!("replica-proof-{stamp}@vault.invalid");
        crate::labels::assign_label(&store, -stamp, &message, 3)?;
        let label = crate::labels::get_label(&store, -stamp, &message)?.unwrap();
        assert_eq!(label.label_number, 3);
        println!("retained replica label proof: {message}");
        Ok(())
    }
}
