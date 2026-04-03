# Mail Actions

## Source
JTBD 5: Take Actions on Emails

## Topic Statement
The system performs actions on emails through Apple Mail so changes sync to the Exchange server without the user switching apps.

## Scope
**In-scope:** Archive, delete, flag/unflag, mark read/unread, bulk actions, confirmation for destructive operations
**Boundaries:** Reply and compose are out of scope (user handles in Mail.app). Triage labeling is owned by triage-labels. Auto-triage orchestration is owned by auto-triage.

## Behaviors (execution order)
1. On archive request: move email out of inbox in Apple Mail (remains searchable)
2. On delete request: move email to trash in Apple Mail
3. On flag request: toggle flag status on email in Apple Mail
4. On mark read/unread request: change read status in Apple Mail
5. On bulk action request: apply the action to multiple emails
6. On destructive action (delete, archive) without --yes flag: require confirmation before executing
7. On destructive action with --yes flag: execute without confirmation
8. After action: the change is reflected in Mail.app and syncs to the Exchange server

## Cross-Topic Shared Behavior
- VIP-protected emails (from auto-triage rules) are excluded from bulk delete/archive unless explicitly targeted by ID
- Actions use email IDs from data-access
- Auto-triage triggers actions through this spec

## Constraints
- Rust (edition 2024), shared library + CLI binary (`mea`)
- CLI is agent-facing: JSON output only, no interactive prompts
- All mail actions are performed through Apple Mail (AppleScript or equivalent) — never modify mail files directly
- Actions sync to the Exchange server via Apple Mail's native sync
- Destructive actions require confirmation by default (--yes to bypass)
- Single user, single account

## Acceptance Criteria
1. Archiving an email moves it out of inbox in Mail.app
2. Deleting an email moves it to trash in Mail.app
3. Flagging/unflagging toggles the flag in Mail.app
4. Marking read/unread changes status in Mail.app
5. Bulk actions work on multiple email IDs in a single call
6. Destructive actions without --yes flag return a confirmation prompt (not executed)
7. VIP emails are excluded from bulk destructive actions

## References
- Related: auto-triage (triggers actions via rules)
- Related: data-access (provides email IDs)
- Related: cli-interface (--yes flag and confirmation flow)
