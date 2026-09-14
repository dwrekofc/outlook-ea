use super::VaultSettings;
use anyhow::{Context, Result};
use std::{fs, path::PathBuf};

impl VaultSettings {
    pub(crate) fn load() -> Result<Self> {
        Self::load_from(&config_path()?)
    }

    pub(super) fn load_from(path: &std::path::Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = fs::read_to_string(path)
            .with_context(|| format!("failed to read Vault settings at {}", path.display()))?;
        serde_json::from_str(&contents)
            .with_context(|| format!("invalid Vault settings at {}", path.display()))
    }
}

pub(super) fn is_remote(url: &str) -> bool {
    url.starts_with("libsql://") || url.starts_with("https://") || url.starts_with("http://")
}

fn config_path() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("VAULT_CONFIG") {
        return Ok(PathBuf::from(path));
    }
    Ok(home_dir()?.join(".config/vault/config.json"))
}

fn home_dir() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .context("HOME is not set; cannot determine Vault configuration directory")
}

impl VaultSettings {
    pub(crate) fn mea_dump_path(&self) -> Result<PathBuf> {
        let notes = self
            .notes_path
            .clone()
            .map(Ok)
            .unwrap_or_else(|| Ok::<_, anyhow::Error>(home_dir()?.join("vault")))?;
        let notes = if notes.starts_with("~") {
            home_dir()?.join(notes.strip_prefix("~")?)
        } else {
            notes
        };
        Ok(notes.join("utilities/context-profiles/mea/MEA_GRAPH_CONTEXT.md"))
    }
}
