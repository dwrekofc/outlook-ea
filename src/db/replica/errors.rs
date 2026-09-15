use anyhow::{Context, Result};
use std::{future::Future, time::Duration};
#[derive(Debug)]
pub enum ReplicaError {
    SyncFailed,
    PrimaryWriteFailed,
    PrimaryMismatch,
}
impl std::fmt::Display for ReplicaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::PrimaryMismatch => "replica_primary_mismatch: replica belongs to another or unknown primary; reset this tool's replica before changing database_url",
            Self::SyncFailed => "replica_sync_failed: cannot synchronize with primary",
            Self::PrimaryWriteFailed => "replica_write_failed: primary write or confirmation failed; check primary before retrying (no offline queue)",
        })
    }
}
impl std::error::Error for ReplicaError {}
pub fn write_error(error: anyhow::Error) -> anyhow::Error {
    if error.downcast_ref::<libsql::Error>().is_some() {
        error.context(ReplicaError::PrimaryWriteFailed)
    } else {
        error
    }
}
pub async fn write<T>(remote: bool, future: impl Future<Output = Result<T>>) -> Result<T> {
    if remote {
        write_for(Duration::from_secs(15), future).await
    } else {
        future.await
    }
}
async fn write_for<T>(duration: Duration, future: impl Future<Output = Result<T>>) -> Result<T> {
    tokio::time::timeout(duration, future)
        .await
        .context(ReplicaError::PrimaryWriteFailed)?
        .map_err(write_error)
}
#[cfg(test)]
pub async fn deadline_probe() -> Result<()> {
    write_for(Duration::from_millis(5), std::future::pending()).await
}

pub fn notice(error: &anyhow::Error, last: Option<u64>) -> String {
    let age = last
        .filter(|t| *t != 0)
        .map(|t| format!("{}s", super::now().saturating_sub(t)))
        .unwrap_or_else(|| "unknown time".into());
    let detail = format!("{error:#}").replace(['\n', '\r'], " ");
    if detail.contains("http dispatch error:") {
        format!("replica: last synced {age} ago, offline")
    } else {
        format!("replica: last synced {age} ago, sync failed ({detail})")
    }
}
