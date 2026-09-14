# Mail Access Primitive

Use this when the user asks about inbox, email triage, daily briefings, mail-derived tasks, or `/mea`.

## Source And Tools

- CLI: `mea`, installed at `~/.cargo/bin/mea`. Mail access uses the local Apple Mail store; shared storage requires vault configuration and Turso connectivity.
- Source: `/Volumes/CORE-02/projects/outlook-ea`. Install or refresh with `cargo install --path . --force`; the installed binary runs independently of the Dev Drive.
- Credentials: `~/.config/vault/config.json` (`database_url`, `auth_token`; `VAULT_CONFIG` may override its path). Vault owns shared schema migrations.
- Runtime/data: `/Users/I852000/vault/utilities/context-profiles/mea`
- Compatibility path: `~/.mea -> /Users/I852000/vault/utilities/context-profiles/mea`
- Database: the unified vault Turso database (`graph_*` and `mail_*`)
- Graph dump: `/Users/I852000/vault/utilities/context-profiles/mea/GRAPH_CONTEXT.md`

## Read Workflow

1. Load `LOGIC_RULES.md`, `PROJECTS_TASKS.md`, and `CONTEXT_GRAPH.md`.
2. Run `mea sync` before inbox review unless the user asks for offline/current cached state.
3. Use `mea list --page-size <n>` for Inbox state; use `mea search` for date/sender/subject/body filters.
4. Read every candidate before proposing archive/trash/capture: `mea read <id> [--all-folders]`.
5. Present proposed actions in chat first unless the user explicitly authorizes execution.

## Core Commands

```bash
mea sync
mea list [--folder <name>] [--page <n>] [--page-size <n>] [--label <1-5>] [--untriaged] [--needs-reply]
mea search [--sender <text>] [--subject <text>] [--date-from <ISO>] [--date-to <ISO>] [--body <text>]
mea read <id> [--all-folders]
mea thread <id>
mea triage [--dry-run]
mea label <id> <0-5>
mea archive --yes <id>...
mea delete --yes <id>...
mea mark-read <id> [--unread]
```

Labels: `1=Follow Up`, `2=Waiting`, `3=Reference`, `4=Read Later`, `5=Receipts`, `0=Clear`.

## Thread / Reply Awareness

`mea` reads the Sent folder, so reply status is first-class:

- Every `mea list`/`mea read` result includes `conversation_id` (Apple Mail's thread key linking inbox ↔ sent ↔ archive).
- `mea thread <id>` returns the full conversation across all folders, each message tagged `direction` `in`/`out` (out = Sent folder or a self-address), plus a `status`: `you_replied_last` (ball in their court) or `awaiting_your_reply` (last message inbound). Use this to see what the user has already replied to before proposing follow-ups.
- `mea list --needs-reply` filters to inbox messages genuinely awaiting the user's reply: latest thread message is inbound, sender is a real person (automated/no-reply senders and calendar/auto-reply notices excluded), AND the user is a direct `To` recipient (not just CC'd). This is a far better "Follow Up" signal than sender-VIP labeling.
- Self-addresses are detected config-free as the dominant senders in the Sent folder (≥1% of the top), so this needs no hardcoded address.

## Safety

- All `mea` output is JSON; parse it before presenting.
- Destructive actions require explicit user confirmation and `--yes`.
- Do not bulk execute dry-run suggestions without reading for false positives.
- VIP senders are never trash/archive candidates unless the user explicitly overrides.

Shared context uses one Turso graph, profiles `personal` | `mea`, through
`vault graph`, `mea graph`, or `pcg`. Use canonical IDs and the shared
`~/.config/vault/config.json`; vault-cli owns migrations.
