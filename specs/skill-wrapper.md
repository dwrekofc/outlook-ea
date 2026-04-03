# Skill Wrapper

## Source
Constraints: Claude Code Integration | JTBD 4: Triage My Inbox Efficiently (interactive triage mode, learned preferences)

## Topic Statement
The system provides a Claude Code skill (SKILL.md) that wraps the CLI binary to orchestrate triage workflows and manage human interaction.

## Scope
**In-scope:** SKILL.md skill definition, interactive triage mode, learned preferences (PATTERNS.md), triage workflow orchestration, self-improving skill behavior, human-facing interaction via AskUserQuestion
**Boundaries:** Data access, search, labeling, actions, and rule evaluation are all owned by their respective specs and executed via the CLI. The skill only orchestrates and presents.

## Behaviors (execution order)
1. On skill invocation: accept free-form user instructions about email management
2. On triage request: call `mea` to list untriaged emails, run auto-triage, then present remaining untriaged emails for manual review
3. On interactive triage mode: group untriaged emails by sender, present groups to user via AskUserQuestion, apply user's decisions via `mea` commands
4. On any email task: translate user intent into appropriate `mea` CLI commands
5. On pattern discovery: update PATTERNS.md with learned triage preferences (what to trash, what to label, sender handling)
6. On rule suggestion: edit the rules config file to add new deterministic rules based on observed patterns

## Cross-Topic Shared Behavior
- Calls CLI commands defined in cli-interface
- Edits rules config owned by rules-engine
- Uses all data/action capabilities from other specs via `mea`

## Constraints
- Skill is a SKILL.md file invoked by Claude Code
- All human interaction happens in the skill layer (AskUserQuestion, summaries, suggestions) — the CLI has none
- Self-improving: Claude can modify SKILL.md and PATTERNS.md as triage patterns emerge
- Interactive triage mode presents groups of emails (by sender) for bulk decisions
- Learned preferences are advisory — deterministic rules go in the rules config, judgment stays in the skill
- VIP senders are never suggested for trash or archive in interactive mode

## Acceptance Criteria
1. A SKILL.md file exists that Claude Code can invoke for email management
2. Skill translates natural language email requests into `mea` CLI calls
3. Interactive triage mode groups untriaged emails by sender for review
4. Skill can update PATTERNS.md with learned preferences
5. Skill can add rules to the rules config file
6. All human-facing interaction uses AskUserQuestion (no CLI prompts)
7. VIP senders are protected in all triage suggestions

## References
- Related: cli-interface (all CLI commands the skill calls)
- Related: rules-engine (skill edits config file)
- Related: auto-triage (skill triggers auto-triage via CLI)
- Reference: Gmail skill workflow (prior art for triage UX)
