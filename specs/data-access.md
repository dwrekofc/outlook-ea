# Data Access

## Source
JTBD 1: See What's In My Inbox

## Topic Statement
The system reads email metadata from Apple Mail so the user can see their inbox without opening Mail.app.

## Scope
**In-scope:** Querying inbox listings, filtering by folder, read/unread status, pagination, sorting by date
**Boundaries:** Triage label filtering is owned by triage-labels. Body content is owned by body-reading. Search queries are owned by search.

## Data Contracts
- EmailSummary: { id: int, message_id: string, sender_name: string, sender_address: string, subject: string, date: ISO-8601 timestamp, is_read: bool, folder: string }
- ListResponse: { emails: EmailSummary[], total_count: int, page: int, page_size: int }

## Behaviors (execution order)
1. On listing request: query Apple Mail's Envelope Index directly for current metadata
2. On listing request with folder filter: narrow results to specified mailbox
3. On listing request with pagination: return the requested page of results with total count
4. Results are always sorted by date descending (newest first) unless otherwise specified

## Cross-Topic Shared Behavior
- Triage labels are joined from the overlay DB at display time (see triage-labels spec)
- Email IDs returned here are used by all other topics to reference specific emails

## Constraints
- Rust (edition 2024), shared library + CLI binary (`mea`)
- CLI is agent-facing: JSON output only, no interactive prompts
- Read Apple Mail's Envelope Index directly — always fresh, no caching of metadata
- Never write to Apple Mail's databases or files
- Single user, single account
- Rowid is the primary email identifier; RFC822 Message-ID stored as backup

## Acceptance Criteria
1. Listing inbox returns current emails with sender, subject, date, read status
2. Listings reflect changes made in Mail.app without any sync step
3. Pagination works with configurable page size
4. Unread vs read status is accurate against Mail.app state
5. Output is valid JSON parseable by the skill wrapper

## References
- Related: triage-labels (label data joined at display time)
- Related: overlay-db (rowid + message-id mapping)
- Related: cli-interface (command structure and output format)
