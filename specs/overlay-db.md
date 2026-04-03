# Overlay DB

## Source
JTBD 1-6 (foundational — supports all jobs)

## Topic Statement
The system maintains a local overlay database for user metadata that Apple Mail doesn't track, keyed by email identity.

## Scope
**In-scope:** Database schema, email identity mapping (rowid + Message-ID), label storage, cached body storage, database lifecycle (creation, migrations)
**Boundaries:** What gets stored in each table is defined by the owning topic (triage-labels owns label records, body-reading owns cached bodies). Rules config is a separate flat file owned by rules-engine.

## Data Contracts
- EmailIdentity: { rowid: int (primary key), message_id: string (backup key) }
- LabelRecord: { rowid: int, label_number: int (1-5), assigned_at: ISO-8601 timestamp }
- CachedBody: { rowid: int, body_text: string, body_format: "plain" | "markdown", cached_at: ISO-8601 timestamp }

## Behaviors (execution order)
1. On first run: create the overlay database if it doesn't exist
2. On schema change: apply migrations to bring database to current version
3. On any email reference: store rowid + Message-ID mapping for resilience
4. If Apple Mail rebuilds its Envelope Index (rowids change): Message-ID backup enables re-mapping [inferred]

## Cross-Topic Shared Behavior
- Triage-labels stores and queries label records here
- Body-reading stores and queries cached bodies here
- Data-access uses rowid as the primary email reference

## Constraints
- Rust (edition 2024), shared library + CLI binary (`mea`)
- Overlay DB is separate from Apple Mail's databases — never modify Apple's data
- Rowid is primary key for all overlay records
- Message-ID stored as backup for index rebuild recovery
- Database is local-only, single user
- No full mirror of Apple Mail data — only user metadata and cached bodies

## Acceptance Criteria
1. Overlay database is created automatically on first run
2. Labels persist across tool restarts
3. Cached bodies persist across tool restarts
4. Email identity mapping (rowid + Message-ID) is stored for every referenced email
5. Database schema supports migrations for future changes

## References
- Related: triage-labels (label storage)
- Related: body-reading (body cache storage)
- Related: data-access (email identity references)
