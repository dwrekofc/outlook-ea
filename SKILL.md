# mea — Mail Executive Assistant

This skill is installed at `~/.claude/skills/mea/SKILL.md`. Edit there, not here.

Invoke with `/mea <instruction>` or `/mea daily-brief` from any Claude Code session.

## Source & Rebuild

```bash
cd /Volumes/CORE-02/projects/outlook-ea
cargo build --release
# Symlink at ~/.cargo/bin/mea auto-updates
```

## Data

- `~/.mea/overlay.db` — SQLite (graph, labels, cached bodies)
- `~/.mea/GRAPH_CONTEXT.md` — auto-generated context dump
- `~/.claude/skills/mea/PATTERNS.md` — learned triage preferences
