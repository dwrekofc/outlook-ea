use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct SyncState {
    #[serde(default)]
    pub primary_url: String,
    pub last_sync_unix: u64,
    pub frame_no: Option<u64>,
    pub frames_synced: Option<usize>,
}
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn due(last: Option<u64>, now: u64, force: bool) -> bool {
    force || last.is_none_or(|last| last == 0 || now < last || now - last >= 30)
}
pub fn state(path: &Path) -> Option<SyncState> {
    serde_json::from_slice(&std::fs::read(path.with_extension("sync.json")).ok()?).ok()
}
pub fn validate(path: &Path, url: &str) -> Result<()> {
    ensure!(
        !url.contains(['@', '?', '#']),
        "replica_primary_invalid: use auth_token separately from the primary URL"
    );
    let prior = state(path);
    if path.exists() || prior.is_some() {
        ensure!(
            prior.is_some_and(|s| s.primary_url == url),
            super::ReplicaError::PrimaryMismatch
        );
    }
    Ok(())
}
pub fn save(path: &Path, state: &SyncState) -> Result<()> {
    let temporary = path.with_extension(format!("sync.{}.tmp", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(&temporary)?;
    file.write_all(&serde_json::to_vec(state)?)?;
    file.sync_all()?;
    std::fs::rename(temporary, path.with_extension("sync.json"))
        .context("cannot record replica sync")
}
