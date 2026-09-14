---
name: mea
description: Apple Mail inbox access wrapper for the vault-owned AI productivity system. Use for triage, daily briefings, task capture, projects, and context graph updates.
allowed-tools:
  - Read
  - Write
  - Edit
  - Bash
  - Grep
  - Glob
  - Agent
  - AskUserQuestion
user-invocable: true
argument-hint: <instruction or "daily-brief">
---

# MEA Skill

The user's request is in `$ARGUMENTS`.

This source skill is a thin wrapper. On Derek's machine, durable rules, projects, tasks, and context live in the iCloud vault and are symlinked into installed skill instances:

- `MAIL_ACCESS.md` -> `/Users/I852000/vault/utilities/ai-productivity/mail/access.md`
- `LOGIC_RULES.md` -> `/Users/I852000/vault/utilities/ai-productivity/rules/logic.md`
- `PROJECTS_TASKS.md` -> `/Users/I852000/vault/utilities/ai-productivity/tasks/projects.md`
- `CONTEXT_GRAPH.md` -> `/Users/I852000/vault/utilities/ai-productivity/context/graph.md`

Before doing mail/task/context work, read those four references. Use `mea` for mail access and graph mutations, then run `mea graph dump` after changes.

Canonical runtime data on Derek's machine:

- MEA data: `/Users/I852000/vault/utilities/context-profiles/mea`
- Graph dump: `/Users/I852000/vault/utilities/context-profiles/mea/GRAPH_CONTEXT.md`
- Learned preferences: `/Users/I852000/vault/utilities/context-profiles/mea/PATTERNS.md`
- SQLite graph/tasks/rules/mail overlay: `/Users/I852000/vault/utilities/context-profiles/mea/overlay.db`

Do not store durable productivity logic only inside this skill file.
