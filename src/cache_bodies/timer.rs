use super::TimerAction;
use anyhow::{Result, ensure};
use serde::Serialize;
use std::{path::PathBuf, process::Command};
const LABEL: &str = "com.mea.cache-bodies";
#[derive(Serialize)]
struct Timer {
    installed: bool,
    path: PathBuf,
    interval_seconds: Option<u32>,
}
pub fn seconds(value: &str) -> Result<u32> {
    let (number, multiplier) = if let Some(n) = value.strip_suffix('m') {
        (n, 60)
    } else if let Some(n) = value.strip_suffix('h') {
        (n, 3600)
    } else {
        (value.strip_suffix('s').unwrap_or(value), 1)
    };
    let seconds = number
        .parse::<u32>()?
        .checked_mul(multiplier)
        .filter(|n| *n > 0);
    seconds.ok_or_else(|| {
        anyhow::anyhow!("interval must be a positive number of seconds, minutes (m), or hours (h)")
    })
}
fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
pub fn plist(binary: &str, interval: u32, home: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>{LABEL}</string>
<key>ProgramArguments</key><array><string>{}</string><string>cache-bodies</string><string>--limit</string><string>200</string><string>--json</string></array>
<key>StartInterval</key><integer>{interval}</integer>
<key>RunAtLoad</key><true/>
<key>EnvironmentVariables</key><dict><key>HOME</key><string>{}</string><key>PATH</key><string>/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin</string></dict>
<key>StandardOutPath</key><string>{}/.config/vault/logs/mea-cache-bodies.log</string>
<key>StandardErrorPath</key><string>{}/.config/vault/logs/mea-cache-bodies.log</string>
</dict></plist>
"#,
        xml(binary),
        xml(home),
        xml(home),
        xml(home)
    )
}
pub fn run(command: &TimerAction) -> Result<serde_json::Value> {
    let installed = match command {
        TimerAction::InstallTimer { .. } => true,
        TimerAction::UninstallTimer => false,
    };
    ensure!(cfg!(target_os = "macos"), "timers require macOS launchd");
    let home =
        PathBuf::from(std::env::var_os("HOME").ok_or_else(|| anyhow::anyhow!("HOME is missing"))?);
    let path = home.join("Library/LaunchAgents/com.mea.cache-bodies.plist");
    let uid = Command::new("/usr/bin/id").arg("-u").output()?;
    ensure!(uid.status.success(), "cannot determine launchd user");
    let domain = format!("gui/{}", String::from_utf8(uid.stdout)?.trim());
    let service = format!("{domain}/{LABEL}");
    let interval_seconds = if let TimerAction::InstallTimer { interval } = command {
        Some(seconds(interval)?)
    } else {
        None
    };
    let binary = std::env::current_exe()?;
    let loaded = Command::new("/bin/launchctl")
        .args(["print", &service])
        .output()?
        .status
        .success();
    if loaded {
        let output = Command::new("/bin/launchctl")
            .args(["bootout", &service])
            .output()?;
        ensure!(
            output.status.success(),
            "launchctl bootout: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    if let Some(interval) = interval_seconds {
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::create_dir_all(home.join(".config/vault/logs"))?;
        std::fs::write(
            &path,
            plist(
                &binary.display().to_string(),
                interval,
                &home.display().to_string(),
            ),
        )?;
        let output = Command::new("/bin/launchctl")
            .args(["bootstrap", &domain])
            .arg(&path)
            .output()?;
        ensure!(
            output.status.success(),
            "launchctl bootstrap: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    } else if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(serde_json::to_value(Timer {
        installed,
        path,
        interval_seconds,
    })?)
}
