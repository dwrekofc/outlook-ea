use std::{path::Path, rc::Rc};

use anyhow::{Context, Result, bail};
use libsql::{Connection, Database, params};
mod body_cache;
mod config;
mod connection;
pub mod replica;
use serde::{Deserialize, Serialize};
use tokio::runtime::{Builder as RuntimeBuilder, Runtime};

const REQUIRED_SCHEMA_VERSION: i64 = 4;

pub type DbError = anyhow::Error;
pub type DbResult<T> = Result<T, DbError>;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct VaultSettings {
    pub database_url: Option<String>,
    pub auth_token: Option<String>,
    pub notes_path: Option<std::path::PathBuf>,
}

pub struct Store {
    _database: Rc<Database>,
    connection: Connection,
    runtime: Rc<Runtime>,
    in_transaction: bool,
    replica: Option<Rc<replica::Replica>>,
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
        let runtime = RuntimeBuilder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()?;
        let (database, connection, replica) =
            runtime.block_on(connection::open(url, auth_token))?;
        Ok(Self {
            _database: Rc::new(database),
            connection,
            runtime: Rc::new(runtime),
            in_transaction: false,
            replica: replica.map(Rc::new),
        })
    }

    pub fn open_read_only(path: &Path) -> DbResult<Self> {
        let runtime = RuntimeBuilder::new_current_thread().enable_all().build()?;
        let (database, connection) = runtime.block_on(connection::read_only(path))?;
        Ok(Self {
            _database: Rc::new(database),
            connection,
            runtime: Rc::new(runtime),
            in_transaction: false,
            replica: None,
        })
    }

    /// Nested graph helpers share the outer immediate transaction and its connection.
    /// D17: read-modify-write operations must stay inside this boundary on replicas too.
    pub fn transaction<T, E>(&self, apply: impl FnOnce(&Store) -> Result<T, E>) -> Result<T, E>
    where
        E: From<anyhow::Error>,
    {
        if self.in_transaction {
            return apply(self);
        }
        let _exclusive = self
            .replica
            .as_ref()
            .map(|r| r.lock.exclusive())
            .transpose()
            .map_err(E::from)?;
        let tx = self
            .db_call(
                self.connection
                    .transaction_with_behavior(libsql::TransactionBehavior::Immediate),
            )
            .map_err(E::from)?;
        let scoped = Store {
            _database: Rc::clone(&self._database),
            connection: (*tx).clone(),
            runtime: Rc::clone(&self.runtime),
            in_transaction: true,
            replica: self.replica.clone(),
        };
        match apply(&scoped) {
            Ok(value) => {
                self.db_call(tx.commit()).map_err(E::from)?;
                self.sync_after_write().map_err(E::from)?;
                Ok(value)
            }
            Err(error) => {
                self.db_call(tx.rollback()).map_err(E::from)?;
                Err(error)
            }
        }
    }

    fn db_call<T>(
        &self,
        future: impl std::future::Future<Output = libsql::Result<T>>,
    ) -> DbResult<T> {
        self.runtime
            .block_on(replica::write(self.replica.is_some(), async {
                Ok(future.await?)
            }))
    }

    pub fn execute(&self, sql: &str, params: impl libsql::params::IntoParams) -> DbResult<u64> {
        let _exclusive = if !self.in_transaction {
            self.replica
                .as_ref()
                .map(|r| r.lock.exclusive())
                .transpose()?
        } else {
            None
        };
        let changed = self.db_call(self.connection.execute(sql, params))?;
        self.sync_after_write()?;
        Ok(changed)
    }

    pub fn execute_batch(&self, sql: &str) -> DbResult<()> {
        let _exclusive = if !self.in_transaction {
            self.replica
                .as_ref()
                .map(|r| r.lock.exclusive())
                .transpose()?
        } else {
            None
        };
        self.db_call(self.connection.execute_batch(sql))?;
        self.sync_after_write()
    }

    fn sync_after_write(&self) -> DbResult<()> {
        if !self.in_transaction
            && let Some(replica) = &self.replica
        {
            self.runtime
                .block_on(replica.sync_locked(&self._database, 15))
                .context(replica::ReplicaError::PrimaryWriteFailed)?;
        }
        Ok(())
    }

    pub fn one<T>(
        &self,
        sql: &str,
        params: impl libsql::params::IntoParams,
        map: impl FnOnce(&libsql::Row) -> Result<T>,
    ) -> DbResult<Option<T>> {
        let mut rows = self.db_call(self.connection.query(sql, params))?;
        let row = self.db_call(rows.next())?;
        row.map(|value| map(&value)).transpose()
    }

    pub fn all<T>(
        &self,
        sql: &str,
        params: impl libsql::params::IntoParams,
        mut map: impl FnMut(&libsql::Row) -> Result<T>,
    ) -> DbResult<Vec<T>> {
        let mut rows = self.db_call(self.connection.query(sql, params))?;
        let mut result = Vec::new();
        while let Some(row) = self.db_call(rows.next())? {
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
            bail!("Vault database schema must be at version 4 or newer; run vault init/migrations");
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
        self.execute_batch(include_str!("db/machine_sources_fixture.sql"))?;
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
    store.transaction(|store| {
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
    let machine_id = machine_id()?;
    let previous = store.one(
        "SELECT identity_id FROM mail_machine_identity_sources WHERE machine_id=?1 AND source_id=?2",
        params![machine_id.clone(), source_id], |r| Ok(r.get::<i64>(0)?),
    )?;
    store.execute(
        "INSERT INTO mail_machine_identity_sources(machine_id,source_id,identity_id) VALUES (?1,?2,?3)
         ON CONFLICT(machine_id,source_id) DO UPDATE SET identity_id=excluded.identity_id",
        params![machine_id, source_id, identity_id],
    )?;
    if previous.is_some_and(|old| old != identity_id) {
        eprintln!("warning: Apple Mail rowid {source_id} was reassigned on this machine");
    }
    Ok(identity_id)
    })
}

#[cfg(test)]
mod regression_tests;

pub(crate) fn machine_id() -> DbResult<String> {
    let output = std::process::Command::new("hostname")
        .output()
        .context("cannot determine machine hostname for mail aliases")?;
    let hostname = String::from_utf8(output.stdout)?.trim().to_owned();
    anyhow::ensure!(
        output.status.success() && !hostname.is_empty(),
        "machine hostname is missing"
    );
    Ok(hostname)
}
