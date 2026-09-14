use super::config::is_remote;
use anyhow::{Context, Result, ensure};
use libsql::{Builder, Connection, Database, OpenFlags};
use std::path::Path;

/// The shared vault connection boundary. D17's future embedded replica changes here.
/// This milestone deliberately follows vault-cli's current remote connection.
pub(super) async fn open(url: &str, token: &str) -> Result<(Database, Connection)> {
    let database = if is_remote(url) {
        ensure!(
            !token.trim().is_empty(),
            "Vault auth_token is required for remote database URL"
        );
        Builder::new_remote(url.to_owned(), token.to_owned())
            .build()
            .await
            .context("failed to connect to Turso")?
    } else {
        let path = url.strip_prefix("file:").unwrap_or(url);
        Builder::new_local(path)
            .build()
            .await
            .context("failed to open configured local vault database")?
    };
    let connection = database.connect()?;
    connection.execute("PRAGMA foreign_keys = ON", ()).await?;
    Ok((database, connection))
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
