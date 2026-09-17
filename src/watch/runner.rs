use super::{Config, timer};
use anyhow::{Result, ensure};
use chrono::{Days, Local, NaiveDate, TimeZone};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::Write,
    path::Path,
    process::Command,
};

#[derive(Default, Serialize)]
pub(super) struct Report {
    pub hits: usize,
    pub new: usize,
    pub errors: usize,
}
impl Report {
    pub fn error(&mut self, config: &Config, message: &str) {
        self.errors += 1;
        log(config, &format!("ERROR {}", clean(message)));
    }
}
pub(super) fn clean(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}
pub(super) fn append(path: &Path) -> Result<File> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(OpenOptions::new().create(true).append(true).open(path)?)
}
pub(super) fn log(config: &Config, message: &str) {
    let line = format!("{} {message}", Local::now().to_rfc3339());
    if let Err(e) = append(&config.log_file).and_then(|mut f| Ok(writeln!(f, "{line}")?)) {
        eprintln!("{line}; log write failed: {e}");
    }
}
pub(super) trait Runtime {
    fn today(&self) -> NaiveDate;
    fn test_clock(&self) -> bool {
        false
    }
    fn search(&mut self, sender: &str, since: &str) -> Result<Vec<(String, String)>>;
    fn wake(&mut self, command: &str, hits: usize) -> Result<()>;
    fn unload(&mut self, config: &Path) -> Result<()>;
}
pub(super) struct System;
impl Runtime for System {
    fn today(&self) -> NaiveDate {
        Local::now().date_naive()
    }
    fn search(&mut self, sender: &str, since: &str) -> Result<Vec<(String, String)>> {
        let path = crate::data::find_envelope_index()?;
        let envelope = crate::data::open_envelope_index(&path)?;
        let query = crate::search::SearchQuery {
            sender: Some(sender.into()),
            date_from: Some(since.into()),
            ..Default::default()
        };
        Ok(crate::search::search_metadata(&envelope, &query)?
            .into_iter()
            .map(|e| {
                (
                    e.id.to_string(),
                    format!(
                        "- {} {} <{}> — {} (mea {})",
                        clean(&e.date),
                        clean(&e.sender_name),
                        clean(&e.sender_address),
                        clean(&e.subject),
                        e.id
                    ),
                )
            })
            .collect())
    }
    fn wake(&mut self, command: &str, hits: usize) -> Result<()> {
        let mut child = Command::new("/bin/sh")
            .args(["-c", &command.replace("$HITS", &hits.to_string())])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(90);
        loop {
            if let Some(status) = child.try_wait()? {
                ensure!(status.success(), "wake failed: {status}");
                break;
            }
            if std::time::Instant::now() >= deadline {
                child.kill()?;
                child.wait()?;
                anyhow::bail!("wake timed out after 90 seconds");
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        Ok(())
    }
    fn unload(&mut self, config: &Path) -> Result<()> {
        timer::unload(config)
    }
}
fn read(path: &Path) -> Result<String> {
    match fs::read_to_string(path) {
        Ok(s) => Ok(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e.into()),
    }
}
pub(super) fn run(
    config: &Config,
    path: &Path,
    since: Option<NaiveDate>,
    runtime: &mut impl Runtime,
    report: &mut Report,
) -> Result<()> {
    let today = runtime.today();
    let prefix = if runtime.test_clock() { "TEST " } else { "" };
    log(config, &format!("{prefix}RUN today={today}"));
    if std::env::var_os("WATCH_TODAY").is_some() {
        log(config, "TEST WATCH_TODAY ignored; expiry uses system clock");
    }
    if today > config.until {
        log(
            config,
            &format!(
                "{prefix}EXPIRED today={today}; unloading {}",
                timer::label(path)?
            ),
        );
        return runtime.unload(path);
    }
    let lock = append(&config.seen_file.with_extension("lock"))?;
    match lock.try_lock() {
        Ok(()) => (),
        Err(std::fs::TryLockError::WouldBlock) => {
            log(config, "SKIP another watcher is running");
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    }
    let since = since
        .or_else(|| today.checked_sub_days(Days::new(config.since_days)))
        .ok_or_else(|| anyhow::anyhow!("since_days out of range"))?;
    let since = Local
        .from_local_datetime(&since.and_hms_opt(0, 0, 0).unwrap())
        .earliest()
        .ok_or_else(|| anyhow::anyhow!("invalid local midnight"))?
        .to_rfc3339();
    let mut seen: BTreeSet<String> = read(&config.seen_file)?
        .lines()
        .map(str::to_owned)
        .collect();
    for line in read(&config.out_file)?.lines() {
        if let Some((_, id)) = line.rsplit_once("(mea ")
            && let Some(id) = id.strip_suffix(')')
            && id.parse::<i64>().is_ok()
        {
            seen.insert(id.into());
        }
    }
    let mut hits = BTreeMap::new();
    for sender in &config.senders {
        match runtime.search(sender, &since) {
            Ok(rows) => {
                log(
                    config,
                    &format!("search sender={sender:?} since={since} hits={}", rows.len()),
                );
                hits.extend(rows);
            }
            Err(e) => report.error(config, &format!("sender={sender:?}: {e:#}")),
        }
    }
    report.hits = hits.len();
    hits.retain(|id, _| !seen.contains(id));
    report.new = hits.len();
    if !hits.is_empty() {
        let mut output = append(&config.out_file)?;
        let mut lines: Vec<_> = hits.values().collect();
        lines.sort();
        for line in lines {
            writeln!(output, "{line}")?;
        }
        output.sync_all()?;
    }
    seen.extend(hits.into_keys());
    let temporary = config.seen_file.with_extension("seen.tmp");
    fs::write(
        &temporary,
        seen.into_iter()
            .map(|id| format!("{id}\n"))
            .collect::<String>(),
    )?;
    fs::rename(temporary, &config.seen_file)?;
    if report.new > 0
        && let Some(command) = &config.wake_cmd
        && let Err(e) = runtime.wake(command, report.new)
    {
        report.error(config, &format!("wake: {e:#}"));
    }
    Ok(())
}
