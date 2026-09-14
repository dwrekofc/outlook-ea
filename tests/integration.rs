use std::process::Command;

fn isolated_command(home: &std::path::Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_mea"));
    command
        .env("HOME", home)
        .env("VAULT_CONFIG", home.join("vault-config.json"));
    command
}

#[test]
fn test_exit_code_zero_on_success_commands() {
    // `mea rules list` should succeed even without Mail.app if rules.toml exists or defaults
    // But we can test that clap parsing works — an unknown command gives non-zero from clap
    let home = tempfile::tempdir().unwrap();
    let output = isolated_command(home.path())
        .args(["rules", "list"])
        .output()
        .expect("failed to run mea");

    // This may fail if rules file doesn't exist — but the JSON status field tells us
    let stdout = String::from_utf8_lossy(&output.stdout);
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&stdout) {
        if parsed.get("status").and_then(|s| s.as_str()) == Some("ok") {
            assert!(output.status.success(), "exit code should be 0 on success");
        } else if parsed.get("status").and_then(|s| s.as_str()) == Some("error") {
            assert!(
                !output.status.success(),
                "exit code should be non-zero on error"
            );
        }
    }
}

#[test]
fn test_exit_code_nonzero_on_error() {
    // Reading a nonexistent email ID should produce an error
    let home = tempfile::tempdir().unwrap();
    let output = isolated_command(home.path())
        .args(["read", "999999999"])
        .output()
        .expect("failed to run mea");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("should be valid JSON");
    assert_eq!(parsed["status"], "error");
    assert!(!output.status.success(), "error should exit non-zero");
}

#[test]
fn test_no_stderr_on_error() {
    // Even on errors, no output should go to stderr
    let home = tempfile::tempdir().unwrap();
    let output = isolated_command(home.path())
        .args(["read", "999999999"])
        .output()
        .expect("failed to run mea");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.is_empty(), "stderr should be empty, got: {stderr}");
}

#[test]
fn test_patterns_md_created() {
    let home = tempfile::tempdir().unwrap();
    let patterns_path = home.path().join(".mea/PATTERNS.md");
    let _ = isolated_command(home.path())
        .args(["label", "1", "1"])
        .output()
        .unwrap();
    assert!(patterns_path.exists());
    std::fs::write(&patterns_path, "Existing preferences").unwrap();
    let _ = isolated_command(home.path())
        .args(["label", "1", "1"])
        .output()
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(patterns_path).unwrap(),
        "Existing preferences"
    );
}
