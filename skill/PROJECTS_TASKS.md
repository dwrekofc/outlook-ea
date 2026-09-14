# Projects And Tasks

> **Unified 2026-09-14 (D15).** Projects, tasks, people, and mail rules share one
> Turso graph. MEA reads both profiles and creates nodes with MEA provenance.
> Reuse existing nodes and canonical IDs from `mea graph list/find`.

## Source Of Truth

- MEA graph tasks/projects currently live in the unified vault Turso database (`graph_*` and `mail_*`).
- Human-readable task/project context is dumped to `/Users/I852000/vault/utilities/context-profiles/mea/GRAPH_CONTEXT.md`.
- New Markdown project notes belong in `/Users/I852000/vault/1-projects`.
- Ongoing responsibility notes belong in `/Users/I852000/vault/2-areas`.
- Raw captures belong in `/Users/I852000/vault/inbox`.

## Commands

```bash
mea graph add-project --name "..." [--description "..."]
mea graph add-task --title "..." [--description "..."] [--due "YYYY-MM-DD"] [--project <id>]
mea graph tasks [--project <id>] [--status todo|in_progress|done|blocked]
mea graph projects [--active]
mea graph task-status <id> --status done|in_progress|blocked|todo
mea graph link --from <id> --to <id> --predicate <pred> [--context "..."]
mea graph dump
```

## Capture Standard

Every task needs: source, owner, due date if known, next action, project/area, and why it matters.

For email tasks, include:

```text
SUMMARY:
SOURCE EMAIL: <id> | received <YYYY-MM-DD HH:MM>
FROM:
CC:
APPLE MAIL:
ACTIONABLE LINK:
CONTEXT:
```

Prefer rolling tasks up to an existing project. If no project fits and the work is ongoing, attach context to an Area. Orphan tasks should be rare.
