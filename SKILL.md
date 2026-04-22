# mea — Mail Executive Assistant

## Skills

Two skills ship with this project (install both to `~/.claude/skills/mea/`):

| Skill | File | Command | Purpose |
|---|---|---|---|
| mea | `skill/SKILL.md` | `/mea <instruction>` | Daily email management, triage, briefings |
| mea-onboard | `skill/mea-onboard.md` | `/mea-onboard` | First-time setup interview |

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

- `~/.mea/overlay.db` — SQLite (graph, labels, cached bodies)
- `~/.mea/GRAPH_CONTEXT.md` — auto-generated context dump
- `~/.claude/skills/mea/PATTERNS.md` — learned triage preferences
