# Search

## Source
JTBD 3: Search My Email

## Topic Statement
The system finds emails matching user queries across metadata fields and body content so the user can locate specific emails quickly.

## Scope
**In-scope:** Search by sender, subject, date range, full-text body search, combining filters, result ranking
**Boundaries:** Listing/browsing inbox is owned by data-access. Triage label filtering is owned by triage-labels.

## Data Contracts
- SearchQuery: { sender: string?, subject: string?, date_from: ISO-8601?, date_to: ISO-8601?, body_text: string? }
- SearchResult: { emails: EmailSummary[], total_count: int }

## Behaviors (execution order)
1. On search with metadata filters (sender, subject, date): query Apple Mail's Envelope Index directly via SQL
2. On search with body text: use macOS Spotlight (`mdfind`) to find matching emails across all mail content
3. On search with both metadata and body filters: intersect results from both sources
4. Results are returned with the same EmailSummary shape as inbox listings
5. Results are ranked by relevance (Spotlight ranking for body, date for metadata-only)

## Cross-Topic Shared Behavior
- Returns the same EmailSummary shape as data-access for consistency
- Triage labels can be joined to search results (see triage-labels spec)

## Constraints
- Rust (edition 2024), shared library + CLI binary (`mea`)
- CLI is agent-facing: JSON output only, no interactive prompts
- Metadata search hits Envelope Index directly — always fresh
- Body search uses macOS Spotlight (`mdfind`) — no custom FTS index needed
- Body search covers all emails regardless of whether they've been opened in this tool
- Search does not depend on the overlay DB body cache
- Single user, single account

## Acceptance Criteria
1. Search by sender address or name returns matching emails
2. Search by subject keywords returns matching emails
3. Search by date range narrows results to that period
4. Body text search returns emails containing the keyword even if never opened in this tool
5. Multiple filters can be combined in a single query
6. Output is valid JSON parseable by the skill wrapper

## References
- Related: data-access (shared EmailSummary shape)
- Related: body-reading (body cache is independent of search)
