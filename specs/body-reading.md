# Body Reading

## Source
JTBD 2: Read an Email's Content

## Topic Statement
The system retrieves and displays an email's body content in a clean readable format so the user can understand what an email says without opening Mail.app.

## Scope
**In-scope:** Fetching email body by ID, converting HTML to readable text, caching parsed bodies, displaying email headers alongside body
**Boundaries:** Search does not depend on cached bodies (see search spec). Metadata listings are owned by data-access.

## Data Contracts
- EmailDetail: { id: int, message_id: string, from: string, to: string[], cc: string[], date: ISO-8601 timestamp, subject: string, body_text: string, body_format: "plain" | "markdown" }

## Behaviors (execution order)
1. On read request: check overlay DB for cached body
2. If cached: return cached body immediately
3. If not cached: locate the email's on-disk file, parse it, extract body content
4. If body is HTML: convert to markdown for readability
5. If body is plain text: use as-is
6. Cache the parsed body in the overlay DB for future reads
7. Return body with email headers (from, to, cc, date, subject)

## Cross-Topic Shared Behavior
- Uses email IDs from data-access to locate specific emails
- Cached bodies are stored in overlay-db

## Constraints
- Rust (edition 2024), shared library + CLI binary (`mea`)
- CLI is agent-facing: JSON output only, no interactive prompts
- Read-only against Apple Mail's data files — never modify them
- Lazy loading: bodies parsed on first read, not proactively
- Plain text preferred when available; HTML converted to markdown as fallback
- Single user, single account

## Acceptance Criteria
1. Reading an email by ID returns its full body text with headers
2. HTML emails are converted to readable markdown
3. Second read of the same email returns instantly from cache
4. Plain text emails are returned as-is without conversion
5. Output is valid JSON parseable by the skill wrapper

## References
- Related: data-access (provides email IDs and metadata)
- Related: overlay-db (stores cached bodies)
- Related: search (search does not depend on body cache)
