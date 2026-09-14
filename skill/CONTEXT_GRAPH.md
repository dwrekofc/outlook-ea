# Context Graph

> **Unified 2026-09-14 (D15).** Personal and MEA context share one Turso graph.
> Reads span both profiles. New MEA nodes use MEA provenance; updates preserve
> existing provenance. Use canonical IDs from `mea graph list/find` or
> `vault graph nodes --json` for links and task/project references. Old IDs are
> import provenance only.

Use this as the guide for MEA's mail rules and the sender/subject matchers behind them.

## Source Of Truth

- Shared graph: the unified vault Turso database (`graph_*` and `mail_*`)
- Markdown dump: `/Users/I852000/vault/utilities/context-profiles/mea/GRAPH_CONTEXT.md`
- Learned preferences: `/Users/I852000/vault/utilities/context-profiles/mea/PATTERNS.md`

## Graph Commands

```bash
mea graph list [--type <type>] [--vip]
mea graph find "<query>"
mea graph show <id>
mea graph edges <id> [--predicate <pred>]
mea graph traverse <id> [--predicate <pred>] [--depth <n>]
mea graph add --type <person|team|org|project|topic|vendor|rule|action|task> --name "..." [--email "..."] [--description "..."] [--vip]
mea graph add-vip --email "..." --name "..." [--description "..."] [--context "..."]
mea graph link --from <id> --to <id> --predicate <pred> [--context "..."]
mea graph dump
```

## Update Rules

- Add context to the graph when it affects prioritization, triage, delegation, task ownership, or project understanding.
- Use Markdown notes for rich narrative/context; use graph nodes/edges for operational facts agents must reuse.
- Run `mea graph dump` after any graph mutation.
- Do not store durable context only in chat, a skill file, or a one-off note.
