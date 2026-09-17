use super::{
    Config,
    runner::{self, Report, Runtime},
    timer,
};
use anyhow::Result;
use chrono::NaiveDate;
use std::path::Path;
struct Fake {
    date: NaiveDate,
    wakes: usize,
    unloads: usize,
    fail: bool,
}
impl Runtime for Fake {
    fn today(&self) -> NaiveDate {
        self.date
    }
    fn test_clock(&self) -> bool {
        true
    }
    fn search(&mut self, _: &str, since: &str) -> Result<Vec<(String, String)>> {
        assert!(since.starts_with("2026-09-15T00:00:00"));
        if self.fail {
            anyhow::bail!("search failed")
        }
        Ok(vec![("42".into(), "- hello (mea 42)".into())])
    }
    fn wake(&mut self, _: &str, hits: usize) -> Result<()> {
        assert_eq!(hits, 1);
        self.wakes += 1;
        Ok(())
    }
    fn unload(&mut self, path: &Path) -> Result<()> {
        assert_eq!(timer::label(path)?, "com.mea.watch-avengers");
        self.unloads += 1;
        Ok(())
    }
}
fn fixture(dir: &Path) -> (Config, Fake) {
    (
        Config {
            senders: vec!["name".into(), "address".into()],
            since_days: 1,
            until: "2026-09-30".parse().unwrap(),
            seen_file: dir.join("seen"),
            out_file: dir.join("out"),
            log_file: dir.join("log"),
            wake_cmd: Some("wake $HITS".into()),
        },
        Fake {
            date: "2026-09-16".parse().unwrap(),
            wakes: 0,
            unloads: 0,
            fail: false,
        },
    )
}
#[test]
fn dedupe_and_wake_once_with_output_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let (config, mut fake) = fixture(dir.path());
    let mut report = Report::default();
    runner::run(
        &config,
        Path::new("avengers.toml"),
        None,
        &mut fake,
        &mut report,
    )
    .unwrap();
    assert_eq!(
        (report.hits, report.new, report.errors, fake.wakes),
        (1, 1, 0, 1)
    );
    for recover in [false, true] {
        if recover {
            std::fs::remove_file(&config.seen_file).unwrap();
        }
        let mut report = Report::default();
        runner::run(
            &config,
            Path::new("avengers.toml"),
            None,
            &mut fake,
            &mut report,
        )
        .unwrap();
        assert_eq!((report.new, fake.wakes), (0, 1));
    }
    assert_eq!(
        std::fs::read_to_string(config.out_file)
            .unwrap()
            .lines()
            .count(),
        1
    );
}
#[test]
fn expiry_uses_injected_clock_and_labels_test() {
    let dir = tempfile::tempdir().unwrap();
    let (config, mut fake) = fixture(dir.path());
    fake.date = "2026-10-01".parse().unwrap();
    runner::run(
        &config,
        Path::new("avengers.toml"),
        None,
        &mut fake,
        &mut Report::default(),
    )
    .unwrap();
    assert_eq!((fake.unloads, fake.wakes), (1, 0));
    assert!(
        std::fs::read_to_string(config.log_file)
            .unwrap()
            .contains("TEST EXPIRED")
    );
}
#[test]
fn search_errors_are_counted() {
    let dir = tempfile::tempdir().unwrap();
    let (config, mut fake) = fixture(dir.path());
    fake.fail = true;
    let mut report = Report::default();
    runner::run(
        &config,
        Path::new("avengers.toml"),
        None,
        &mut fake,
        &mut report,
    )
    .unwrap();
    assert_eq!((report.errors, fake.wakes), (2, 0));
}
#[test]
fn timer_runs_binary_directly_and_escapes_xml() {
    let plist = timer::plist(
        Path::new("/bin/me&a"),
        Path::new("/tmp/avengers.toml"),
        "08:15",
    )
    .unwrap();
    assert!(plist.contains("<array><string>/bin/me&amp;a</string><string>watch</string>"));
    assert!(plist.contains("<integer>15</integer>"));
    assert!(timer::plist(Path::new("/bin/mea"), Path::new("avengers.toml"), "25:00").is_err());
}

#[test]
fn expiry_boundary_is_inclusive() {
    let dir = tempfile::tempdir().unwrap();
    let (mut config, mut fake) = fixture(dir.path());
    config.until = fake.date;
    runner::run(
        &config,
        Path::new("avengers.toml"),
        None,
        &mut fake,
        &mut Report::default(),
    )
    .unwrap();
    assert_eq!(fake.unloads, 0);
}

#[test]
fn wake_shell_substitutes_hits_and_reports_failure() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("count");
    runner::System
        .wake(&format!("printf '%s' '$HITS' > '{}'", path.display()), 3)
        .unwrap();
    assert_eq!(std::fs::read_to_string(path).unwrap(), "3");
    assert!(runner::System.wake("exit 7", 1).is_err());
}

#[test]
fn cli_parses_run_and_timer_forms() {
    use clap::Parser;
    for args in [
        vec![
            "mea",
            "watch",
            "--config",
            "avengers.toml",
            "--since",
            "2026-09-10",
            "--json",
        ],
        vec![
            "mea",
            "watch",
            "install-timer",
            "--config",
            "avengers.toml",
            "--at",
            "09:30",
        ],
        vec![
            "mea",
            "watch",
            "uninstall-timer",
            "--config",
            "avengers.toml",
        ],
    ] {
        assert!(crate::cli::Cli::try_parse_from(args).is_ok());
    }
    assert!(crate::cli::Cli::try_parse_from(["mea", "watch"]).is_err());
}
