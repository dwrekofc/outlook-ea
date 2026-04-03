# Auto-Triage

## Source
JTBD 4: Triage My Inbox Efficiently (automation subset) | JTBD 6: Define and Manage Rules

## Topic Statement
The system automatically categorizes obvious emails using pattern-matching rules so the user only manually triages ambiguous messages.

## Scope
**In-scope:** Auto-labeling emails by rule, auto-trashing known noise, VIP sender protection, running rules against untriaged emails
**Boundaries:** Manual label assignment is owned by triage-labels. Rule definition/storage is owned by rules-engine. Interactive triage mode and learned preferences live in skill-wrapper.

## Behaviors (execution order)
1. On auto-triage request: fetch all untriaged emails from the inbox
2. For each untriaged email: evaluate against the rules config in priority order
3. If email matches a VIP sender rule: auto-label as "Follow Up" (label 1), protect from bulk actions
4. If email matches an auto-label rule (e.g., receipts pattern): apply the specified label
5. If email matches an auto-trash rule (e.g., food order confirmations): move to trash via mail-actions
6. If no rule matches: leave as untriaged for manual triage
7. Return a summary of actions taken (count per category, which emails were affected)

## Cross-Topic Shared Behavior
- Reads rules from the rules config file (see rules-engine spec)
- Applies labels via triage-labels
- Performs trash/archive actions via mail-actions
- VIP-protected emails are excluded from bulk actions in mail-actions

## Constraints
- Rust (edition 2024), shared library + CLI binary (`mea`)
- CLI is agent-facing: JSON output only, no interactive prompts
- VIP senders are never bulk-actioned (trashed, archived) — always protected
- Auto-triage is deterministic: based on config file rules, not AI judgment
- Higher-level judgment (suggesting labels, learning from decisions) lives in the skill layer, not here
- Single user, single account

## Acceptance Criteria
1. Running auto-triage labels receipts matching receipt patterns as label 5
2. Running auto-triage trashes emails matching auto-trash patterns
3. VIP sender emails are auto-labeled Follow Up (label 1)
4. VIP sender emails are never trashed or archived by auto-triage
5. Emails matching no rule remain untriaged
6. Auto-triage returns a summary of all actions taken
7. Auto-triage is idempotent — running it twice doesn't double-label or re-trash

## References
- Related: rules-engine (rule definitions and config)
- Related: triage-labels (label application)
- Related: mail-actions (trash/archive execution)
- Related: skill-wrapper (higher-level learned preferences)
