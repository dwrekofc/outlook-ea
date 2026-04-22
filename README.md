# mea — Mail Executive Assistant

A local-first email management system for macOS. Rust CLI + Claude Code skill that reads Apple Mail via AppleScript, stores state in SQLite, and gives you an AI-powered triage workflow with a knowledge graph of your contacts, teams, and projects.

No cloud services. No API keys. No data leaves your machine.

```
Apple Mail ←──AppleScript──→ mea CLI ←──SQLite──→ overlay.db
                                ↑
                          Claude Code skill
                        (SKILL.md + PATTERNS.md)
                                ↑
                           You, via /mea
```

## What It Does

- **Sync** your Apple Mail inbox into a local SQLite database
- **Triage** emails with auto-rules that match senders and subjects → trash, archive, or label
- **Knowledge graph** of people, teams, orgs, projects, topics, and vendors — Claude uses this to understand who's emailing you and why it matters
- **VIP protection** — important senders are never auto-trashed or archived
- **Task tracking** — capture action items from emails as graph-linked tasks with due dates
- **Daily briefings** — morning summary of inbox state, overdue tasks, VIP emails, and unknown senders
- **Self-improving** — Claude logs your triage decisions to PATTERNS.md and gets smarter over time

## Quick Start

> Full walkthrough in [QUICKSTART.md](QUICKSTART.md)

```bash
# Build
cargo build --release

# Install binary
ln -sf "$(pwd)/target/release/mea" ~/.cargo/bin/mea

# Install Claude Code skills
mkdir -p ~/.claude/skills/mea
cp skill/SKILL.md ~/.claude/skills/mea/SKILL.md
cp skill/mea-onboard.md ~/.claude/skills/mea/mea-onboard.md

# First sync (creates ~/.mea/ and the database)
mea sync

# Interactive onboarding (in Claude Code)
/mea-onboard
```

## Requirements

- macOS (Apple Mail + AppleScript — no Windows/Linux support)
- Rust toolchain ([rustup.rs](https://rustup.rs))
- [Claude Code](https://claude.ai/claude-code) CLI

## How It Works

### The CLI (`mea`)

A Rust binary that does the heavy lifting. All output is JSON — it's designed to be called by Claude, not used directly (though you can).

| Command | What it does |
|---|---|
| `mea sync` | Tells Mail.app to check for new mail |
| `mea list` | List inbox emails with pagination, label/untriaged filters |
| `mea read <id>` | Read full email body (cached in SQLite after first read) |
| `mea search` | Search by sender, subject, date range, or body text (Spotlight) |
| `mea label <id> <1-5>` | Assign a triage label (local only, not in Mail.app) |
| `mea delete --yes <id>` | Move to trash (VIP-protected) |
| `mea archive --yes <id>` | Archive and mark read (VIP-protected) |
| `mea mark-read <id>` | Mark as read/unread |
| `mea flag <id>` | Flag/unflag |
| `mea triage` | Run auto-triage rules against untriaged emails |
| `mea graph *` | Full CRUD for the knowledge graph (see below) |

### The Skill (`/mea`)

A Claude Code skill file that teaches Claude how to use the CLI. It defines:

- **Daily Brief** — 8-step morning routine (sync → counts → auto-triage preview → tasks → VIPs → unknown senders → rule proposals → summary)
- **Triage Workflow** — batch and one-by-one review modes with graph context
- **Task Capture** — detect action items in emails and create linked tasks
- **Self-Improvement** — Claude edits SKILL.md and appends to PATTERNS.md as it learns

### The Onboarding Skill (`/mea-onboard`)

An interactive interview that sets up a new user's mea instance from scratch. It walks through 8 phases:

1. Who you are (role, company, inbox character)
2. Org structure (manager, reports, collaborators → graph nodes)
3. Inbox sampling (real emails → initial triage rules)
4. Label preferences (keep defaults or customize)
5. Triage aggressiveness (conservative / moderate / aggressive)
6. Workflow preferences (daily brief? batch vs one-by-one?)
7. Feature wishlist (what you wish existed)
8. Finalize (dump graph, summary, first daily brief)

## Knowledge Graph

The graph is the core differentiator. Instead of flat rules, mea builds a model of your work world:

```
┌─────────┐  manages   ┌──────────┐  member_of  ┌──────┐
│  Alice   │──────────→│  Platform │←────────────│ Bob  │
│ (person) │           │  (team)   │             │(VIP) │
└─────────┘           └──────────┘             └──────┘
                           │                       │
                      works_on                expert_in
                           ↓                       ↓
                      ┌─────────┐            ┌─────────┐
                      │ Q3 Infra│            │  Auth   │
                      │(project)│            │ (topic) │
                      └─────────┘            └─────────┘
```

**Node types:** person, team, org, project, topic, vendor, rule, action, task

**Edge types:** manages, reports_to, member_of, leads, works_on, owns, expert_in, contact_for, collaborates, belongs_to, matches_sender, matches_subject, applies_action, protects

**Triage rules** are graph nodes too — a `rule` node connects to a `matches_sender` edge and an `applies_action` edge. This means rules are queryable, linkable, and visible alongside the rest of your context.

### Graph Commands

```bash
# People
mea graph add --type person --name "Alice" --email "alice@company.com" --description "Platform lead"
mea graph add-vip --email "boss@company.com" --name "Boss" --description "Skip-level"

# Teams & Orgs
mea graph add --type team --name "Platform"
mea graph link --from 1 --to 2 --predicate member_of

# Rules
mea graph add-rule --name "Jira noise" --match-sender "noreply@jira.com" --action archive
mea graph add-rule --name "Marketing spam" --match-subject "webinar" --action trash

# Tasks
mea graph add-task --title "Review Q3 plan" --due "2026-05-01" --project 5
mea graph tasks --status todo

# Explore
mea graph show 1          # node + all edges
mea graph find "alice"    # search by name/email/description
mea graph traverse 1 --depth 3
mea graph dump            # export full graph as markdown
```

## Labels

Labels are stored in the overlay DB, not in Apple Mail. They're local-only triage markers:

| # | Name | Meaning |
|---|---|---|
| 1 | Follow Up | Needs action from you |
| 2 | Waiting | Waiting on someone else |
| 3 | Reference | Keep for reference, no action |
| 4 | Read Later | Long reads, newsletters |
| 5 | Receipts | Purchase confirmations, expenses |
| 0 | (clear) | Remove label |

## Data Storage

Everything lives under `~/.mea/`:

| File | Contents |
|---|---|
| `overlay.db` | SQLite — graph nodes/edges, labels, triage state, cached email bodies |
| `GRAPH_CONTEXT.md` | Auto-generated markdown dump of the graph (read by Claude at session start) |

And under `~/.claude/skills/mea/`:

| File | Contents |
|---|---|
| `SKILL.md` | The Claude Code skill — workflows, commands, rules |
| `mea-onboard.md` | Onboarding interview skill |
| `PATTERNS.md` | Append-only log of learned triage preferences |

## Project Structure

```
src/
├── bin/mea.rs    — CLI entrypoint, command dispatch
├── cli.rs        — Clap argument parsing, JSON response formatters
├── data.rs       — Apple Mail envelope DB reader (V10 schema), email listing
├── db.rs         — Overlay SQLite schema, migrations, connection management
├── graph.rs      — Knowledge graph CRUD, traversal, VIP logic, rule engine
├── body.rs       — Email body extraction, MIME parsing, HTML→text, caching
├── labels.rs     — Label assignment and lookup
├── triage.rs     — Auto-triage engine (evaluate rules against untriaged emails)
├── rules.rs      — Legacy TOML rules loader (fallback for pre-graph configs)
├── search.rs     — Multi-field search with Spotlight body search
├── actions.rs    — AppleScript actions (delete, archive, flag, mark-read)
└── lib.rs        — Module declarations
skill/
├── SKILL.md          — Sanitized Claude Code skill (copy to ~/.claude/skills/mea/)
└── mea-onboard.md    — Onboarding interview skill
```

## How Apple Mail Integration Works

mea reads Apple Mail's local SQLite database directly (the V10 envelope index at `~/Library/Mail/V10/MailData/Envelope Index`). This is read-only — mea never writes to Mail's database.

For actions that modify mailbox state (delete, archive, flag, mark-read), mea shells out to `osascript` to run AppleScript commands against Mail.app. This requires macOS Automation permissions.

Email bodies are extracted from the `.emlx` files on disk, parsed with `mailparse`, converted from HTML to text with `html2text`, and cached in the overlay DB for fast re-reads.

## Customization

This project was built for one person's workflow, then generalized. You can change anything:

| Layer | What to change | How |
|---|---|---|
| **Workflows** | Daily brief steps, triage flow, review modes | Edit `~/.claude/skills/mea/SKILL.md` |
| **Rules** | What gets auto-trashed, archived, or labeled | `mea graph add-rule` or edit rules in graph |
| **Labels** | Rename or repurpose the 5 label slots | Edit SKILL.md label descriptions |
| **Graph schema** | Add new node/edge types | Modify `src/graph.rs` |
| **CLI commands** | Add new subcommands or flags | Modify `src/cli.rs` + `src/bin/mea.rs` |
| **Mail source** | Adapt for a different mail client | Replace `src/data.rs` and `src/actions.rs` |
| **Preferences** | Triage aggressiveness, patterns | Append to `~/.claude/skills/mea/PATTERNS.md` |

The `/mea-onboard` skill is designed to help you set up your own version interactively — it interviews you about your role, org, and preferences, then configures everything.

## Design Decisions

**Why Apple Mail?** It stores email locally in a well-documented SQLite + emlx format. No OAuth, no API rate limits, no token refresh. The data is already on your disk.

**Why a separate overlay DB?** Apple Mail's database is read-only (and shared with the system). The overlay keeps mea's state (labels, graph, cached bodies) separate so there's zero risk of corrupting Mail.

**Why Claude Code as the interface?** Email triage is a judgment call — "is this important?" depends on context that's hard to encode in static rules. Claude reads the graph, the patterns, and the email content, then makes suggestions. The skill file is the control surface — you can change how Claude behaves by editing markdown.

**Why not Outlook/Gmail API?** This was built for a setup where email comes through Apple Mail regardless of the upstream provider (Exchange, Gmail, iCloud). The integration point is the local mail store, not the server.

## License

MIT
