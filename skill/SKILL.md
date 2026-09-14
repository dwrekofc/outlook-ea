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

This source skill ships repo-owned reference copies of the vault productivity guides:

- `MAIL_ACCESS.md` — mail commands and workflow
- `LOGIC_RULES.md` — triage rules
- `PROJECTS_TASKS.md` — task and project capture
- `CONTEXT_GRAPH.md` — unified graph operations

Before doing mail/task/context work, read those four references. Use `mea` for mail access and graph mutations, then run `mea graph dump` after changes.

Canonical runtime data on Derek's machine:

- MEA data: `${VAULT_NOTES}/utilities/context-profiles/mea`
- Graph dump: `${VAULT_NOTES}/utilities/context-profiles/mea/MEA_GRAPH_CONTEXT.md`
- Learned preferences: `${VAULT_NOTES}/utilities/context-profiles/mea/PATTERNS.md`
- Shared graph/tasks/rules/mail storage: the unified vault Turso database (`graph_*` and `mail_*`)

Do not store durable productivity logic only inside this skill file.

MEA reads vault’s `database_url` and `auth_token` from `~/.config/vault/config.json` (or `VAULT_CONFIG`). Vault owns schema migration 003; MEA uses the existing schema.

Shared context uses one Turso graph, profiles `personal` | `mea`, through
`vault graph`, `mea graph`, or `pcg`. Use canonical IDs and the shared
`~/.config/vault/config.json`; vault-cli owns migrations.

`VAULT_NOTES` denotes `notes_path` from `VAULT_CONFIG` (default `~/.config/vault/config.json`), falling back to `~/vault`. Resolve it before running shell examples. MEA owns `MEA_GRAPH_CONTEXT.md`; the scheduled vault graph-dump script owns the separate `GRAPH_CONTEXT.md`.
