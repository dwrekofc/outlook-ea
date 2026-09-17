use super::{Action, Config};
use anyhow::{Result, ensure};
use std::{
    path::{Path, PathBuf},
    process::Command,
};
pub(super) fn home() -> Result<PathBuf> {
    Ok(PathBuf::from(
        std::env::var_os("HOME").ok_or_else(|| anyhow::anyhow!("HOME missing"))?,
    ))
}
pub(super) fn label(config: &Path) -> Result<String> {
    let name = config.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    ensure!(
        !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
        "config name must contain only letters, digits, - or _"
    );
    Ok(format!("com.mea.watch-{name}"))
}
fn domain() -> Result<String> {
    let output = Command::new("/usr/bin/id").arg("-u").output()?;
    ensure!(output.status.success(), "id failed");
    Ok(format!("gui/{}", String::from_utf8(output.stdout)?.trim()))
}
pub(super) fn unload(config: &Path) -> Result<()> {
    let service = format!("{}/{}", domain()?, label(config)?);
    if Command::new("/bin/launchctl")
        .args(["print", &service])
        .output()?
        .status
        .success()
    {
        let output = Command::new("/bin/launchctl")
            .args(["bootout", &service])
            .output()?;
        ensure!(
            output.status.success(),
            "bootout failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}
fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
pub(super) fn plist(binary: &Path, config: &Path, at: &str) -> Result<String> {
    let time = chrono::NaiveTime::parse_from_str(at, "%H:%M")?;
    use chrono::Timelike;
    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>{}</string>
<key>ProgramArguments</key><array><string>{}</string><string>watch</string><string>--config</string><string>{}</string></array>
<key>StartCalendarInterval</key><dict><key>Hour</key><integer>{}</integer><key>Minute</key><integer>{}</integer></dict>
<key>RunAtLoad</key><false/>
<key>EnvironmentVariables</key><dict><key>HOME</key><string>{}</string><key>PATH</key><string>/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin</string></dict>
</dict></plist>
"#,
        label(config)?,
        xml(&binary.display().to_string()),
        xml(&config.display().to_string()),
        time.hour(),
        time.minute(),
        xml(&home()?.display().to_string())
    ))
}
pub(super) fn run(action: Action) -> Result<PathBuf> {
    ensure!(cfg!(target_os = "macos"), "timers require macOS");
    let (config, at) = match action {
        Action::InstallTimer { config, at } => (config, Some(at)),
        Action::UninstallTimer { config } => (config, None),
    };
    let path = home()?
        .join("Library/LaunchAgents")
        .join(format!("{}.plist", label(&config)?));
    // Validate everything before unloading an existing timer.
    let content = if let Some(at) = at {
        Config::read(&config)?;
        Some(plist(
            &std::env::current_exe()?,
            &std::fs::canonicalize(&config)?,
            &at,
        )?)
    } else {
        None
    };
    unload(&config)?;
    if let Some(content) = content {
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(&path, content)?;
        let output = Command::new("/bin/launchctl")
            .args(["bootstrap", &domain()?])
            .arg(&path)
            .output()?;
        ensure!(
            output.status.success(),
            "bootstrap failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    } else if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(path)
}
