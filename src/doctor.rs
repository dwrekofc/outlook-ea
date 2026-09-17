use std::{path::PathBuf, process::Command};
pub fn command() -> String {
    let binary = std::env::current_exe().unwrap_or_default();
    let read = (|| -> anyhow::Result<()> {
        let path = crate::data::find_envelope_index()?;
        let store = crate::data::open_envelope_index(&path)?;
        store.one("SELECT ROWID FROM messages LIMIT 1", (), |row| {
            Ok(row.get::<i64>(0)?)
        })?;
        Ok(())
    })();
    let mut lines = vec![match read {
        Ok(()) => format!("FDA: ok — {}", binary.display()),
        Err(e) => format!(
            "FDA: denied — grant Full Disk Access to {}\nRead error: {e:#}",
            binary.display()
        ),
    }];
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    match std::fs::read_dir(home.join("Library/LaunchAgents")) {
        Ok(entries) => {
            let mut paths: Vec<_> = entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .is_some_and(|n| n.to_string_lossy().starts_with("com.mea."))
                        && p.extension().is_some_and(|e| e == "plist")
                })
                .collect();
            paths.sort();
            for path in paths {
                let output = Command::new("/usr/bin/plutil")
                    .args(["-convert", "json", "-o", "-"])
                    .arg(&path)
                    .output();
                let value = output
                    .ok()
                    .filter(|o| o.status.success())
                    .and_then(|o| serde_json::from_slice::<serde_json::Value>(&o.stdout).ok());
                let executable = value.as_ref().and_then(|v| {
                    v.get("Program")
                        .and_then(|v| v.as_str())
                        .or_else(|| v.get("ProgramArguments")?.get(0)?.as_str())
                });
                let same = executable.is_some_and(|e| {
                    std::fs::canonicalize(e).ok() == std::fs::canonicalize(&binary).ok()
                });
                lines.push(format!(
                    "{} executable={} this_binary={same}",
                    path.file_stem().unwrap().to_string_lossy(),
                    executable.unwrap_or("unknown")
                ));
            }
        }
        Err(e) => lines.push(format!("LaunchAgents: {e}")),
    }
    lines.join("\n")
}
