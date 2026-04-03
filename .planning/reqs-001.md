---
session: "001"
summary: "Local email operating system for Apple Mail — Rust CLI with Claude Code skill for triage, search, and email management"
decisions_archive: ".planning/archive/decisions-001.md"
roadmap_file: ".planning/roadmap-001.md"
created: "2026-03-23"
last_updated: "2026-03-23"
---

# outlook-ea Requirements

## Project Overview

A local email operating system where Apple Mail handles server sync, but all triage, search, categorization, and workflow management happen through custom tooling. The user manages their work email (SAP Exchange via Apple Mail) through a Rust CLI that Claude Code invokes via a skill wrapper.

**Why:** The user's inbox is high-volume and diverse (work communications, receipts, notifications, meetings, vendor emails). Reading every email in Mail.app is slow and context-switching-heavy. This tool lets Claude Code act as an email assistant — surfacing what matters, auto-triaging the obvious, and letting the user make fast decisions on the rest.

**Who:** Single user (the developer). No multi-user or multi-tenant requirements. One Apple Mail account.

**Lineage:** Rebuilt from scratch based on lessons from two prior systems:
- **swiftea** — Swift CLI that mirrored Apple Mail into libSQL. Worked but was painful to develop/test in Swift. Over-engineered with vaults, threading, and a full mirror DB.
- **Gmail skill** — Claude Code skill with Python scripts for Gmail triage. Mature triage workflow with auto-rules, VIP protection, interactive triage mode, and learned preferences. This is the gold standard for the triage experience.

---

## JTBD 1: See What's In My Inbox

**When** I want to check my email, **I want** to see a list of messages with sender, subject, date, and triage status, **so that** I can quickly scan what needs attention without opening Mail.app.

### User Stories
- As a user, I want to list my inbox emails sorted by date, so I can see what's new.
- As a user, I want to filter by triage label (e.g., "show me only Follow Ups"), so I can focus on one category at a time.
- As a user, I want to see which emails are untriaged (no label assigned), so I know what still needs my attention.
- As a user, I want to see unread vs read status, so I know what I haven't looked at.
- As a user, I want email listings to be always current (not stale cached data), so I trust what I see.

### Notes
- Email metadata (sender, subject, date, read status, folder) comes directly from Apple Mail's data store — always fresh, no sync required.
- Triage labels and other user metadata come from the overlay database.
- Listing should support pagination for large inboxes.

---

## JTBD 2: Read an Email's Content

**When** I want to understand what an email says, **I want** to read its body content in a clean readable format, **so that** I can make decisions about it without opening Mail.app.

### User Stories
- As a user, I want to view an email's full body text by its ID, so I can read it.
- As a user, I want HTML emails converted to readable plain text/markdown, so I'm not looking at raw HTML.
- As a user, I want the body cached after first read, so subsequent reads are instant.
- As a user, I want to see email headers (from, to, cc, date, subject) alongside the body.

### Notes
- Body content is parsed from Apple Mail's on-disk email files on first access, then cached in the overlay database.
- Plain text is preferred when available; HTML is converted to markdown as fallback.

---

## JTBD 3: Search My Email

**When** I'm looking for a specific email or set of emails, **I want** to search across my inbox by keyword, sender, subject, or date range, **so that** I can find what I need quickly.

### User Stories
- As a user, I want to search by sender address or name, so I can find emails from a specific person.
- As a user, I want to search by subject keywords, so I can find emails about a topic.
- As a user, I want to search by date range, so I can narrow results to a time period.
- As a user, I want full-text body search to work broadly (not just emails I've opened), so I can find content buried in emails I haven't read yet.
- As a user, I want search results ranked by relevance, so the best matches appear first.

### Notes
- Metadata search (sender, subject, date) queries Apple Mail's Envelope Index directly via SQL.
- Full-text body search uses macOS Spotlight (`mdfind`) which indexes all email content natively — no custom FTS index needed.
- Bodies are still cached in the overlay DB on first read for display purposes, but search does not depend on the cache.

---

## JTBD 4: Triage My Inbox Efficiently

**When** I have a batch of untriaged emails, **I want** to quickly categorize, archive, or trash them in bulk, **so that** my inbox stays manageable and I focus on what matters.

### User Stories
- As a user, I want to assign numbered triage labels (1-5) to emails, so they're categorized for my workflow.
- As a user, I want obvious categories auto-triaged (receipts auto-labeled, known noise auto-trashed), so I only manually triage the ambiguous emails.
- As a user, I want VIP senders auto-labeled as "Follow Up" and protected from bulk actions, so important emails are never accidentally trashed or archived.
- As a user, I want an interactive triage mode where I'm presented groups of emails (by sender) and choose what to do with each group, so I can triage dozens of emails in minutes.
- As a user, I want the system to learn my preferences over time (what I trash, what I label, what I archive), so auto-triage gets smarter.
- As a user, I want to clear all labels from an email (reset triage), so I can re-triage if I change my mind.

### Triage Labels
| # | Label | When to use |
|---|-------|-------------|
| 1 | Follow Up | Requires my attention or action |
| 2 | Waiting | Waiting on someone else |
| 3 | Reference | Important reference material (use sparingly) |
| 4 | Read Later | Interesting but not actionable |
| 5 | Receipts | Receipts, invoices, purchases |

- Labels 1-4 are manually assigned (user decides, tool can suggest).
- Label 5 (Receipts) can be auto-applied by rules.
- `0` clears all labels from an email.

---

## JTBD 5: Take Actions on Emails

**When** I've decided what to do with an email, **I want** to archive, delete, flag, or mark it read/unread, **so that** the action reflects in Apple Mail without me switching apps.

### User Stories
- As a user, I want to archive an email (move out of inbox), so it's out of my way but still searchable.
- As a user, I want to delete an email (move to trash), so it's gone.
- As a user, I want to flag/unflag an email, so I can mark it for attention in Mail.app too.
- As a user, I want to mark an email as read or unread, so its status reflects my intent.
- As a user, I want to perform these actions in bulk (multiple emails at once), so triage is fast.
- As a user, I want destructive actions (delete, archive) to require confirmation (or a --yes flag), so I don't accidentally lose emails.

### Notes
- Actions are performed through Apple Mail — when we archive an email, it actually moves in Mail.app and syncs to the server.
- Reply and compose are explicitly out of scope — the user handles those in Mail.app.

---

## JTBD 6: Define and Manage Rules

**When** I see patterns in my email (certain senders, subjects, or types that always get the same treatment), **I want** to define rules that auto-triage future emails matching those patterns, **so that** less email needs manual attention over time.

### User Stories
- As a user, I want to define rules like "emails from X sender → label as Receipts + archive", so common patterns are handled automatically.
- As a user, I want to define auto-trash rules (e.g., "food order confirmations → trash"), so noise is removed without my input.
- As a user, I want to maintain a VIP sender list where emails are always labeled "Follow Up" and never bulk-actioned.
- As a user, I want Claude to be able to add/modify rules by editing a config file, so the system evolves without recompiling.
- As a user, I want to review what rules exist and what they do.

### Notes
- Rules are evaluated at the tool layer, not in Apple Mail. This gives full control over triage logic without depending on Apple Mail's limited rule system.
- Rules config is a file that Claude (via the skill wrapper) can read and edit.

---

## Constraints & Principles

### Hard Tech Requirements
- **Language:** Rust (edition 2024)
- **Architecture:** Shared library (core) + CLI binary. The library will be reused by a future GPUI desktop app.
- **Data strategy:** Read Apple Mail's Envelope Index directly for metadata (always fresh). Maintain a separate overlay database for user metadata (labels, cached bodies, rules state, etc.). No full data mirror — no daemon required for data freshness.
- **Email IDs:** Apple Mail rowid as primary reference, RFC822 Message-ID stored as backup for resilience if Mail rebuilds its index.
- **Mail actions:** Performed through Apple Mail (AppleScript or equivalent) so they sync to the Exchange server.
- **Claude Code integration:** A Claude Code skill (SKILL.md) wraps the CLI binary. The skill holds triage logic, learned preferences, and interactive workflows. The binary handles data access and actions.
- **CLI binary name:** `mea` (mail executive assistant)
- **Agent-facing CLI:** The CLI is designed for AI agent consumption, not human interactive use. No REPL, no interactive prompts, no TUI. All output is machine-parseable (JSON). The skill layer handles all human-facing interaction (AskUserQuestion, summaries, etc.).
- **Self-improving skill:** The skill wrapper (SKILL.md + PATTERNS.md) is self-improving — Claude can modify it as it learns triage patterns, same as the Gmail skill.
- **Rules engine:** Layered — the binary reads a config file for deterministic rules (pattern matching, sender lists). The skill wrapper handles higher-level judgment (suggesting labels, learning from user decisions).

### Design Principles
- **Apple Mail is the sync engine.** It handles Exchange/IMAP connectivity. We never talk to the mail server directly.
- **Read-only against Apple data.** We never write to Apple Mail's databases or files. All writes go through Apple Mail's scripting interface.
- **Always fresh for listings.** Inbox listings query Apple Mail's live data, not a potentially stale cache.
- **Lazy body loading.** Email bodies are parsed on first read and cached. This avoids the massive upfront sync cost.
- **Local-first.** Everything runs on the Mac. No cloud services required for core functionality. (AI screening may be added later as an optional feature.)
- **Single user, single account.** No vault abstraction, no multi-account routing. Simple global config.

### What's Explicitly Out of Scope (v1)
- **Email threading / conversation grouping** — deferred to future phase
- **AI screening / auto-categorization via LLM** — deferred to future phase
- **Export to markdown or JSON files** — deferred to future phase
- **Reply and compose** — user handles these in Mail.app
- **Calendar, contacts, reminders** — deferred to future phase
- **Desktop GUI (GPUI app)** — future phase; the shared library is designed to support it
- **Multiple accounts or vault system** — dropped; single account only

---

## Resolved Questions

- **CLI binary name:** `mea` (mail executive assistant)
- **Self-improving skill:** Yes — Claude can modify SKILL.md, add PATTERNS.md entries, and evolve triage logic over time (same pattern as Gmail skill).
- **Interactive triage mode:** Lives entirely in the skill layer (AskUserQuestion loops in SKILL.md). The CLI has no interactive/REPL modes.
- **CLI is agent-facing:** The CLI is designed to be called by an AI agent (Claude Code), not used interactively by a human. No REPL loops, no interactive prompts, no TUI. All output should be machine-parseable (JSON). The skill layer handles all human interaction.

---

_Future phases and deferred items are tracked in `.planning/roadmap-001.md`. These are NOT requirements — they are placeholders for future exploration._

_Decisions log archived at `.planning/archive/decisions-001.md` for provenance. This requirements document is the authoritative source of truth for all downstream phases._
