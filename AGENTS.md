The role of this file is to describe common mistakes and confusion points that agents might encounter as they work in this project. 

If you ever encounter something in the project that surprises you, please alert the developer working with you and indicate that this is the case to help prevent future agents from having the same issue.

This project is super green field and no one is using it yet. we are focused on getting it in the right shape.

## Build & Run

- Language: Rust (edition 2024)
- Application: `mea` CLI (Clap); this is not a GPUI app.
- Build: `cargo build`
- Run: `cargo run`

## Validation

- Tests: `cargo nextest run` (fallback: `cargo test`)
- Clippy: `cargo clippy --all-targets -- -D warnings`
- Format check: `cargo fmt --all -- --check`

## Operational Notes

### Codebase Patterns

- Vault owns Turso schema (`graph_*`, `mail_*`) and credentials in `~/.config/vault/config.json` (or `VAULT_CONFIG`). Do not add MEA migrations or a credential store.
- `src/db/connection.rs` isolates connection setup. D17 requires a later vault-owned replica; MEA currently follows vault's remote connection.
- Apple Mail’s Envelope Index remains read-only. Use libsql for both adapters: mixing rusqlite and libsql caused SQLite initialization failures.
- Mail rowids are aliases; use message IDs for shared labels/bodies. Canonical graph IDs differ from legacy IDs retained in import provenance.
- `skill/*.md` guides are repo-owned copies; they were external symlinks before migration. Do not recreate those links or edit the vault through them.

- D17 follow-up: preserve `Store::transaction` immediate transactions around complete writes, including the read phase of read-modify-write. Nested helpers must use the scoped store and propagate errors.
- Schema 4 separates immutable imported rowid aliases from live `(hostname, rowid)` aliases. Distinct Macs need distinct hostnames; never use unscoped import aliases for live label lookups.
