//! flock upgrades need an admission gate: conversion may release the shared lock.
use anyhow::{Context, Result};
use std::{
    fs::{File, OpenOptions},
    os::{
        fd::AsRawFd,
        unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Path, PathBuf},
};

pub struct ReplicaLock {
    held: File,
    gate: File,
    path: PathBuf,
}
pub struct Exclusive<'a> {
    lock: &'a ReplicaLock,
}
fn flock(file: &File, operation: i32) -> std::io::Result<()> {
    // SAFETY: the File owns a valid descriptor for the duration of this call.
    if unsafe { libc::flock(file.as_raw_fd(), operation) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}
fn private_file(path: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(path)?;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    Ok(file)
}
pub fn protect(path: &Path) -> Result<()> {
    let parent = path.parent().context("replica directory missing")?;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)?;
    std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
    for entry in std::fs::read_dir(parent)? {
        let entry = entry?;
        if entry.file_type()?.is_file()
            && entry.file_name().to_string_lossy().starts_with(
                path.file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .as_ref(),
            )
        {
            match std::fs::set_permissions(entry.path(), std::fs::Permissions::from_mode(0o600)) {
                Ok(()) => (),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Err(error) => return Err(error.into()),
            }
        }
    }
    Ok(())
}
impl ReplicaLock {
    pub fn open(path: &Path) -> Result<Self> {
        protect(path)?;
        let lock = Self {
            path: path.into(),
            held: private_file(&path.with_extension("lock"))?,
            gate: private_file(&path.with_extension("gate"))?,
        };
        flock(&lock.gate, libc::LOCK_EX | libc::LOCK_NB)
            .context("replica_busy: another process is using the replica; retry later")?;
        let result = flock(&lock.held, libc::LOCK_SH | libc::LOCK_NB);
        flock(&lock.gate, libc::LOCK_UN)?;
        result.context("replica_busy: replica is syncing or writing; retry later")?;
        Ok(lock)
    }
    pub fn exclusive(&self) -> Result<Exclusive<'_>> {
        flock(&self.gate, libc::LOCK_EX | libc::LOCK_NB)
            .context("replica_busy: another process is using the replica; retry later")?;
        if let Err(error) = flock(&self.held, libc::LOCK_EX | libc::LOCK_NB) {
            // Gate prevents another upgrade while shared protection is restored.
            flock(&self.held, libc::LOCK_SH)?;
            flock(&self.gate, libc::LOCK_UN)?;
            return Err(error).context(
                "replica_busy: another connection is open; close it before sync, write, or reset",
            );
        }
        Ok(Exclusive { lock: self })
    }
}
impl Drop for Exclusive<'_> {
    fn drop(&mut self) {
        let _ = protect(&self.lock.path);
        let _ = flock(&self.lock.held, libc::LOCK_SH);
        let _ = flock(&self.lock.gate, libc::LOCK_UN);
    }
}
pub fn files(path: &Path) -> Vec<PathBuf> {
    ["", "-wal", "-shm", "-info", "-client_wal_index"]
        .iter()
        .map(|suffix| PathBuf::from(format!("{}{suffix}", path.display())))
        .chain([path.with_extension("sync.json")])
        .collect()
}
pub fn reset(path: &Path) -> Result<()> {
    let lock = ReplicaLock::open(path)?;
    let _exclusive = lock.exclusive()?;
    for file in files(path) {
        match std::fs::remove_file(file) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
