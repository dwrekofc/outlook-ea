mod source;
#[cfg(test)]
mod tests;
mod timer;
use crate::{body, cli, db::Store};
use anyhow::Result;
use clap::{Args as ClapArgs, Subcommand};
use serde::Serialize;
use std::{collections::HashSet, time::Instant};

#[derive(ClapArgs)]
pub struct Args {
    #[arg(long)]
    pub since: Option<chrono::NaiveDate>,
    #[arg(long)]
    pub all: bool,
    #[arg(long, default_value_t = 200)]
    pub limit: usize,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long, global = true)]
    pub json: bool,
    #[command(subcommand)]
    pub action: Option<TimerAction>,
}
#[derive(Subcommand)]
pub enum TimerAction {
    InstallTimer {
        #[arg(long, default_value = "15m")]
        interval: String,
    },
    UninstallTimer,
}
#[derive(Default, Serialize)]
pub struct Report {
    scanned: usize,
    cached: usize,
    skipped_existing: usize,
    would_cache: usize,
    failed: usize,
    failures: Vec<Failure>,
    stopped: Option<String>,
    dry_run: bool,
    duration_ms: u128,
}
#[derive(Serialize)]
struct Failure {
    id: i64,
    message_id: String,
    error: String,
}
#[derive(Clone)]
struct Message {
    id: i64,
    message_id: String,
    sent: i64,
}
fn replica_failure(error: &str) -> bool {
    error.contains("replica_")
}

fn capture(
    messages: &mut [Message],
    existing: &mut HashSet<String>,
    args: &Args,
    report: &mut Report,
    mut extract: impl FnMut(&Message) -> Result<body::CachedBody>,
    mut insert: impl FnMut(&Message, &body::CachedBody) -> Result<bool>,
) {
    messages.sort_by_key(|m| (std::cmp::Reverse(m.sent), std::cmp::Reverse(m.id)));
    let mut attempts = 0;
    for message in messages {
        if attempts >= args.limit {
            break;
        }
        report.scanned += 1;
        if existing.contains(&message.message_id) {
            report.skipped_existing += 1;
            continue;
        }
        attempts += 1;
        let result = (|| {
            anyhow::ensure!(!message.message_id.trim().is_empty(), "missing message_id");
            if args.dry_run {
                report.would_cache += 1;
                existing.insert(message.message_id.clone());
                return Ok(());
            }
            let body = extract(message)?;
            if insert(message, &body)? {
                report.cached += 1;
            } else {
                report.skipped_existing += 1;
            }
            existing.insert(message.message_id.clone());
            Ok(())
        })();
        if let Err(error) = result {
            let error = format!("{error:#}");
            let stop = replica_failure(&error);
            report.failed += 1;
            report.failures.push(Failure {
                id: message.id,
                message_id: message.message_id.clone(),
                error: error.clone(),
            });
            if stop {
                report.stopped = Some(error);
                break;
            }
        }
    }
}

pub fn command(args: Args) -> String {
    if let Some(action) = &args.action {
        return match timer::run(action) {
            Ok(value) => cli::success(value),
            Err(error) => cli::error(&format!("{error:#}"), "TIMER_ERROR"),
        };
    }
    let started = Instant::now();
    let mut report = Report {
        dry_run: args.dry_run,
        ..Report::default()
    };
    let result = (|| -> Result<()> {
        let envelope = crate::data::open_envelope_index(&crate::data::find_envelope_index()?)?;
        let mut messages = source::messages(&envelope, &args)?;
        let store = Store::open_body_cache(args.dry_run)?;
        let mut existing = source::existing(&store)?;
        capture(
            &mut messages,
            &mut existing,
            &args,
            &mut report,
            |m| Ok(body::extract_email_body(m.id, !args.all)?),
            |m, b| store.insert_body_once(&m.message_id, b),
        );
        Ok(())
    })();
    if let Err(error) = result {
        let detail = format!("{error:#}");
        if !replica_failure(&detail) {
            return cli::error(&detail, "CACHE_BODIES_ERROR");
        }
        report.stopped = Some(detail);
    }
    report.duration_ms = started.elapsed().as_millis();
    if let Some(reason) = &report.stopped {
        eprintln!("cache-bodies: {reason}");
    }
    if args.json {
        cli::success(&report)
    } else {
        format!(
            "scanned={} cached={} skipped-existing={} would-cache={} failed={} duration={}ms{}{}",
            report.scanned,
            report.cached,
            report.skipped_existing,
            report.would_cache,
            report.failed,
            report.duration_ms,
            report
                .stopped
                .as_ref()
                .map(|s| format!(" stopped={s}"))
                .unwrap_or_default(),
            report
                .failures
                .iter()
                .map(|f| format!("\n{} {}: {}", f.id, f.message_id, f.error))
                .collect::<String>()
        )
    }
}
