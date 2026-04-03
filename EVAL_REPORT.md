# Evaluation Report

**Date:** 2026-04-02 12:00
**Evaluator:** Ralph v2 Adversarial Eval
**Strategy:** prompt

## Summary

pass_rate: 95%
features_total: 10
features_pass: 9
features_partial: 1
features_fail: 0

## Feature Scores

| # | Feature | Weight | Score | Summary |
|---|---------|--------|-------|---------|
| 1 | CLI Interface | 1 | Pass | All commands output valid JSON, exit codes correct, --yes flag works |
| 2 | Data Access | 1 | Pass | Listings, folder filter, pagination, read status all correct |
| 3 | Body Reading | 1 | Pass | Body parsing, HTML→markdown, caching, headers all work |
| 4 | Search | 1 | Pass | Sender, subject, date range, body (Spotlight), combined filters work |
| 5 | Triage Labels | 1 | Pass | Assign 1-5, clear with 0, filter, untriaged, persistence all work |
| 6 | Mail Actions | 1 | Partial | VIP bulk action logic blocks ALL emails (including non-VIP) when VIPs present |
| 7 | Auto-Triage | 1 | Pass | Labels receipts, trashes matches, VIP protection, idempotent |
| 8 | Rules Engine | 1 | Pass | TOML config, match criteria, VIP list, first-match-wins, roundtrip |
| 9 | Overlay DB | 1 | Pass | Auto-create, migrations, identity mapping, persistence |
| 10 | Skill Wrapper | 1 | Pass | SKILL.md exists with full command reference and triage workflow |

## Detailed Findings

### Feature 1: CLI Interface
**Score:** Pass
**Evidence:**
- Tested: Parsed all CLI subcommands via clap unit tests (7 tests)
- Tested: All three response formatters (success, error, confirm) produce valid JSON
- Tested: Integration test confirms exit code 0 on success, non-zero on error
- Tested: Integration test confirms no stderr output on errors
- Expected: Binary named `mea`, JSON output, structured errors, --yes confirmation
- Actual: All criteria met
**Issues:** None

### Feature 2: Data Access
**Score:** Pass
**Evidence:**
- Tested: `list_emails` on mock DB returns correct emails sorted by date descending
- Tested: Pagination works (page 0 = first N, page 1 = remainder)
- Tested: Folder filter narrows to matching mailbox URL
- Tested: Read/unread status accurately reflected from `read` column
- Tested: Labels joined from overlay DB at display time via `list_emails_filtered`
- Tested: Label/untriaged filters applied BEFORE pagination (regression test included)
- Expected: All acceptance criteria from data-access spec
- Actual: All criteria met
**Issues:** None

### Feature 3: Body Reading
**Score:** Pass
**Evidence:**
- Tested: Plain text email parsing returns format "plain" with correct body
- Tested: HTML email parsing converts to readable text with format "markdown"
- Tested: `.emlx` file format parsing (byte count + RFC 2822 message)
- Tested: Cache stores and retrieves body with to/cc fields
- Tested: Cache upsert replaces existing cached body
- Tested: Second read returns instantly from cache
- Tested: Body persists across DB connections
- Expected: All acceptance criteria from body-reading spec
- Actual: All criteria met
**Issues:** None

### Feature 4: Search
**Score:** Pass
**Evidence:**
- Tested: Search by sender returns matching emails (LIKE pattern)
- Tested: Search by subject returns matching emails
- Tested: Search by date range using ISO 8601 → NSDate conversion
- Tested: Combined sender + subject filters narrow results correctly
- Tested: No results for non-matching queries returns empty array
- Tested: Results have same EmailSummary shape as list output
- Tested: `rowid_from_emlx_path` extracts IDs including partial files
- Tested: Body text search uses `mdfind` (Spotlight) and intersects with metadata
- Expected: All acceptance criteria from search spec
- Actual: All criteria met
**Issues:** None

### Feature 5: Triage Labels
**Score:** Pass
**Evidence:**
- Tested: Assign label 1-5 stores in overlay DB with correct label_number
- Tested: Assigning new label replaces existing (upsert behavior)
- Tested: Label 0 clears the label (DELETE from labels table)
- Tested: Invalid label (6+) returns error
- Tested: `get_all_labels` returns map for batch joining
- Tested: Labels persist across DB connections (tempfile test)
- Tested: Label filter works with pagination (regression test)
- Tested: Untriaged filter with pagination returns correct counts
- Expected: All acceptance criteria from triage-labels spec
- Actual: All criteria met
**Issues:** None

### Feature 6: Mail Actions
**Score:** Partial
**Evidence:**
- Tested: Delete/archive/flag/mark-read commands exist with correct AppleScript generation
- Tested: `--yes` flag bypasses confirmation; without it, returns confirmation JSON
- Tested: AppleScript string escaping handles quotes and backslashes
- Tested: VIP bulk action test confirms VIP exclusion response
- **BUG FOUND:** `bulk_action()` at `src/actions.rs:124` — when VIP emails are present in the ID list and `force=false`, the function returns early with `success: false` and `message_ids_acted: []`, meaning **zero emails are processed** including non-VIP ones. Both `cmd_delete` and `cmd_archive` in `src/bin/mea.rs:222,267` always pass `force=false`.
- Expected per spec: "VIP-protected emails are excluded from bulk delete/archive unless explicitly targeted by ID" — VIPs should be silently skipped, non-VIP emails should still be processed.
- Actual: If ANY VIP email is in a bulk delete/archive, ALL emails (including non-VIP) are blocked. The `force` parameter exists but is never set to `true` by any CLI command, making it unreachable. Even explicitly targeting a single VIP email by ID fails.
- Impact: Any bulk delete/archive containing even one VIP email does nothing.
**Issues:**
1. `bulk_action` should skip VIP emails and process non-VIP ones (not block everything)
2. The `force` parameter is dead code — never set to `true` by any caller
3. Explicitly targeting a VIP email by ID should work per spec ("unless explicitly targeted by ID") but doesn't

### Feature 7: Auto-Triage
**Score:** Pass
**Evidence:**
- Tested: Receipts matching receipt pattern get label 5
- Tested: Doordash emails matching trash rule are trashed (dry_run verified)
- Tested: VIP sender emails auto-labeled Follow Up (label 1)
- Tested: VIP emails never trashed even when matching trash rules
- Tested: Unmatched emails increment untriaged counter
- Tested: Summary includes labeled/trashed/archived/untriaged/total counts
- Tested: Idempotent — second run skips already-labeled emails
- Tested: Combined scenario (4 emails: receipt, doordash, friend, VIP) produces correct counts
- Expected: All acceptance criteria from auto-triage spec
- Actual: All criteria met. Note: auto-triage correctly bypasses the bulk_action issue by calling `delete_email`/`archive_email` individually per email with its own VIP check.
**Issues:** None

### Feature 8: Rules Engine
**Score:** Pass
**Evidence:**
- Tested: TOML config load/save roundtrip preserves all rules and VIP senders
- Tested: Missing config file returns empty defaults (no error)
- Tested: VIP sender match is case-insensitive
- Tested: VIP takes priority over all other rules
- Tested: First match wins for ordered rules
- Tested: sender_contains, sender_exact, subject_contains, and any_of criteria all work
- Tested: Rule actions: label (with label_number), trash, archive
- Expected: All acceptance criteria from rules-engine spec
- Actual: All criteria met
**Issues:** None

### Feature 9: Overlay DB
**Score:** Pass
**Evidence:**
- Tested: DB auto-created with in-memory and file-based connections
- Tested: Schema version tracked; migrations run to version 2
- Tested: Migration is idempotent (running twice doesn't error)
- Tested: email_identity table stores rowid + message_id mapping
- Tested: Identity upsert updates message_id for existing rowid
- Tested: labels and cached_bodies tables exist with correct constraints
- Tested: Migration v2 adds cached_to and cached_cc columns
- Tested: Data persists across connection close/reopen (tempfile tests)
- Expected: All acceptance criteria from overlay-db spec
- Actual: All criteria met
**Issues:** None

### Feature 10: Skill Wrapper
**Score:** Pass
**Evidence:**
- Tested: SKILL.md exists at project root with comprehensive skill definition
- Tested: Documents all `mea` CLI commands with correct syntax
- Tested: Triage workflow described (dry-run → approve → execute → manual review)
- Tested: References PATTERNS.md for learned preferences
- Tested: Documents rules.toml editing with TOML example
- Tested: Notes VIP protection, --yes requirement, JSON output parsing
- Tested: Integration test confirms PATTERNS.md bootstrapping on first run
- Expected: All acceptance criteria from skill-wrapper spec
- Actual: All criteria met
**Issues:** None

## Back-Pressure Results

| Check | Status | Details |
|-------|--------|---------|
| Build | PASS | Compiles cleanly with no warnings |
| Tests | PASS | 81/81 passing (77 unit + 4 integration) |
| Lint | PASS | clippy clean with `-D warnings` |
| Typecheck | PASS | (covered by build — Rust compiler) |
| Format | PASS | `cargo fmt --check` reports no issues |

## Code Quality Issues

### Critical
- None

### Major
- **VIP bulk action blocks all emails** (`src/actions.rs:124`): When any VIP email is in a bulk delete/archive list, `bulk_action` returns early without processing ANY email (including non-VIP ones). The `force` parameter that would bypass this is never set to `true` by any CLI command. Fix: skip VIP emails silently and process the rest, or expose `--force` in the CLI.

### Minor
- **Dead code — `db::find_rowid_by_message_id`** (`src/db.rs:99`): Public function only used in its own test. Intended for index rebuild recovery but not called by any production code path.
- **Dead code — `labels::get_emails_by_label`** (`src/labels.rs:88`): Public function only used in its own test. No production caller.
- **Dead code — `labels::get_untriaged`** (`src/labels.rs:105`): Public function only used in its own test. The untriaged logic in `list_emails_filtered` uses `get_all_labels` + retain instead.
- **Dead code — `rules::save_rules`** (`src/rules.rs:85`): Only used in test. The skill wrapper edits rules.toml directly, but the function exists for potential programmatic use.
- **Spotlight query injection** (`src/search.rs:128`): Body text containing single quotes (`'`) in the mdfind query `kMDItemTextContent == '*{body_text}*'cd` could break the Spotlight query. Not a security risk (mdfind doesn't execute commands) but causes search failures for queries with apostrophes.
- **Hardcoded triage limit** (`src/bin/mea.rs:338`): `cmd_triage` fetches up to 10000 emails. Users with >10000 inbox emails would have some emails missed by auto-triage.

## Recommendations

1. **Fix VIP bulk action logic** — `bulk_action` should silently skip VIP emails and process non-VIP ones instead of blocking the entire operation. Consider also exposing a `--force` flag to override VIP protection when explicitly desired.
2. **Escape body text for mdfind** — sanitize single quotes in Spotlight queries to prevent search failures.
3. **Remove or annotate dead code** — `find_rowid_by_message_id`, `get_emails_by_label`, `get_untriaged` are unused in production. Either remove them or add `#[allow(dead_code)]` with a comment explaining future intent.
4. **Make triage page size configurable** — replace the hardcoded 10000 limit with a configurable value or loop until all emails are processed.
