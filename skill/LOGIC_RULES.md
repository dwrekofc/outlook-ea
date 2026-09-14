# Interpretation Rules

Use this as the main rulebook for interpreting email, tasks, projects, and context.

## Canonical Rule Sources

- Learned preferences: `${VAULT_NOTES}/utilities/context-profiles/mea/PATTERNS.md`
- Deterministic graph rules: the unified vault Turso database (`graph_*` and `mail_*`)
- Human-readable graph: `${VAULT_NOTES}/utilities/context-profiles/mea/MEA_GRAPH_CONTEXT.md`

Read `PATTERNS.md` before any email triage. Append learned behavior there; never delete prior entries.

## Triage Defaults

- Archive means durable reference, project correspondence, 1:1 known-person context, receipts/contracts, or user-marked Follow Up/Waiting/Read Later.
- Trash means newsletters, broadcasts, promos, cold outreach, recurring system noise, completed training nudges, and low-value event/webinar announcements.
- Capture means a concrete next action, deadline, owner decision, access request, approval, response needed, or project/task update.
- Keep in inbox only when the user must still act in Mail/Calendar or the correct action is unclear.

## Known-Person Bias

If the sender is a VIP, teammate, known colleague, or graph person and the message is not a broadcast/calendar/system notice, bias to archive or capture, not trash.

## Training Nudges

Cross-reference training emails against the `Overdue SAP Trainings` project. If a task exists and is done, trash. If pending, keep/capture priority. If missing, propose a task.

## Triage Process — 6-Phase Funnel

The full triage workflow, used whenever Derek asks for a real inbox clean-up (not a quick lookup). It's a funnel: strip out everything certain first so the slow interactive phase only touches genuinely ambiguous mail. This does not replace the Triage Defaults, decision tree, or any LOAD-BEARING rule in `PATTERNS.md` — every phase below defers to those for the actual keep/trash/archive call.

**Phase 0 — Sync & inventory.** Pull the inbox into a working list of (sender, subject, age, mea id, current label/status). Report the untriaged count.

**Phase 1 — Bulk auto-actions (no questions).** Act only on what already matches an established rule with 100% certainty — see the Triage Defaults, the PATTERNS.md decision tree (2026-04-29), and every dated LOAD-BEARING rule (training nudges, past-event/OOO trash, calendar-notice VIP exemption, age-based noise sweeps, domain blocklist, etc.). Always dry-run first: print per-bucket counts + sample subjects, sanity-check that VIPs/known people are excluded per the Known-Person Bias, then execute with `mea delete`/`mea archive --yes`. If Derek says "only do what you're 100% sure about," run this phase alone and stop.

**Phase 2 — Sub-categorize within each sender.** A sender is not one decision — split by subject/topic so the same sender can go different ways (e.g. one SAP IT for You email is noise, another is about Derek's specific hardware and must be kept; one Expedia-style thread is a refund status to keep, another is a "rate your trip" nudge to trash).

**Phase 3 — File the already-labeled piles out of the inbox.** Labeled/captured ≠ filed. Archive non-actionable-but-worth-keeping categories out of the inbox per the Triage Defaults' archive definition; keep anything user-marked Follow Up/Waiting/Read Later visible.

**Phase 4 — Open-items summary.** Where relevant, produce a summary table of open threads/orders/actions (e.g. pending training tasks from the training-nudge cross-reference, awaiting-reply threads via `mea list --needs-reply`, upcoming invites/OOO via `invite_status.py`).

**Phase 5 — Interactive walkthrough.** Everything still undecided goes one sender+subcategory per question, 4 questions per round, via `AskUserQuestion`. Every question MUST include a 1-2 sentence plain-language body summary per the 2026-05-05 body-summary rule — never subject-only. Recommended action goes first, followed by Archive / Trash / Leave / Other. Execute every few rounds via `mea delete`/`mea archive` to lock in progress (re-check the VIP/calendar-notice guard), then end with a labeled summary and totals.

**Invariants that always hold:** dry-run before any bulk write; VIPs/known people are never trashed by a rule, only on Derek's explicit per-item say-so — use `mea delete`/`mea archive --force` for this (2026-07-10) only after that explicit approval, never in a Phase 1 bulk pass (the calendar-notice exemption from 2026-06-15 is separate and automatic); one-time actions are never silently turned into durable rules — durable rules go through `mea graph add-rule` + `mea graph dump`, and Derek confirms first; recommend-don't-survey (first option is the pick); a running "inbox X → Y" count throughout; append any new pattern learned during the session to `PATTERNS.md`, never only to a skill file.

## Rule Changes

Add deterministic sender/subject rules through `mea graph add-rule`, then run `mea graph dump`.
Do not encode durable rules only inside a skill file.

Shared context uses one Turso graph, profiles `personal` | `mea`, through
`vault graph`, `mea graph`, or `pcg`. Use canonical IDs and the shared
`~/.config/vault/config.json`; vault-cli owns migrations.

`VAULT_NOTES` denotes `notes_path` from `VAULT_CONFIG` (default `~/.config/vault/config.json`), falling back to `~/vault`. Resolve it before running shell examples. MEA owns `MEA_GRAPH_CONTEXT.md`; the scheduled vault graph-dump script owns the separate `GRAPH_CONTEXT.md`.
