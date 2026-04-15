-- Daily briefings (per-database, scoped by db_id).
--
-- Mirrors the SQLite schema in db.rs:648-674 plus a db_id column for
-- multi-database isolation in shared Postgres deployments.

CREATE TABLE IF NOT EXISTS briefings (
    id TEXT PRIMARY KEY,
    content TEXT NOT NULL,
    created_at TEXT NOT NULL,
    atom_count INTEGER NOT NULL,
    last_run_at TEXT NOT NULL,
    db_id TEXT NOT NULL DEFAULT 'default'
);

CREATE INDEX IF NOT EXISTS idx_briefings_db_created
    ON briefings(db_id, created_at DESC);

CREATE TABLE IF NOT EXISTS briefing_citations (
    id TEXT PRIMARY KEY,
    briefing_id TEXT NOT NULL REFERENCES briefings(id) ON DELETE CASCADE,
    citation_index INTEGER NOT NULL,
    atom_id TEXT NOT NULL REFERENCES atoms(id) ON DELETE CASCADE,
    excerpt TEXT NOT NULL,
    db_id TEXT NOT NULL DEFAULT 'default'
);

CREATE INDEX IF NOT EXISTS idx_briefing_citations_briefing
    ON briefing_citations(briefing_id);

INSERT INTO schema_version (version) VALUES (7);
