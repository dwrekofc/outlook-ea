# Progress

## Completed
- [x] Project restructure: removed GPUI, set up lib + mea binary with all dependencies
- [x] overlay-db: SQLite with migrations, email identity mapping (rowid + message_id)
- [x] data-access: Read Apple Mail Envelope Index, list emails with pagination/filtering
- [x] triage-labels: Assign/clear/filter labels 1-5, untriaged detection, batch label map
- [x] body-reading: Parse .emlx files, HTML→markdown conversion, body caching in overlay DB
- [x] search: Metadata search via SQL, body search via Spotlight (mdfind), combined search
- [x] rules-engine: TOML config file, pattern matching (sender/subject/any_of), VIP senders
- [x] mail-actions: AppleScript actions (delete, archive, flag, mark read), bulk with VIP protection
- [x] auto-triage: Rule evaluation, VIP auto-label Follow Up, idempotent operation
- [x] cli-interface: mea binary with all commands, JSON output, --yes confirmation flow
- [x] skill-wrapper: SKILL.md with triage workflow, rules management, learned preferences

## Remaining
- (none)

## Back-Pressure Status
- Build: PASS
- Tests: 71/71 passing
- Clippy: PASS (zero warnings with -D warnings)
- Format: PASS
