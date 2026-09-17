mod runner;
#[cfg(test)]
mod tests;
mod timer;
use anyhow::{Result, ensure};
use chrono::NaiveDate;
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(clap::Args)]
#[command(subcommand_negates_reqs = true)]
pub struct Args {
    #[arg(long, required = true)]
    config: Option<PathBuf>,
    #[arg(long)]
    since: Option<NaiveDate>,
    #[arg(long)]
    json: bool,
    #[command(subcommand)]
    action: Option<Action>,
}
#[derive(clap::Subcommand)]
enum Action {
    InstallTimer {
        #[arg(long)]
        config: PathBuf,
        #[arg(long, default_value = "08:00")]
        at: String,
    },
    UninstallTimer {
        #[arg(long)]
        config: PathBuf,
    },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Config {
    senders: Vec<String>,
    #[serde(default = "one")]
    since_days: u64,
    until: NaiveDate,
    seen_file: PathBuf,
    out_file: PathBuf,
    wake_cmd: Option<String>,
    log_file: PathBuf,
}
fn one() -> u64 {
    1
}
impl Config {
    fn read(path: &Path) -> Result<Self> {
        let mut config: Self = toml::from_str(&std::fs::read_to_string(path)?)?;
        ensure!(!config.senders.is_empty(), "senders must not be empty");
        let home = timer::home()?;
        let base = std::fs::canonicalize(path)?.parent().unwrap().to_owned();
        for path in [
            &mut config.seen_file,
            &mut config.out_file,
            &mut config.log_file,
        ] {
            if let Ok(rest) = path.strip_prefix("~/") {
                *path = home.join(rest);
            } else if path.is_relative() {
                *path = base.join(&*path);
            }
        }
        ensure!(
            config.seen_file != config.out_file
                && config.seen_file != config.log_file
                && config.out_file != config.log_file,
            "watch paths must be distinct"
        );
        Ok(config)
    }
}
pub fn command(args: Args) -> String {
    if let Some(action) = args.action {
        return match timer::run(action) {
            Ok(path) => crate::cli::success(path),
            Err(e) => crate::cli::error(&format!("{e:#}"), "WATCH_TIMER_ERROR"),
        };
    }
    let result = (|| {
        let path = args
            .config
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("config required"))?;
        let config = Config::read(path)?;
        let mut report = runner::Report::default();
        let result = runner::run(&config, path, args.since, &mut runner::System, &mut report);
        if let Err(e) = result {
            report.error(&config, &format!("{e:#}"));
        }
        runner::log(
            &config,
            &format!(
                "DONE hits={} new={} errors={}",
                report.hits, report.new, report.errors
            ),
        );
        Ok::<_, anyhow::Error>(report)
    })();
    let report = result.unwrap_or_else(|e| {
        eprintln!("ERROR config: {e:#}; hits=0 new=0 errors=1");
        runner::Report {
            errors: 1,
            ..Default::default()
        }
    });
    if args.json {
        crate::cli::success(report)
    } else {
        format!(
            "hits={} new={} errors={}",
            report.hits, report.new, report.errors
        )
    }
}
