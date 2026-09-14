use std::{fs, io::Write, path::PathBuf};

pub(super) fn bootstrap() -> std::io::Result<()> {
    let directory =
        PathBuf::from(std::env::var_os("HOME").unwrap_or_else(|| ".".into())).join(".mea");
    fs::create_dir_all(&directory)?;
    match fs::OpenOptions::new().write(true).create_new(true).open(directory.join("PATTERNS.md")) {
        Ok(mut file) => file.write_all(b"# Learned Triage Preferences\n\nThis file records patterns observed during triage sessions.\nThe skill wrapper updates it as the user makes consistent decisions.\n\n## Sender Patterns\n\n## Subject Patterns\n\n## Notes\n"),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error),
    }
}
