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
cargo build --release
ln -sf "$(pwd)/target/release/mea" ~/.cargo/bin/mea
mkdir -p ~/.mea ~/.claude/skills/mea
cp skill/SKILL.md ~/.claude/skills/mea/SKILL.md
cp skill/mea-onboard.md ~/.claude/skills/mea/mea-onboard.md
mea sync
# Then run /mea-onboard in Claude Code
```

## Data

- `/Users/I852000/vault/utilities/context-profiles/mea/overlay.db` — SQLite (graph, labels, cached bodies)
- `/Users/I852000/vault/utilities/context-profiles/mea/GRAPH_CONTEXT.md` — auto-generated context dump
- `/Users/I852000/vault/utilities/context-profiles/mea/PATTERNS.md` — learned triage preferences
- `~/.mea` remains a compatibility symlink to the vault MEA directory.
