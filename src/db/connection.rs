use super::config::is_remote;
use anyhow::{Context, Result, ensure};
use libsql::{Builder, Connection, Database, OpenFlags};
use std::path::Path;

/// Per-tool replica of the shared primary (D17).
pub(super) async fn open(
    url: &str,
    token: &str,
) -> Result<(Database, Connection, Option<super::replica::Replica>)> {
    if is_remote(url) {
        ensure!(
            !token.trim().is_empty(),
            "replica_sync_failed: Vault auth_token is required for remote database URL"
        );
        let (database, connection, replica) =
            super::replica::Replica::open(&super::replica::path()?, url, token).await?;
        replica.on_connect(&database, false).await?;
        return Ok((database, connection, Some(replica)));
    }
    let database = Builder::new_local(url.strip_prefix("file:").unwrap_or(url))
        .build()
        .await?;
    let connection = database.connect()?;
    connection.execute("PRAGMA foreign_keys = ON", ()).await?;
    Ok((database, connection, None))
}

/// Apple Mail is always opened read-only; never create or migrate its database.
pub(super) async fn read_only(path: &Path) -> Result<(Database, Connection)> {
    let database = Builder::new_local(path)
        .flags(OpenFlags::SQLITE_OPEN_READ_ONLY)
        .build()
        .await
        .context("failed to open read-only Envelope Index")?;
    let connection = database.connect()?;
    Ok((database, connection))
}
