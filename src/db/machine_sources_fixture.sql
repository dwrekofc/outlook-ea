BEGIN IMMEDIATE;
-- Keep imported rowid provenance intact; live aliases belong to a specific Mac.
CREATE TABLE IF NOT EXISTS mail_machine_identity_sources (
    machine_id TEXT NOT NULL,
    source_id INTEGER NOT NULL,
    identity_id INTEGER NOT NULL REFERENCES mail_identities(id),
    PRIMARY KEY (machine_id, source_id)
);
CREATE INDEX IF NOT EXISTS idx_mail_machine_sources_identity
    ON mail_machine_identity_sources(identity_id);
INSERT OR IGNORE INTO schema_migrations(version, applied_at)
VALUES (4, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'));
COMMIT;
