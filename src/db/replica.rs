//! Local replica lifecycle. Also ported to MEA; neither crate depends on the other.
mod errors;
pub mod guard;
#[cfg(test)]
mod integration;
mod state;
#[cfg(test)]
mod tests;
use anyhow::{Context, Result, ensure};
pub use errors::{ReplicaError, notice, write};
use libsql::{Builder, Connection, Database};
pub use state::{SyncState, due, now, state};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

pub struct Replica {
    pub path: PathBuf,
    pub primary_url: String,
    // Must outlive the database and every connection.
    pub lock: guard::ReplicaLock,
}
impl Replica {
    pub async fn open(path: &Path, url: &str, token: &str) -> Result<(Database, Connection, Self)> {
        let replica = Self {
            path: path.into(),
            primary_url: url.trim_end_matches('/').into(),
            lock: guard::ReplicaLock::open(path)?,
        };
        state::validate(path, &replica.primary_url)?;
        let fresh = !path.exists();
        ensure!(
            fresh || PathBuf::from(format!("{}-info", path.display())).exists(),
            "replica_sync_failed: replica metadata is missing; reset this tool's replica"
        );
        let database = Builder::new_synced_database(path, url.to_owned(), token.to_owned())
            .remote_writes(true)
            .build()
            .await
            .context(ReplicaError::SyncFailed)?;
        if fresh {
            let _exclusive = replica.lock.exclusive()?;
            // Bind before download; a partial bootstrap is never reused for another primary.
            state::save(
                path,
                &SyncState {
                    primary_url: replica.primary_url.clone(),
                    ..SyncState::default()
                },
            )?;
            replica.sync_locked(&database, 15).await?;
        }
        // sync above bootstraps asynchronously with a deadline. connect() now does local I/O only.
        let connection = database.connect().context(ReplicaError::SyncFailed)?;
        guard::protect(path)?;
        Ok((database, connection, replica))
    }
    pub async fn sync(&self, database: &Database, seconds: u64) -> Result<SyncState> {
        let _exclusive = self.lock.exclusive()?;
        self.sync_locked(database, seconds).await
    }
    /// Caller holds the exclusive lock, including throughout any auto-pull on writes.
    pub async fn sync_locked(&self, database: &Database, seconds: u64) -> Result<SyncState> {
        state::validate(&self.path, &self.primary_url)?;
        let result = tokio::time::timeout(Duration::from_secs(seconds), database.sync())
            .await
            .context(ReplicaError::SyncFailed)?
            .context(ReplicaError::SyncFailed)?;
        let state = SyncState {
            primary_url: self.primary_url.clone(),
            last_sync_unix: now(),
            frame_no: result.frame_no(),
            frames_synced: Some(result.frames_synced()),
        };
        state::save(&self.path, &state)?;
        guard::protect(&self.path)?;
        Ok(state)
    }
    pub async fn on_connect(&self, database: &Database, force: bool) -> Result<()> {
        let last = state(&self.path).map(|s| s.last_sync_unix);
        if due(
            last,
            now(),
            force || std::env::var("VAULT_SYNC").as_deref() == Ok("always"),
        ) && let Err(error) = self.sync(database, 5).await
        {
            eprintln!("{}", notice(&error, last));
        }
        Ok(())
    }
}

pub(super) fn path() -> Result<PathBuf> {
    Ok(super::config::config_path()?
        .parent()
        .context("config path has no parent")?
        .join("replica/mea.db"))
}
