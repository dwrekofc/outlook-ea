The role of this file is to describe common mistakes and confusion points that agents might encounter as they work in this project. 

If you ever encounter something in the project that surprises you, please alert the developer working with you and indicate that this is the case to help prevent future agents from having the same issue.

This project is super green field and no one is using it yet. we are focused on getting it in the right shape.

## Build & Run

- Language: Rust (edition 2024)
- Binary: `mea` (CLI tool, JSON output, agent-facing)
- Library: `src/lib.rs` with modules: db, data, labels, body, search, rules, actions, triage, cli
- Build: `cargo build`
- Run: `cargo run -- <command>` (e.g., `cargo run -- list`)

## Validation

- Tests: `cargo nextest run` (fallback: `cargo test`)
- Clippy: `cargo clippy --all-targets -- -D warnings`
- Format check: `cargo fmt --all -- --check`

## Operational Notes

### Codebase Patterns

- Apple Mail dates use NSDate epoch (seconds since 2001-01-01, offset 978307200 from Unix epoch)
- Overlay DB at `~/.mea/overlay.db` (SQLite, user metadata not in Apple Mail)
- Rules config at `~/.mea/rules.toml` (TOML, editable by Claude skill)
- Apple Mail Envelope Index at `~/Library/Mail/V*/MailData/Envelope Index` (read-only SQLite)
- Email files are `.emlx` format under `~/Library/Mail/V*/` (first line = byte count, then RFC 2822 message)
- `test_bulk_action_no_vip` test takes ~120s because osascript hangs in test env — expected behavior
- All tests use in-memory SQLite mocks for Envelope Index — no real Mail.app dependency in tests
