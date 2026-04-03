---
session: "001"
summary: "Rebuild swiftea email tooling in Rust — tech stack, feature selection, architecture"
reqs_file: ".planning/reqs-001.md"
created: "2026-03-23"
last_updated: "2026-03-23"
---

# Decisions Log — Session 001

## Context

Rebuilding the swiftea (Swift Executive Assistant) email tooling in a new technology. The existing Swift project has:
- Mail sync from Apple Mail (Envelope Index SQLite + EMLX parsing) → local libSQL mirror
- FTS5 full-text search
- Stable email IDs (deterministic hash)
- Email threading (Message-ID/References/In-Reply-To headers)
- Watch mode via LaunchAgent daemon
- AppleScript-based mail actions (archive, delete, move, flag, mark, reply, compose)
- AI screening via OpenRouter (categorizes emails)
- Triage labels (task, waiting, reference, read_later, expenses)
- Obsidian plugin UI (keyboard-first inbox, virtualized list, column sort/resize/reorder)
- Markdown + JSON export (Obsidian-compatible frontmatter)
- Vault system (account binding, config)

Calendar module was in draft/proposal stage (not implemented). User explicitly defers calendar to future phase.

## Core Vision

**Status:** decided
**Strength:** authoritative

User confirmed: this is a **local email operating system**. Apple Mail is the sync engine to Exchange/IMAP servers. The user's tooling owns triage, search, categorization, and workflow. The rebuild should carry this vision forward.

**Date:** 2026-03-23

## Feature-by-Feature Review

### Vault System
**Status:** decided
**Strength:** authoritative
**Decision:** Drop vaults. Single global config + single DB. No account binding abstraction.
**Rationale:** User has one account. Vault system was over-engineering for a single-user tool.
**Date:** 2026-03-23

### Email Threading
**Status:** deferred
**Strength:** strong
**Decision:** Defer to future phase. Nice-to-have, not essential for v1.
**Rationale:** Was 0/30 tasks in swiftea. User survived without it. Individual email workflow works fine.
**Date:** 2026-03-23

### Data Architecture: Direct Read + Overlay
**Status:** decided
**Strength:** authoritative

**Options Considered:**
- **Full Mirror (swiftea approach)** — Copy all data into own DB. Requires daemon, initial sync, can drift.
- **Direct Read + Overlay (chosen)** — Query Apple Mail's Envelope Index directly for metadata. Maintain thin overlay DB for user's own data.

**Decision:** Direct Read + Overlay with lazy body caching.
- **Listings** (inbox, search): Always query Envelope Index directly. Always fresh, no sync needed.
- **Body content**: Parse EMLX on-demand on first read, cache in overlay DB. Subsequent reads hit cache.
- **User metadata** (labels, categories, screening, export paths): Stored in overlay DB, keyed by email ID.
- **No daemon required** for data freshness. Watch mode may still be useful for triggering AI screening of new mail.

**Rationale:** Eliminates the entire sync subsystem (MailSync, MailSyncBackward, MailSyncParallel, incremental sync, bulk copy, daemon). Metadata is always fresh. Body parsing happens lazily. Dramatically simpler than swiftea's approach.
**Date:** 2026-03-23

### ID Strategy
**Status:** decided
**Strength:** strong

**Options Considered:**
- **Apple Mail rowid only** — Simplest but fragile if index rebuilds.
- **RFC822 Message-ID only** — Globally unique but not all emails have one.
- **Hash-based stable ID (swiftea)** — Most portable, most complex.
- **Rowid primary + Message-ID backup (chosen)** — Pragmatic. Fast lookups via rowid, Message-ID stored for recovery.

**Decision:** Use Apple Mail rowid as primary key for overlay references. Store RFC822 Message-ID as backup for recovery if Envelope Index is rebuilt. No hash-based ID system.
**Rationale:** Rowid is what the Envelope Index uses natively — zero translation cost for queries. Message-ID backup handles the rare index rebuild scenario without adding complexity to every operation.
**Date:** 2026-03-23

### Full-Text Search
**Status:** decided
**Strength:** authoritative
**Decision:** Metadata search (subject/from/to/date) queries Envelope Index directly via SQL. Full-text body search uses macOS Spotlight (`mdfind`) which indexes all email content natively. No custom FTS index needed — Spotlight covers all emails regardless of whether they've been opened in the tool.
**Rationale:** Spotlight already indexes all Apple Mail content. Eliminates need for FTS5, proactive EMLX parsing, or any body caching for search purposes. Bodies are still cached on first read for display.
**Date:** 2026-04-02 (updated from 2026-03-23)

### AI Screening
**Status:** deferred
**Strength:** strong
**Decision:** Defer to future phase. Get basics working first.
**Rationale:** AI categorization is valuable but adds complexity (OpenRouter integration, prompt templates, screening daemon). Not needed for v1 triage workflow.
**Date:** 2026-03-23

### Triage Labels
**Status:** decided
**Strength:** authoritative
**Decision:** Keep as-is. Five fixed labels: task, waiting, reference, read_later, expenses. Toggled with keyboard shortcuts 1-5, 0 to clear. Stored in overlay DB.
**Date:** 2026-03-23

### Mail Actions
**Status:** decided
**Strength:** authoritative
**Decision:** Archive, delete, flag, mark read/unread via AppleScript to Mail.app. Reply and compose out of scope — user handles those in Mail.app directly.
**Rationale:** User confirmed they don't need help with replying. These four actions cover the triage workflow.
**Date:** 2026-03-23

### Tech Stack
**Status:** decided
**Strength:** authoritative
**Decision:** Rust. Shared library architecture (core lib + CLI binary). Future GPUI app will use the same core lib.
**Key crates:**
- `osakit` — in-process OSAKit bindings for AppleScript/JXA (no shell-out overhead)
- `rusqlite` — read Apple Mail's Envelope Index + manage overlay DB
- `mail-parser` — parse EMLX/RFC2822 email content
**Rationale:** Fast CLI startup (0.5ms vs 100ms+ Python), low memory, shared lib for future GPUI app with zero FFI boundary. AppleScript IPC cost is the same regardless of language.
**Date:** 2026-03-23

### Export
**Status:** deferred
**Strength:** strong
**Decision:** Defer export (markdown + JSON file output) to future phase. CLI outputs to stdout. Overlay DB is the primary interface.
**Rationale:** Focus on CLI + overlay DB for v1. Export can be added later if needed for Obsidian or other tools.
**Date:** 2026-03-23

## Gmail Skill — Triage Workflow Reference

The user has a mature Gmail triage workflow (via `/gmail` skill) that should inform how this Apple Mail tool works. Key patterns to carry over:

### Label System (Gmail → Apple Mail rebuild)
- **Manual triage labels (1-4):** Follow Up, Waiting, Reference, Read Later — user assigns, never auto-applied
- **Auto-triage labels (5, 7):** Receipts (with sub-labels), Gary Expenses — filter-driven, auto-applied
- **VIP senders:** Auto-labeled "Follow Up", never bulk-actioned
- swiftea had 5 labels: task, waiting, reference, read_later, expenses — similar but not identical to Gmail

### Triage Workflow
1. Sync/refresh email list
2. Auto-triage: apply labels to obvious categories (receipts, known senders)
3. Present manual-triage candidates grouped by suggested label
4. User confirms → bulk actions applied
5. Optional interactive triage mode: 4 sender groups at a time via AskUserQuestion

### Learned Preferences (from Gmail SKILL.md)
- Auto-trash: food order confirmations, subscription updates, OTPs, marketing, old meeting invites
- Auto-label: card expiring → Follow Up, receipts → categorized sub-label + archive
- Never trash VIP senders
- Never use "Reference" liberally — sparingly for truly important docs
- Key people with special handling (financial advisor, partner, family)

### Architecture Patterns (from Gmail scripts)
- Local SQLite DB as working store
- Pre-built SQL views for common queries (v_inbox, v_unread, etc.)
- Body parsing: HTML → markdown for readability
- Bulk actions with safety confirmations (>100 emails, deletes, filter changes)
- Self-improving skill: Claude can modify scripts as patterns emerge

### Hard Requirements vs Implementation Details
**Status:** decided
**Strength:** authoritative

User clarified: reqs should be product-focused (what and why), not micro-technical (how). Exceptions are explicit tech choices the user has locked in.

**Hard requirements (product-level or explicit tech choices):**
- Rust language, CLI binary, shared library for future GPUI app
- Direct-read Apple Mail Envelope Index + overlay DB (not a full mirror)
- Rowid primary + Message-ID backup for email references
- Gmail-style numbered triage labels (1-5)
- Full triage workflow (auto-trash, VIP, interactive mode, learned prefs)
- Mail actions via Apple Mail (archive, delete, flag, mark read/unread)
- Claude Code skill wrapping the CLI
- Layered rules engine (config file + skill wrapper)
- Body search should work broadly (implementation approach is flexible)

**Implementation details (figure it out):**
- Specific AppleScript crate/approach
- Config file format (TOML vs JSON)
- FTS indexing strategy for body search
- Specific Rust crates beyond rusqlite
- CLI command structure

**Date:** 2026-03-23

### Triage Sophistication
**Status:** decided
**Strength:** authoritative
**Decision:** Full Gmail-style triage from v1. Auto-trash rules, VIP sender protection, interactive triage mode, learned preferences. This is the user's proven workflow — port it, don't dumb it down.
**Date:** 2026-03-23

### Claude Code Integration
**Status:** decided
**Strength:** authoritative
**Decision:** Claude Code skill (SKILL.md) wrapping the Rust CLI binary. Same pattern as Gmail skill wraps gws + Python scripts.
**Rationale:** Skill layer holds triage logic, learned preferences, prompt templates. Binary handles data access and actions. Claude can update the skill without recompiling.
**Date:** 2026-03-23

### Adaptability / Rules Engine
**Status:** decided
**Strength:** authoritative
**Decision:** Layered approach — both config-driven rules engine AND skill wrapper.
- **Binary layer:** Config file (TOML or JSON) for rules, VIP lists, auto-trash patterns, label mappings. Claude can edit this file to add new rules without recompiling.
- **Skill layer:** SKILL.md holds higher-level triage logic, learned preferences, interactive workflows, RLM analysis patterns. Claude can self-improve this.
**Rationale:** Binary handles deterministic rules (pattern matching, sender lookups). Skill handles judgment calls (suggesting labels, grouping for triage, learning from user decisions).
**Date:** 2026-03-23
