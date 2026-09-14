# mea — Mail Executive Assistant

## Skills

Two skills ship with this project. Derek's active installs are thin wrappers around vault-owned productivity primitives:

| Skill | File | Command | Purpose |
|---|---|---|---|
| mea | `skill/SKILL.md` | `/mea <instruction>` | Daily email management, triage, briefings |
| mea-onboard | `skill/mea-onboard.md` | `/mea-onboard` | First-time setup interview |

Canonical docs on Derek's machine:

- Mail access: `/Users/I852000/vault/utilities/ai-productivity/mail/access.md`
- Rules/logic: `/Users/I852000/vault/utilities/ai-productivity/rules/logic.md`
- Projects/tasks: `/Users/I852000/vault/utilities/ai-productivity/tasks/projects.md`
- Context graph: `/Users/I852000/vault/utilities/ai-productivity/context/graph.md`

## Quick Start

See `QUICKSTART.md` for full setup instructions.

```bash
cargo install --path . --force
mkdir -p ~/.mea ~/.claude/skills/mea
cp skill/*.md ~/.claude/skills/mea/
mea sync
# Then run /mea-onboard in Claude Code
```

## Shared storage

Configure vault first with schema migration 003. MEA reads `database_url` and `auth_token` from `~/.config/vault/config.json` (or `VAULT_CONFIG`). It never creates credentials or migrates the shared schema. Verify with `mea graph list`.

Graph reads span personal and MEA profiles; new nodes/history use MEA provenance. Use canonical IDs from `mea graph list/find`, not legacy IDs in old notes. Local preferences and generated dumps remain under `~/.mea`.

## Data

- the unified vault Turso database (`graph_*` and `mail_*`) — SQLite (graph, labels, cached bodies)
- `/Users/I852000/vault/utilities/context-profiles/mea/GRAPH_CONTEXT.md` — auto-generated context dump
- `/Users/I852000/vault/utilities/context-profiles/mea/PATTERNS.md` — learned triage preferences
- `~/.mea` remains a compatibility symlink to the vault MEA directory.
