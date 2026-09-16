use super::{Store, VaultSettings, config, replica};
use crate::body::CachedBody;
use anyhow::{Context, Result};
use libsql::params;

impl Store {
    /// Preview opens the existing local database read-only, without sync or bootstrap.
    pub fn open_body_cache(preview: bool) -> Result<Self> {
        let settings = VaultSettings::load()?;
        let url = settings
            .database_url
            .as_deref()
            .context("Vault database_url is missing")?;
        if preview {
            let path = if config::is_remote(url) {
                replica::path()?
            } else {
                url.strip_prefix("file:").unwrap_or(url).into()
            };
            // Acquire protection before opening SQLite, including against reset.
            let replica = if config::is_remote(url) {
                let lock = replica::guard::ReplicaLock::open(&path)?;
                anyhow::ensure!(
                    path.exists(),
                    "replica_sync_failed: no local replica; run mea replica sync before preview"
                );
                Some(std::rc::Rc::new(replica::Replica {
                    path: path.clone(),
                    primary_url: url.trim_end_matches('/').into(),
                    lock,
                }))
            } else {
                None
            };
            let mut store = Self::open_read_only(&path)?;
            store.replica = replica;
            return Ok(store);
        }
        if !config::is_remote(url) {
            return Self::open_from_vault_config();
        }
        let token = settings
            .auth_token
            .as_deref()
            .filter(|t| !t.trim().is_empty())
            .context("replica_sync_failed: Vault auth_token is required")?;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()?;
        let path = replica::path()?;
        let fresh = !path.exists();
        let (database, connection, replica) = runtime.block_on(async {
            let (database, connection, replica) = replica::Replica::open(&path, url, token).await?;
            if !fresh {
                replica.sync(&database, 15).await?;
            }
            Ok::<_, anyhow::Error>((database, connection, replica))
        })?;
        let store = Self {
            _database: std::rc::Rc::new(database),
            connection,
            runtime: std::rc::Rc::new(runtime),
            in_transaction: false,
            replica: Some(std::rc::Rc::new(replica)),
        };
        store.require_schema()?;
        Ok(store)
    }

    /// One atomic insert-only message write, including confirmation, under one deadline.
    pub fn insert_body_once(&self, message_id: &str, body: &CachedBody) -> Result<bool> {
        let _exclusive = self
            .replica
            .as_ref()
            .map(|r| r.lock.exclusive())
            .transpose()?;
        self.runtime.block_on(replica::write(self.replica.is_some(), async {
            let tx = self.connection.transaction_with_behavior(libsql::TransactionBehavior::Immediate).await?;
            tx.execute("INSERT INTO mail_identities(message_id, account) VALUES (?1, 'work-outlook') ON CONFLICT(message_id) DO NOTHING", params![message_id]).await?;
            let changed = tx.execute(
                "INSERT INTO mail_bodies(identity_id, body_text, body_format, cached_at, cached_to, cached_cc)
                 SELECT id, ?2, ?3, ?4, ?5, ?6 FROM mail_identities WHERE message_id=?1
                 ON CONFLICT(identity_id) DO NOTHING",
                params![message_id, body.body_text.clone(), body.body_format.clone(),
                    chrono::Utc::now().to_rfc3339(), serde_json::to_string(&body.to)?, serde_json::to_string(&body.cc)?],
            ).await?;
            tx.commit().await?;
            if let Some(replica) = &self.replica {
                replica.sync_locked(&self._database, 15).await?;
            }
            Ok(changed > 0)
        }))
    }
}
