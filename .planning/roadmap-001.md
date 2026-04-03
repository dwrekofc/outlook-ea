---
session: "001"
summary: "Future phases beyond v1 email triage — threading, AI screening, calendar, GPUI app"
reqs_file: ".planning/reqs-001.md"
created: "2026-03-23"
last_updated: "2026-03-23"
---

## Phase 2: Intelligence Layer
- **AI email screening** — Auto-categorize emails using LLM (action-required, internal-fyi, meeting-invite, noise). *Deferred because: get basics working first, adds OpenRouter dependency and prompt engineering complexity.*
- **Email threading** — Group emails into conversations via Message-ID/References/In-Reply-To headers. *Deferred because: was 0/30 tasks in swiftea, user survived without it, individual email workflow works fine for now.*

## Phase 3: Export & Integration
- **Markdown export** — Export emails as .md files with YAML frontmatter for Obsidian compatibility. *Deferred because: CLI + overlay DB is sufficient for v1. Export adds file management complexity.*
- **JSON export** — Structured JSON output for integration with other tools. *Deferred because: CLI stdout is sufficient for v1.*

## Phase 4: PIM Expansion
- **Calendar module** — Mirror Apple Calendar events, search, export. swiftea had a draft proposal using EventKit. *Deferred because: user explicitly scoped v1 to email only.*
- **Contacts module** — Access Apple Contacts for sender enrichment. *Deferred because: not needed for email triage.*
- **Reminders integration** — Create reminders from flagged emails. *Deferred because: not needed for v1 workflow.*

## Phase 5: Desktop App
- **GPUI app** — Native desktop email management UI built with Zed's GPUI framework. Keyboard-first, virtualized list, column sort/resize (inspired by the swiftea Obsidian plugin). *Deferred because: need the core library stable first. The shared Rust library is designed to support this.*

## Someday / Maybe
- **Reply/compose from CLI** — Draft or send emails without opening Mail.app. *Deferred because: user explicitly said they don't need help with replying.*
- **Multi-account support** — Manage multiple Apple Mail accounts. *Deferred because: user has one account. Would require vault-like abstraction.*
- **Receipt sub-categorization** — Sub-labels for receipts (like Gmail's 19 sub-labels). *Deferred because: work email likely has fewer receipt types than personal Gmail.*
