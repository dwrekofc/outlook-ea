-- Test fixture: exact vault-cli migration 003_graph_mail.sql; vault owns this schema.
PRAGMA foreign_keys = ON;
BEGIN IMMEDIATE;

CREATE TABLE IF NOT EXISTS graph_nodes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    node_type TEXT NOT NULL,
    name TEXT NOT NULL,
    email TEXT,
    description TEXT,
    status TEXT,
    due_date TEXT,
    metadata TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(metadata)),
    is_vip INTEGER NOT NULL DEFAULT 0,
    archived INTEGER NOT NULL DEFAULT 0,
    event_start TEXT,
    event_end TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    profile TEXT NOT NULL CHECK(profile IN ('personal', 'mea')),
    source_id INTEGER
);
CREATE INDEX IF NOT EXISTS idx_graph_nodes_type ON graph_nodes(node_type);
CREATE INDEX IF NOT EXISTS idx_graph_nodes_status ON graph_nodes(status);
CREATE INDEX IF NOT EXISTS idx_graph_nodes_profile ON graph_nodes(profile);
-- Keep source spelling: MEA contains two addresses differing only in case.
-- Import identity matching is case-insensitive; storage preserves both originals.
CREATE UNIQUE INDEX IF NOT EXISTS idx_graph_nodes_email ON graph_nodes(email)
    WHERE email IS NOT NULL AND node_type != 'email';
CREATE UNIQUE INDEX IF NOT EXISTS idx_graph_email_message_id
    ON graph_nodes(json_extract(metadata, '$.message_id')) WHERE node_type = 'email';

CREATE TABLE IF NOT EXISTS graph_edges (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id INTEGER NOT NULL REFERENCES graph_nodes(id) ON DELETE CASCADE,
    target_id INTEGER NOT NULL REFERENCES graph_nodes(id) ON DELETE CASCADE,
    predicate TEXT NOT NULL,
    context TEXT,
    weight REAL DEFAULT 1.0,
    created_at TEXT NOT NULL,
    metadata TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(metadata)),
    UNIQUE(source_id, target_id, predicate)
);
CREATE INDEX IF NOT EXISTS idx_graph_edges_source ON graph_edges(source_id);
CREATE INDEX IF NOT EXISTS idx_graph_edges_target ON graph_edges(target_id);
CREATE INDEX IF NOT EXISTS idx_graph_edges_predicate ON graph_edges(predicate);

CREATE TABLE IF NOT EXISTS graph_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    ts TEXT NOT NULL,
    node_id INTEGER REFERENCES graph_nodes(id) ON DELETE SET NULL,
    source_node_id INTEGER,
    event TEXT NOT NULL,
    detail TEXT,
    profile TEXT NOT NULL CHECK(profile IN ('personal', 'mea')),
    source_id INTEGER,
    UNIQUE(profile, source_id)
);
CREATE INDEX IF NOT EXISTS idx_graph_history_node ON graph_history(node_id);
CREATE INDEX IF NOT EXISTS idx_graph_history_ts ON graph_history(ts);

CREATE TABLE IF NOT EXISTS mail_identities (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    message_id TEXT NOT NULL UNIQUE
);
-- Outlook's local rowids are aliases, not portable message identities.
CREATE TABLE IF NOT EXISTS mail_identity_sources (
    source_id INTEGER PRIMARY KEY,
    identity_id INTEGER NOT NULL REFERENCES mail_identities(id)
);
CREATE INDEX IF NOT EXISTS idx_mail_identity_sources_identity
    ON mail_identity_sources(identity_id);
CREATE TABLE IF NOT EXISTS mail_labels (
    identity_id INTEGER PRIMARY KEY REFERENCES mail_identities(id),
    label_number INTEGER NOT NULL CHECK(label_number BETWEEN 1 AND 5),
    assigned_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS mail_bodies (
    identity_id INTEGER PRIMARY KEY REFERENCES mail_identities(id),
    body_text TEXT NOT NULL,
    body_format TEXT NOT NULL CHECK(body_format IN ('plain', 'markdown')),
    cached_to TEXT NOT NULL DEFAULT '',
    cached_cc TEXT NOT NULL DEFAULT '',
    cached_at TEXT NOT NULL
);

INSERT OR IGNORE INTO schema_migrations (version, applied_at)
VALUES (3, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'));

COMMIT;
