use super::{guard, path, state};
use crate::db::{Store, VaultSettings};
use anyhow::{Context, Result};
use clap::Subcommand;
use serde_json::{Value, json};

#[derive(Subcommand)]
pub enum Command {
    /// Inspect local state without contacting the primary
    Status,
    /// Force synchronization with the primary
    Sync,
    /// Remove only MEA's disposable replica; bootstrap on the next command
    Reset,
}

pub fn command(command: Command) -> Result<Value> {
    let path = path()?;
    match command {
        Command::Reset => guard::reset(&path)?,
        Command::Sync => {
            let settings = VaultSettings::load()?;
            let store = Store::connect(
                settings
                    .database_url
                    .as_deref()
                    .context("Vault database_url is missing")?,
                settings.auth_token.as_deref().unwrap_or(""),
            )?;
            let replica = store
                .replica
                .as_ref()
                .context("replica sync requires a remote database URL")?;
            store.runtime.block_on(replica.sync(&store._database, 15))?;
        }
        Command::Status => (),
    }
    Ok(
        json!({"path": path, "size": std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0), "sync": state(&path)}),
    )
}
