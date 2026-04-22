# mea — Quick Start Guide

mea is a CLI + Claude Code skill that turns your Apple Mail inbox into a managed system with auto-triage rules, a knowledge graph of your contacts/teams/projects, task tracking, and daily briefings. Everything runs locally — no cloud services, no API keys. Just Apple Mail, SQLite, and Claude Code.

## Prerequisites

- macOS with Apple Mail configured and receiving email
- Rust toolchain (`rustup` — https://rustup.rs)
- Claude Code CLI installed

## 1. Build the CLI

```bash
cd /path/to/outlook-ea
cargo build --release
```

## 2. Install the binary

Symlink into your PATH so it's available everywhere:

```bash
ln -sf "$(pwd)/target/release/mea" ~/.cargo/bin/mea
```

Verify: `mea --help`

## 3. Create the data directory

```bash
mkdir -p ~/.mea
```

This is where mea stores everything:

| File | Purpose | Created by |
|---|---|---|
| `~/.mea/overlay.db` | SQLite DB — graph nodes, edges, labels, cached email bodies | `mea sync` (first run) |
| `~/.mea/GRAPH_CONTEXT.md` | Human-readable dump of your graph — loaded by Claude at session start | `mea graph dump` |

## 4. Install the Claude Code skill

```bash
mkdir -p ~/.claude/skills/mea
cp skill/SKILL.md ~/.claude/skills/mea/SKILL.md
```

Create the patterns file (starts empty, grows as you triage):

```bash
cat > ~/.claude/skills/mea/PATTERNS.md << 'EOF'
# Learned Triage Preferences

Append-only log of triage patterns observed during sessions. Claude updates this as you make consistent decisions.
EOF
```

## 5. First sync

```bash
mea sync
```

This reads your Apple Mail inbox via AppleScript and caches message metadata + bodies into `~/.mea/overlay.db`. macOS will prompt you to grant Terminal/iTerm access to Mail — allow it.

## 6. Run onboarding

Open Claude Code in this project directory and run:

```
/mea-onboard
```

This starts an interactive interview that will:
- Learn who you are and what kind of email you get
- Walk through your inbox to build initial triage rules
- Set up your VIP contacts and org structure in the graph
- Establish your triage preferences in PATTERNS.md

See the "Onboarding" section below for details.

---

## Architecture

```
Apple Mail ←(AppleScript)→ mea CLI ←(SQLite)→ overlay.db
                              ↑
                        Claude Code skill
                        (SKILL.md + PATTERNS.md)
                              ↑
                         You, via /mea
```

**mea CLI** — Rust binary. Reads Mail via AppleScript, stores everything in SQLite. Handles sync, search, labels, triage rule evaluation, and the full graph CRUD. All output is JSON.

**Claude Code skill** — SKILL.md teaches Claude the CLI commands and workflows. Claude is the interface layer — it calls `mea` commands, parses JSON output, and presents things to you conversationally. You never need to run `mea` commands directly.

**Overlay DB** — All state lives in `~/.mea/overlay.db`. Labels, triage rules, graph nodes/edges, cached bodies. Nothing is written back to Apple Mail (except mark-as-read/archive/delete actions you explicitly approve).

**Graph** — A lightweight knowledge graph storing people, teams, orgs, projects, topics, vendors, and rules as nodes with typed edges (manages, reports_to, member_of, etc.). This is what powers auto-triage — rules match senders/subjects and apply actions.

**PATTERNS.md** — Append-only log of your triage preferences. Claude writes to this during triage sessions when it notices consistent decisions. On future sessions, Claude reads this to pre-suggest actions.

---

## The Default Workflow

This is how the original author uses mea daily. You can customize any of this.

### Daily Brief (`/mea daily-brief`)

Morning routine — run this first thing:
1. Syncs inbox
2. Shows counts: total / unread / untriaged
3. Previews auto-triage (dry run) — asks you to approve
4. Shows overdue and upcoming tasks
5. Highlights unread VIP emails
6. Flags unknown senders — offers to add to graph or create rules
7. Proposes new rules for high-frequency senders

### Triage (`/mea triage my inbox`)

Bulk inbox processing:
1. Auto-triage runs first (rules you've already set)
2. Remaining emails grouped by sender
3. For each group: label, archive, trash, or skip
4. Unknown senders prompt graph additions or new rules

### One-by-One Review (`/mea review each email`)

Individual email review:
1. Each email presented with sender context and body preview
2. Choose: keep, archive, trash, or capture as task
3. Summary at end

### Ad-hoc (`/mea <anything>`)

Free-form instructions work too:
- `/mea find emails from John about the Q3 budget`
- `/mea add a rule to auto-archive all emails from noreply@jira.com`
- `/mea show me my VIP contacts`
- `/mea what tasks are overdue?`

---

## Key Concepts

### Labels (1-5)
Stored in the overlay DB, not in Apple Mail:
1. **Follow Up** — needs action from you
2. **Waiting** — you're waiting on someone else
3. **Reference** — keep for reference, no action needed
4. **Read Later** — long reads, newsletters worth reading
5. **Receipts** — purchase confirmations, expense docs

### VIP Senders
People marked VIP get:
- Automatic "Follow Up" label on new emails
- Protection from trash/archive suggestions
- Priority placement in daily briefs

### Triage Rules
Rules live in the graph as `rule` nodes with edges to `action` nodes:
- **match-sender** — matches sender email/domain
- **match-subject** — matches subject line text
- **Actions:** trash, archive, or label:N

### Graph Nodes & Edges
The graph models your work world:
- **Nodes:** person, team, org, project, topic, vendor, rule, action, task
- **Edges:** manages, reports_to, member_of, leads, works_on, owns, expert_in, contact_for, collaborates, belongs_to

This lets Claude understand context — "this email is from Sarah who manages the Platform team and is your skip-level" — which informs triage suggestions.

---

## Customization Points

Everything is yours to change. Here's where to look:

| What | Where | How |
|---|---|---|
| Triage workflows | `~/.claude/skills/mea/SKILL.md` | Edit the Triage Workflow section |
| Daily brief steps | `~/.claude/skills/mea/SKILL.md` | Edit the Daily Brief Procedure |
| Label meanings | `~/.claude/skills/mea/SKILL.md` | Change the label descriptions |
| Learned preferences | `~/.claude/skills/mea/PATTERNS.md` | Append entries (never delete) |
| Auto-triage rules | Graph DB via `mea graph add-rule` | Add/remove rules |
| CLI behavior | `src/` Rust source | Modify and `cargo build --release` |

The skill is self-improving — Claude appends to PATTERNS.md during triage sessions. Over time, it learns your preferences and pre-suggests the right actions.

---

## Onboarding

The `/mea-onboard` skill runs an interactive setup interview. It will:

1. **Ask about your role** — job title, company, what kind of email you deal with
2. **Map your org** — who are your key contacts, what teams matter, who's your manager
3. **Sample your inbox** — pull a page of emails and walk through them with you to establish initial rules
4. **Set preferences** — how aggressive should auto-triage be? What's trash vs. archive vs. keep?
5. **Build your graph** — create person/team/org nodes and link them
6. **Write initial patterns** — record your decisions to PATTERNS.md

After onboarding, run `/mea daily-brief` for your first real session.

---

## Troubleshooting

**"mea: command not found"** — Symlink missing. Re-run: `ln -sf /path/to/outlook-ea/target/release/mea ~/.cargo/bin/mea`

**AppleScript permission denied** — macOS needs to grant your terminal access to Mail. Go to System Settings → Privacy & Security → Automation → enable Mail for your terminal app.

**Empty inbox after sync** — Make sure Apple Mail is open and the account is connected. `mea sync` reads from the running Mail.app instance.

**"no such table"** — Database not initialized. Run `mea sync` to create the schema.
