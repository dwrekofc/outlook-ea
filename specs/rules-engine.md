# Rules Engine

## Source
JTBD 6: Define and Manage Rules

## Topic Statement
The system stores and manages triage rules in a config file so email patterns are handled automatically without recompiling.

## Scope
**In-scope:** Rule definitions (pattern→action), VIP sender list, auto-trash patterns, auto-label mappings, rule review/listing, config file that Claude can edit
**Boundaries:** Rule execution/evaluation is owned by auto-triage. Learned preferences and higher-level judgment live in skill-wrapper.

## Data Contracts
- Rule: { name: string, match: MatchCriteria, action: Action }
- MatchCriteria: { sender_contains: string?, sender_exact: string?, subject_contains: string?, any_of: MatchCriteria[]? }
- Action: { type: "label" | "trash" | "archive", label_number: int? }
- VipSender: { address: string, name: string? }
- RulesConfig: { rules: Rule[], vip_senders: VipSender[] }

## Behaviors (execution order)
1. On rule list request: read the config file and return all defined rules with their match criteria and actions
2. On VIP list request: return all VIP senders from the config
3. Config file is human-readable and editable by Claude via the skill wrapper
4. Rules are evaluated in order — first match wins
5. VIP rules take precedence over all other rules (VIP sender → always Follow Up, never bulk-actioned)

## Cross-Topic Shared Behavior
- Auto-triage reads rules from this config to evaluate emails (see auto-triage spec)
- Skill-wrapper can add/modify rules by editing the config file

## Constraints
- Rust (edition 2024), shared library + CLI binary (`mea`)
- CLI is agent-facing: JSON output only, no interactive prompts
- Config file is a flat file (not in the overlay DB) — Claude can read and edit it directly
- Rules handle deterministic pattern matching only — AI/judgment-based triage is in the skill layer
- Single user, single account

## Acceptance Criteria
1. Rules are defined in a config file readable by the binary
2. Claude can add new rules by editing the config file without recompiling
3. VIP senders can be listed and managed in the config
4. Rules can match on sender (exact or contains) and subject (contains)
5. Rules map to actions: label, trash, or archive
6. Listing rules returns all defined rules with their criteria and actions
7. Rule order determines priority (first match wins)

## References
- Related: auto-triage (evaluates rules at triage time)
- Related: skill-wrapper (edits config, holds learned preferences)
