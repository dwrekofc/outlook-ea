use mea::db::{Store, replica::guard::ReplicaLock};
use serde_json::{Value, json};
use std::{path::Path, process::Command};

fn envelope(home: &Path) {
    let directory = home.join("Library/Mail/V10/MailData");
    std::fs::create_dir_all(&directory).unwrap();
    let store = Store::connect(directory.join("Envelope Index").to_str().unwrap(), "").unwrap();
    store
        .execute_batch(
            "CREATE TABLE messages(mailbox INTEGER, global_message_id INTEGER, date_sent INTEGER);
        CREATE TABLE mailboxes(url TEXT); CREATE TABLE message_global_data(message_id_header TEXT);
        INSERT INTO mailboxes VALUES ('ews://test/Inbox');
        INSERT INTO message_global_data VALUES ('fixture');
        INSERT INTO messages VALUES (1, 1, 10);",
        )
        .unwrap();
}
fn command(home: &Path, config: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_mea"));
    command
        .env("HOME", home)
        .env("VAULT_CONFIG", config)
        .env_remove("VAULT_DATABASE_URL")
        .env_remove("VAULT_AUTH_TOKEN");
    command
}
#[test]
fn busy_replica_exits_zero_with_counts_without_network() {
    let temp = tempfile::tempdir().unwrap();
    envelope(temp.path());
    let config = temp.path().join("config.json");
    std::fs::write(
        &config,
        json!({"database_url": "libsql://unused.invalid", "auth_token": "fixture"}).to_string(),
    )
    .unwrap();
    let path = temp.path().join("replica/mea.db");
    let lock = ReplicaLock::open(&path).unwrap();
    let _exclusive = lock.exclusive().unwrap();
    let started = std::time::Instant::now();
    let output = command(temp.path(), &config)
        .args(["cache-bodies", "--json"])
        .output()
        .unwrap();
    assert!(started.elapsed().as_secs() < 5);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"], "ok");
    assert_eq!(report["data"]["cached"], 0);
    assert_eq!(report["data"]["scanned"], 0);
    assert!(
        report["data"]["stopped"]
            .as_str()
            .unwrap()
            .contains("replica_busy")
    );
}
#[test]
fn dry_run_leaves_database_unchanged_without_body_files() {
    let temp = tempfile::tempdir().unwrap();
    envelope(temp.path());
    let path = temp.path().join("cache.db");
    {
        let store = Store::connect(path.to_str().unwrap(), "").unwrap();
        store
            .execute_batch(
                "CREATE TABLE mail_identities(id INTEGER PRIMARY KEY, message_id TEXT);
            CREATE TABLE mail_bodies(identity_id INTEGER PRIMARY KEY);",
            )
            .unwrap();
    }
    let before = std::fs::read(&path).unwrap();
    let config = temp.path().join("config.json");
    std::fs::write(&config, json!({"database_url": path}).to_string()).unwrap();
    let output = command(temp.path(), &config)
        .args(["cache-bodies", "--dry-run", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["data"]["would_cache"], 1);
    assert_eq!(report["data"]["cached"], 0);
    assert_eq!(std::fs::read(path).unwrap(), before);
}
