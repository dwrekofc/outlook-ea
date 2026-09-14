use std::path::Path;

use anyhow::{Context, Result, bail};
use libsql::{Connection, Database, params};
mod config;
mod connection;
use serde::{Deserialize, Serialize};
use tokio::runtime::{Builder as RuntimeBuilder, Runtime};

const REQUIRED_SCHEMA_VERSION: i64 = 3;

pub type DbError = anyhow::Error;
pub type DbResult<T> = Result<T, DbError>;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct VaultSettings {
    pub database_url: Option<String>,
    pub auth_token: Option<String>,
}

pub struct Store {
    _database: Database,
    connection: Connection,
    runtime: Runtime,
}

impl Store {
    pub fn open_in_memory() -> DbResult<Self> {
        Self::connect(":memory:", "")
    }

    pub fn open_from_vault_config() -> DbResult<Self> {
        let settings = VaultSettings::load()?;
        let url = settings
            .database_url
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .context("Vault database_url is missing")?;
        let token = settings.auth_token.as_deref().unwrap_or("");
        let store = Self::connect(url, token)?;
        store.require_schema()?;
        Ok(store)
    }

    pub fn connect(url: &str, auth_token: &str) -> DbResult<Self> {
        let runtime = RuntimeBuilder::new_current_thread().enable_all().build()?;
        let (database, connection) = runtime.block_on(connection::open(url, auth_token))?;
        Ok(Self {
            _database: database,
            connection,
            runtime,
        })
    }

    pub fn open_read_only(path: &Path) -> DbResult<Self> {
        let runtime = RuntimeBuilder::new_current_thread().enable_all().build()?;
        let (database, connection) = runtime.block_on(connection::read_only(path))?;
        Ok(Self {
            _database: database,
            connection,
            runtime,
        })
    }

    pub fn execute(&self, sql: &str, params: impl libsql::params::IntoParams) -> DbResult<u64> {
        self.runtime
            .block_on(self.connection.execute(sql, params))
            .with_context(|| format!("database execute failed: {sql}"))
    }

    pub fn execute_batch(&self, sql: &str) -> DbResult<()> {
        self.runtime
            .block_on(self.connection.execute_batch(sql))
            .with_context(|| "database batch failed")?;
        Ok(())
    }

    pub fn one<T>(
        &self,
        sql: &str,
        params: impl libsql::params::IntoParams,
        map: impl FnOnce(&libsql::Row) -> Result<T>,
    ) -> DbResult<Option<T>> {
        let mut rows = self
            .runtime
            .block_on(self.connection.query(sql, params))
            .with_context(|| format!("database query failed: {sql}"))?;
        let row = self.runtime.block_on(rows.next())?;
        row.map(|value| map(&value)).transpose()
    }

    pub fn all<T>(
        &self,
        sql: &str,
        params: impl libsql::params::IntoParams,
        mut map: impl FnMut(&libsql::Row) -> Result<T>,
    ) -> DbResult<Vec<T>> {
        let mut rows = self
            .runtime
            .block_on(self.connection.query(sql, params))
            .with_context(|| format!("database query failed: {sql}"))?;
        let mut result = Vec::new();
        while let Some(row) = self.runtime.block_on(rows.next())? {
            result.push(map(&row)?);
        }
        Ok(result)
    }

    fn require_schema(&self) -> DbResult<()> {
        let version = self
            .one(
                "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                (),
                |row| Ok(row.get::<i64>(0)?),
            )?
            .unwrap_or(0);
        if version < REQUIRED_SCHEMA_VERSION {
            bail!("Vault database schema must be at version 3 or newer; run vault init/migrations");
        }
        self.one("SELECT 1 FROM graph_nodes LIMIT 0", (), |_| Ok(()))?;
        self.one("SELECT 1 FROM mail_identities LIMIT 0", (), |_| Ok(()))?;
        Ok(())
    }

    #[cfg(test)]
    pub fn install_test_schema(&self) -> DbResult<()> {
        // Copied from vault-cli migrations/003_graph_mail.sql as the test fixture.
        self.execute_batch("CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL);")?;
        self.execute_batch(include_str!("db/schema_fixture.sql"))?;
        Ok(())
    }
}

#[cfg(test)]
pub fn test_store() -> DbResult<Store> {
    let store = Store::connect(":memory:", "")?;
    store.install_test_schema()?;
    Ok(store)
}

pub fn ensure_identity(store: &Store, source_id: i64, message_id: &str) -> DbResult<i64> {
    if message_id.trim().is_empty() {
        bail!("message_id is required for mail identity storage");
    }
    store.execute(
        "INSERT INTO mail_identities (message_id) VALUES (?1)
         ON CONFLICT(message_id) DO NOTHING",
        params![message_id],
    )?;
    let identity_id = store
        .one(
            "SELECT id FROM mail_identities WHERE message_id = ?1",
            params![message_id],
            |row| Ok(row.get::<i64>(0)?),
        )?
        .context("mail identity was not created")?;
    store.execute(
        "INSERT OR IGNORE INTO mail_identity_sources (source_id, identity_id) VALUES (?1, ?2)",
        params![source_id, identity_id],
    )?;
    Ok(identity_id)
}

#[cfg(test)]
mod regression_tests;
