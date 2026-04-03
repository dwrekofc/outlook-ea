# Progress

## Completed
- [x] Project restructure: removed GPUI, set up lib + mea binary with all dependencies
- [x] overlay-db: SQLite with migrations (v1+v2), email identity mapping (rowid + message_id)
- [x] data-access: Read Apple Mail Envelope Index, list emails with pagination/filtering
- [x] triage-labels: Assign/clear/filter labels 1-5, untriaged detection, batch label map
- [x] body-reading: Parse .emlx files, HTML→markdown conversion, body caching with to/cc in overlay DB
- [x] search: Metadata search via SQL, body search via Spotlight (mdfind), combined search
- [x] rules-engine: TOML config file, pattern matching (sender/subject/any_of), VIP senders
- [x] mail-actions: AppleScript actions with injection-safe escaping, bulk with VIP protection, message_ids_acted tracking
- [x] auto-triage: Rule evaluation, VIP auto-label Follow Up, idempotent operation, structured warnings
- [x] cli-interface: mea binary with all commands, JSON output, --yes confirmation flow, non-zero exit on error
- [x] skill-wrapper: SKILL.md with triage workflow, rules management, PATTERNS.md bootstrapped on first run

## Eval Fixes (pass_rate 80% → targeting 100%)
- [x] body.rs: Fixed SQL bug (subject selected twice instead of to); added migration v2 for cached_to/cached_cc columns; cached reads now return to/cc
- [x] actions.rs: Added escape_applescript() to prevent injection; ActionResponse now tracks message_ids_acted
- [x] mea.rs: Exit code 1 on error status; PATTERNS.md bootstrapped in open_overlay()
- [x] triage.rs: Replaced eprintln with warnings vec in TriageSummary JSON

## Remaining
- (none)

## Back-Pressure Status
- Build: PASS
- Tests: 78/78 passing (74 unit + 4 integration)
- Clippy: PASS (zero warnings with -D warnings)
- Format: PASS
