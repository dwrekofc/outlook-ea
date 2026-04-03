# Triage Labels

## Source
JTBD 4: Triage My Inbox Efficiently (labeling subset)

## Topic Statement
The system allows the user to assign, clear, and filter by numbered triage labels so emails are categorized for their workflow.

## Scope
**In-scope:** Assigning labels 1-5, clearing labels (0), querying by label, detecting untriaged emails, joining labels to listings
**Boundaries:** Auto-applying labels is owned by auto-triage. Rule definitions are owned by rules-engine. Interactive triage mode lives in skill-wrapper.

## Data Contracts
- TriageLabel: { id: int, label_number: int (1-5), label_name: string, assigned_at: ISO-8601 timestamp }
- Label definitions (fixed):
  - 1: Follow Up — requires user's attention or action
  - 2: Waiting — waiting on someone else
  - 3: Reference — important reference material (use sparingly)
  - 4: Read Later — interesting but not actionable
  - 5: Receipts — receipts, invoices, purchases

## Behaviors (execution order)
1. On label assign (email_id, label 1-5): store the label in the overlay DB, replacing any existing label
2. On label clear (email_id, 0): remove any label from the email in the overlay DB
3. On list with label filter: join overlay DB labels with Envelope Index metadata to return only emails with the specified label
4. On list untriaged: return emails that have no label assigned in the overlay DB
5. Labels 1-4 are manually assigned only (user decides, tool can suggest)
6. Label 5 (Receipts) can also be auto-applied by rules (see auto-triage)

## Cross-Topic Shared Behavior
- Label data is joined to EmailSummary results from data-access at display time
- Auto-triage can assign labels programmatically (see auto-triage spec)
- Labels are stored in overlay-db

## Constraints
- Rust (edition 2024), shared library + CLI binary (`mea`)
- CLI is agent-facing: JSON output only, no interactive prompts
- Labels stored in overlay DB, keyed by email rowid
- Label set is fixed (1-5) — no custom labels in v1
- Single user, single account

## Acceptance Criteria
1. User can assign any label 1-5 to an email
2. User can clear a label with 0
3. Inbox listing can be filtered to show only emails with a specific label
4. Untriaged emails (no label) can be listed separately
5. Assigning a new label replaces any existing label on that email
6. Labels persist across tool restarts (stored in overlay DB)

## References
- Related: auto-triage (auto-applies labels via rules)
- Related: data-access (labels joined to listings)
- Related: overlay-db (label storage)
