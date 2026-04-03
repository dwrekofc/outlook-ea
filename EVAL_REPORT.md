# Evaluation Report

**Date:** 2026-04-02 14:30
**Evaluator:** Ralph v2 Adversarial Eval
**Strategy:** prompt

## Summary

pass_rate: 80%
features_total: 10
features_pass: 6
features_partial: 4
features_fail: 0

## Feature Scores

| # | Feature | Weight | Score | Summary |
|---|---------|--------|-------|---------|
| 1 | Data Access | 3 | Pass | Metadata listing, pagination, folder filter, read status all work correctly |
| 2 | Body Reading | 2 | Partial | Parsing & caching work but to/cc always empty on cached reads; SQL bug selects subject twice |
| 3 | Search | 2 | Pass | Metadata search works; Spotlight integration implemented; combined filters correct |
| 4 | Triage Labels | 3 | Pass | Assign, clear, filter by label, untriaged detection all work |
| 5 | Auto-Triage | 3 | Pass | Rule evaluation, VIP protection, idempotency, dry-run all work |
| 6 | Rules Engine | 2 | Pass | Config load/save, VIP check, first-match-wins, any_of all work |
| 7 | Mail Actions | 2 | Partial | Logic layer correct (VIP exclusion, confirmation flow), but actions depend on osascript — untestable in automated env |
| 8 | CLI Interface | 3 | Partial | All commands exist, JSON output correct, --yes/confirm flow works, but exit code is always 0 (errors print JSON to stdout with exit 0) |
| 9 | Overlay DB | 2 | Pass | Schema, migrations, identity mapping, persistence all work |
| 10 | Skill Wrapper | 1 | Partial | SKILL.md exists with triage workflow, but PATTERNS.md not created; skill cannot self-modify in practice |

## Detailed Findings

### Feature 1: Data Access
**Score:** Pass
**Evidence:**
- Tested: `test_list_emails_on_mock_db`, `test_list_emails_pagination`, `test_list_emails_folder_filter` — all pass
- Expected: Current emails returned with sender, subject, date, read status; pagination; folder filter
- Actual: All criteria met. Mock Envelope Index tests verify sort order (date desc), pagination (total_count vs page), folder filtering, read status
- Labels are joined from overlay DB at display time in `mea.rs:73-76`
- Default filters to INBOX when no folder specified (`data.rs:101`)
**Issues:** None

### Feature 2: Body Reading
**Score:** Partial
**Evidence:**
- Tested: `test_parse_plain_email`, `test_parse_html_email`, `test_parse_emlx`, `test_cache_and_retrieve_body`, `test_second_read_from_cache`, `test_body_persists_across_connections` — all pass
- Expected: Full body with headers (from, to, cc, date, subject); HTML→markdown; caching
- Actual: Body parsing and caching work. HTML conversion via html2text works. Caching is correct with upsert.
**Issues:**
1. **BUG (body.rs:185):** SQL query selects `COALESCE(subject, '')` twice — the third SELECT column should fetch `to` recipients, not `subject` again. The `_to_raw` variable is always `String::new()` (line 192). This means `to`/`cc` fields are **always empty** when reading from cache, since cached reads skip the .emlx file parsing path (lines 203-215 return immediately with `to: vec![], cc: vec![]`).
2. **BUG (body.rs:185):** Even for uncached reads, `to` and `cc` come from .emlx file parsing (lines 224-236) but the Envelope Index probably doesn't have a `to` column anyway — so the SQL workaround is intentional but the dead code/misleading variable name is confusing.
3. Spec requires `EmailDetail` to include `to: string[]` and `cc: string[]` — these are always empty on cached reads.

### Feature 3: Search
**Score:** Pass
**Evidence:**
- Tested: `test_search_by_sender`, `test_search_by_subject`, `test_search_by_date_range`, `test_search_combined_filters`, `test_search_no_results`, `test_search_results_shape_matches_list` — all pass
- Expected: Search by sender, subject, date range, body text; combined filters; same EmailSummary shape
- Actual: All metadata search criteria work. Spotlight body search implemented via `mdfind`. Combined search intersects results correctly. Date conversion (ISO→NSDate) is correct. Result shape matches data-access's EmailSummary.
**Issues:** None — body search via Spotlight can't be unit-tested but implementation looks correct.

### Feature 4: Triage Labels
**Score:** Pass
**Evidence:**
- Tested: `test_assign_and_get_label`, `test_assign_replaces_existing`, `test_clear_label`, `test_invalid_label`, `test_get_emails_by_label`, `test_get_untriaged`, `test_labels_persist` — all pass
- Expected: Assign 1-5, clear with 0, filter by label, detect untriaged, persist across restarts
- Actual: All criteria met. Labels stored with upsert. Clear via DELETE. Untriaged detection uses set-difference query. Persistence verified with tempfile test.
**Issues:** None

### Feature 5: Auto-Triage
**Score:** Pass
**Evidence:**
- Tested: `test_auto_triage_labels_receipts`, `test_auto_triage_trashes_matching`, `test_vip_auto_labeled_follow_up`, `test_vip_never_trashed`, `test_no_match_stays_untriaged`, `test_idempotent`, `test_triage_summary_counts` — all pass
- Expected: Receipts labeled 5; trash patterns trashed; VIP→Follow Up; VIP never trashed; no-match untriaged; idempotent; summary returned
- Actual: All 7 acceptance criteria met. Idempotency works via `get_label` check before processing. VIP priority enforced by `evaluate_rules` returning VIP before other rules. Summary counts verified.
**Issues:** None

### Feature 6: Rules Engine
**Score:** Pass
**Evidence:**
- Tested: `test_vip_sender_always_follow_up`, `test_vip_case_insensitive`, `test_receipt_rule`, `test_food_order_trash`, `test_any_of_matching`, `test_no_match`, `test_first_match_wins`, `test_vip_takes_priority`, `test_load_save_roundtrip`, `test_load_missing_file_returns_default`, `test_sender_exact_match` — all pass
- Expected: Config file readable; Claude can edit; VIP managed; match on sender/subject; first match wins; actions: label/trash/archive
- Actual: All criteria met. TOML config with serde. VIP case-insensitive. First-match-wins with VIP priority. `any_of` composite matching works. Missing file returns default config gracefully.
**Issues:** None

### Feature 7: Mail Actions
**Score:** Partial
**Evidence:**
- Tested: `test_bulk_action_vip_protection`, `test_vip_emails_excluded_from_bulk`, `test_bulk_action_no_vip` — all pass
- Expected: Archive, delete, flag, mark read, bulk actions, VIP protection, --yes confirmation
- Actual: Logic layer is fully correct — VIP exclusion from bulk actions works, confirmation flow implemented in `mea.rs`. AppleScript commands are well-structured.
**Issues:**
1. Actions use `message_id` to find emails in Mail.app (`whose message id is "{message_id}"`), but the message_id is interpolated directly into the AppleScript string without escaping. A message_id containing `"` characters would break the AppleScript or cause injection. (`actions.rs:46,60,75,91`)
2. `bulk_action` returns `email_ids: vec![]` in the response (line 157) — the ActionResponse never reports which IDs were actually acted upon, making it impossible for the caller to verify what happened.
3. Actual mail actions can't be tested without Mail.app — `test_bulk_action_no_vip` takes 120s because osascript hangs. This is documented but means mail action correctness is unverifiable.

### Feature 8: CLI Interface
**Score:** Partial
**Evidence:**
- Tested: `test_cli_parse_list`, `test_cli_parse_label`, `test_cli_parse_search`, `test_cli_parse_delete_with_yes`, `test_cli_parse_triage`, `test_success_response`, `test_error_response`, `test_confirm_response`, `test_all_output_valid_json` — all pass
- Expected: Binary named `mea`; all JSON output; structured errors; --yes confirmation; non-zero exit on error
- Actual: Binary is `mea`. All output is valid JSON. SuccessResponse, ErrorResponse, ConfirmationResponse all match spec data contracts. --yes flag works for delete/archive.
**Issues:**
1. **Exit codes always 0.** `mea.rs:8` calls `println!("{output}")` and `main()` returns `()`. Errors return JSON with `status: "error"` but exit code is always 0. Spec says "Exit code 0 for success, non-zero for errors" — this is not implemented. (`mea.rs:5-9`)
2. Spec says "No stderr for normal operation" — `triage.rs:94,102` writes warnings to stderr via `eprintln!`. While these are non-normal (AppleScript failures), it violates the clean stdout-only contract for the agent consumer.

### Feature 9: Overlay DB
**Score:** Pass
**Evidence:**
- Tested: `test_create_overlay_db`, `test_migration_idempotent`, `test_ensure_identity`, `test_ensure_identity_upsert`, `test_find_rowid_by_message_id`, `test_tables_exist`, `test_persistent_db` — all pass
- Expected: Auto-created on first run; labels persist; cached bodies persist; identity mapping; migrations
- Actual: All criteria met. Schema version tracking with migration system. WAL journal mode. Foreign keys enabled. Identity upsert works. Persistence verified.
**Issues:** None

### Feature 10: Skill Wrapper
**Score:** Partial
**Evidence:**
- Tested: Read SKILL.md — exists with correct command documentation and triage workflow
- Expected: SKILL.md exists; translates NL to CLI; interactive triage mode; PATTERNS.md; can add rules; AskUserQuestion; VIP protection
- Actual: SKILL.md exists with all commands documented, triage workflow described (dry-run → execute → manual review). Rules management documented with TOML example.
**Issues:**
1. **PATTERNS.md does not exist.** SKILL.md references `~/.mea/PATTERNS.md` for learned preferences but this file is never created. No mechanism to bootstrap it.
2. Skill describes interactive triage (group by sender, AskUserQuestion) but this behavior is entirely aspirational — no Claude Code skill integration exists to validate it actually works.
3. The skill says "self-improving: Claude can modify SKILL.md" — this is a spec aspiration, not a testable feature.

## Back-Pressure Results

| Check | Status | Details |
|-------|--------|---------|
| Build | PASS | Clean build, no warnings |
| Tests | PASS | 71/71 passing, 0 failing |
| Lint | PASS | 0 clippy warnings (with -D warnings) |
| Typecheck | PASS | (covered by build — Rust) |
| Format | PASS | 0 unformatted files |

## Code Quality Issues

### Critical
- **AppleScript injection** (`actions.rs:46,60,75,91`): message_id is interpolated directly into AppleScript strings without escaping quotes. A message_id containing `"` would break the script or allow injection.
- **Exit code always 0** (`mea.rs:5-9`): Error responses exit with code 0, making it impossible for the skill wrapper to detect failures via exit code. The agent must parse JSON to determine success/failure.

### Major
- **to/cc always empty on cached reads** (`body.rs:185,203-215`): SQL selects subject twice instead of to. Cached body reads return empty to/cc vectors. This silently degrades the EmailDetail contract.
- **ActionResponse.email_ids always empty** (`actions.rs:157`): Bulk action responses never report which IDs were acted upon, violating the principle of observable behavior.
- **stderr output from triage** (`triage.rs:94,102`): `eprintln!` calls leak warnings to stderr, potentially confusing the agent consumer.

### Minor
- **Duplicate code in cmd_delete and cmd_archive** (`mea.rs:172-261`): These two functions are nearly identical — VIP resolution + bulk_action call. Could be extracted.
- **Duplicate code in search.rs**: Body-only search path (lines 179-228) duplicates the same row-mapping closure from `search_metadata`.
- **No PATTERNS.md bootstrap**: Skill references a file that doesn't exist and has no creation mechanism.

## Recommendations

1. **Fix exit codes** — `main()` should check the JSON status and call `std::process::exit(1)` for errors. This is the most impactful fix for agent integration.
2. **Fix body.rs SQL bug** — Remove the dead `_to_raw` path or fetch actual `to` data if available in the Envelope Index schema.
3. **Escape AppleScript strings** — Sanitize `message_id` before interpolating into AppleScript to prevent injection.
4. **Populate ActionResponse.email_ids** — Return the actual IDs that were acted upon in bulk operations.
5. **Remove eprintln calls** — Either log to a file or include warnings in the JSON response structure.
6. **Create PATTERNS.md on first triage** — Bootstrap the file so the skill can start recording learned preferences.
