use std::process::Command;

#[test]
fn test_exit_code_zero_on_success_commands() {
    // `mea rules list` should succeed even without Mail.app if rules.toml exists or defaults
    // But we can test that clap parsing works — an unknown command gives non-zero from clap
    let output = Command::new(env!("CARGO_BIN_EXE_mea"))
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
    let output = Command::new(env!("CARGO_BIN_EXE_mea"))
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
    let output = Command::new(env!("CARGO_BIN_EXE_mea"))
        .args(["read", "999999999"])
        .output()
        .expect("failed to run mea");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.is_empty(), "stderr should be empty, got: {stderr}");
}

#[test]
fn test_patterns_md_created() {
    // When mea creates ~/.mea/, PATTERNS.md should be bootstrapped
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let patterns_path = std::path::PathBuf::from(&home)
        .join(".mea")
        .join("PATTERNS.md");

    // `mea label` triggers open_overlay() before open_envelope()
    // The label command will error, but open_overlay() runs first
    let _ = Command::new(env!("CARGO_BIN_EXE_mea"))
        .args(["label", "1", "1"])
        .output();

    assert!(
        patterns_path.exists(),
        "PATTERNS.md should exist at {patterns_path:?}"
    );
}
