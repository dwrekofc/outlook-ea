# Evaluation Report

**Date:** 2026-04-02 12:00
**Evaluator:** Ralph v2 Adversarial Eval
**Strategy:** prompt

## Summary

pass_rate: 88%
features_total: 8
features_pass: 7
features_partial: 1
features_fail: 0

## Feature Scores

| # | Feature | Weight | Score | Summary |
|---|---------|--------|-------|---------|
| 1 | Data Access | 3 | Partial | Core listing works but label/untriaged filtering applied post-pagination — returns wrong results |
| 2 | Body Reading | 3 | Pass | Cache, emlx parsing, HTML→markdown, to/cc caching all working |
| 3 | Search | 3 | Pass | Metadata search by sender/subject/date works; Spotlight body search implemented |
| 4 | Triage Labels | 3 | Pass | Assign 1-5, clear with 0, filter, untriaged detection, persistence all correct |
| 5 | Mail Actions | 2 | Pass | VIP protection, confirmation flow, AppleScript escaping all correct; inherently untestable without Mail.app |
| 6 | Auto-Triage | 3 | Pass | Rule evaluation, VIP priority, idempotency, dry-run, summary all verified |
| 7 | Rules Engine | 3 | Pass | TOML config, VIP, first-match-wins, any_of, sender_exact/contains all working |
| 8 | CLI Interface | 3 | Pass | JSON output, exit codes (non-zero on error), --yes confirmation, no stderr all correct |

## Detailed Findings

### Feature 1: Data Access
**Score:** Partial
**Evidence:**
- Tested: `test_list_emails_on_mock_db`, `test_list_emails_pagination`, `test_list_emails_folder_filter` — all pass
- Expected: Listings filtered by label or untriaged status return correct paginated results
- Actual: Core listing (no filters) works perfectly. Pagination, folder filter, date sort, read status all correct.
- **Bug:** Label and untriaged filters are applied AFTER pagination from the Envelope Index (mea.rs:90-114). Flow: fetch page N of size S from DB → join labels → then filter. This means `mea list --label 1 --page 0 --page-size 20` fetches the first 20 inbox emails and then retains only those with label 1. If none of the first 20 have label 1, the result is empty even though labeled emails exist later. Same issue with `--untriaged`. The filter should be applied before or during the SQL query, not after pagination.
**Issues:**
- **Major:** Post-pagination filtering (mea.rs:104-114) produces incorrect results for `--label` and `--untriaged` flags

### Feature 2: Body Reading
**Score:** Pass
**Evidence:**
- Tested: All 8 body tests pass (cache roundtrip, emlx parsing, plain/HTML, upsert, persistence)
- Expected: Body with headers, HTML→markdown, caching
- Actual: Body parsing works. HTML converted via html2text. Cache stores to/cc from .emlx headers (migration v2). Cached reads return to/cc correctly.
**Issues:** None

### Feature 3: Search
**Score:** Pass
**Evidence:**
- Tested: All 7 search tests pass (sender, subject, date range, combined, no results, shape match)
- Expected: Metadata search via SQL, body search via Spotlight, combined intersection
- Actual: SQL LIKE queries work. Date iso8601↔nsdate conversion correct. Spotlight integration via mdfind coded with proper intersection logic.
**Issues:** None

### Feature 4: Triage Labels
**Score:** Pass
**Evidence:**
- Tested: All 9 label tests pass (assign, replace, clear, invalid, by-label, untriaged, persistence, get_all)
- Expected: Assign 1-5, clear with 0, filter, persist
- Actual: All criteria met. CHECK constraint on label_number. Upsert on re-assign. DELETE on clear.
**Issues:** None

### Feature 5: Mail Actions
**Score:** Pass
**Evidence:**
- Tested: `test_bulk_action_vip_protection`, `test_vip_emails_excluded_from_bulk`, `test_escape_applescript` — all pass
- Expected: VIP excluded from bulk actions, AppleScript escaping, confirmation flow
- Actual: VIP protection works. `escape_applescript` (actions.rs:45-47) properly escapes backslashes and double quotes. Confirmation flow returns ConfirmationResponse without --yes. Auto-triage uses direct calls (not bulk_action) with its own VIP check.
- **Design note:** `bulk_action` returns early with warning when VIPs are in batch (actions.rs:124-142) rather than executing non-VIP emails. This is a conservative safety design — the spec says "excluded from bulk actions" which could mean either approach. The auto-triage path bypasses this entirely.
**Issues:** None functional

### Feature 6: Auto-Triage
**Score:** Pass
**Evidence:**
- Tested: All 7 triage tests pass (receipts labeled, trash match, VIP follow-up, VIP never trashed, no-match untriaged, idempotent, counts)
- Expected: Rule evaluation, VIP priority, idempotency, dry-run, summary
- Actual: All criteria met. Idempotency via get_label check. VIP always wins. Dry-run skips AppleScript. Warnings stored in summary.warnings (not stderr).
**Issues:** None

### Feature 7: Rules Engine
**Score:** Pass
**Evidence:**
- Tested: All 12 rules tests pass (VIP always/case-insensitive, receipt, food-trash, any_of, no-match, first-wins, VIP priority, roundtrip, missing-file, sender_exact)
- Expected: TOML config, VIP management, sender/subject matching, first-match-wins
- Actual: All criteria met. TOML serde works. `any_of` composite matching. `sender_exact` case-insensitive. Default empty config on missing file.
**Issues:** None

### Feature 8: CLI Interface
**Score:** Pass
**Evidence:**
- Tested: All 11 CLI tests + 4 integration tests pass
- Expected: `mea` binary, JSON output, structured errors, exit codes, --yes, no stderr
- Actual: Binary named `mea`. All output valid JSON. Exit code 1 on error (mea.rs:10-16, verified by `test_exit_code_nonzero_on_error`). No stderr (verified by `test_no_stderr_on_error`). Confirmation flow for destructive ops without --yes.
**Issues:** None

## Back-Pressure Results

| Check | Status | Details |
|-------|--------|---------|
| Build | PASS | Clean build, no warnings |
| Tests | PASS | 78/78 passing (74 unit + 4 integration), 0 failing |
| Lint | PASS | 0 clippy warnings (with -D warnings) |
| Typecheck | PASS | Covered by Rust compiler build |
| Format | PASS | 0 unformatted files |

## Code Quality Issues

### Critical
- None

### Major
- **Post-pagination label/untriaged filtering** (mea.rs:104-114): `--label` and `--untriaged` filters applied after SQL pagination. Returns wrong subsets — filtered results depend on which emails happen to be in the current page, not on which emails match the filter. Should push filtering into the SQL query or fetch all results before filtering.

### Minor
- **cli::error fallback not JSON-safe** (cli.rs:136): Fallback format string `format!(r#"..."error":"{message}"..."#)` doesn't escape quotes. Only reachable if serde_json serialization fails (near-impossible), but technically incorrect.
- **AppleScript targets inbox only** (actions.rs:53,68,84,100): All AppleScript commands use `every message of inbox`. Flag/mark-read won't work on emails in other mailboxes.
- **Unused public functions**: `find_rowid_by_message_id` (db.rs:99), `get_emails_by_label` (labels.rs:88), `get_untriaged` (labels.rs:105) — tested but never called from CLI or other modules. The triage command reimplements untriaged logic inline (mea.rs:357-362) instead of using `get_untriaged`.
- **bulk_action blocks all on VIP presence** (actions.rs:124-142): When any VIP is in batch and force=false, returns early without actioning non-VIP emails. `force` parameter is never set to true from CLI. Functionally this means the user must manually remove VIP IDs from their batch.

## Recommendations

1. **Fix label/untriaged filtering**: Either push the filter into the SQL query (JOIN with overlay labels table) or fetch all results and filter before pagination. This is the only functional bug affecting correctness.
2. **Wire up `get_untriaged`**: The cmd_triage function reimplements this inline — use the existing helper from labels.rs instead.
3. **Consider broadening AppleScript scope**: Search all mailboxes, not just inbox, for flag/mark-read actions.
4. **Clean up unused functions**: Either call `find_rowid_by_message_id` for index rebuild recovery or remove it.
