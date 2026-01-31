//! Database management for atomic-core

use crate::error::AtomicCoreError;
use rusqlite::ffi::sqlite3_auto_extension;
use rusqlite::Connection;
use sqlite_vec::sqlite3_vec_init;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Database handle with connection management
pub struct Database {
    pub conn: Mutex<Connection>,
    pub db_path: PathBuf,
}

impl Database {
    /// Open an existing database
    pub fn open(path: impl AsRef<Path>) -> Result<Self, AtomicCoreError> {
        Self::open_internal(path.as_ref(), false)
    }

    /// Open an existing database or create a new one
    pub fn open_or_create(path: impl AsRef<Path>) -> Result<Self, AtomicCoreError> {
        Self::open_internal(path.as_ref(), true)
    }

    fn open_internal(path: &Path, create: bool) -> Result<Self, AtomicCoreError> {
        // Register sqlite-vec extension
        unsafe {
            #[allow(clippy::missing_transmute_annotations)]
            sqlite3_auto_extension(Some(std::mem::transmute(sqlite3_vec_init as *const ())));
        }

        // Create parent directory if needed
        if create {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let conn = Connection::open(path)?;

        // Enable WAL mode for concurrent reads with single writer
        // busy_timeout prevents SQLITE_BUSY when another connection holds a write lock
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA busy_timeout=5000;")?;

        if create {
            Self::run_migrations(&conn)?;
        }

        Ok(Database {
            conn: Mutex::new(conn),
            db_path: path.to_path_buf(),
        })
    }

    /// Create a new connection to the same database.
    /// Registers sqlite-vec so the connection can query vec_chunks.
    pub fn new_connection(&self) -> Result<Connection, AtomicCoreError> {
        // sqlite-vec is registered via sqlite3_auto_extension in open_internal,
        // which applies to all connections opened after that call.
        let conn = Connection::open(&self.db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA busy_timeout=5000;")?;
        Ok(conn)
    }

    /// Run database migrations
    pub fn run_migrations(conn: &Connection) -> Result<(), AtomicCoreError> {
        conn.execute_batch(
            r#"
            -- Atoms are the core content units
            CREATE TABLE IF NOT EXISTS atoms (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                source_url TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                embedding_status TEXT DEFAULT 'pending',
                tagging_status TEXT DEFAULT 'pending'
            );

            -- Hierarchical tags
            CREATE TABLE IF NOT EXISTS tags (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL COLLATE NOCASE,
                parent_id TEXT REFERENCES tags(id) ON DELETE SET NULL,
                created_at TEXT NOT NULL,
                UNIQUE(name COLLATE NOCASE)
            );

            -- Many-to-many relationship
            CREATE TABLE IF NOT EXISTS atom_tags (
                atom_id TEXT REFERENCES atoms(id) ON DELETE CASCADE,
                tag_id TEXT REFERENCES tags(id) ON DELETE CASCADE,
                PRIMARY KEY (atom_id, tag_id)
            );

            -- For Phase 2 embeddings
            CREATE TABLE IF NOT EXISTS atom_chunks (
                id TEXT PRIMARY KEY,
                atom_id TEXT REFERENCES atoms(id) ON DELETE CASCADE,
                chunk_index INTEGER NOT NULL,
                content TEXT NOT NULL,
                embedding BLOB
            );

            -- Settings table for app configuration
            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            -- Wiki articles
            CREATE TABLE IF NOT EXISTS wiki_articles (
                id TEXT PRIMARY KEY,
                tag_id TEXT UNIQUE REFERENCES tags(id) ON DELETE CASCADE,
                content TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                atom_count INTEGER NOT NULL
            );

            -- Wiki citations
            CREATE TABLE IF NOT EXISTS wiki_citations (
                id TEXT PRIMARY KEY,
                wiki_article_id TEXT REFERENCES wiki_articles(id) ON DELETE CASCADE,
                citation_index INTEGER NOT NULL,
                atom_id TEXT REFERENCES atoms(id) ON DELETE CASCADE,
                chunk_index INTEGER,
                excerpt TEXT NOT NULL
            );

            -- Atom positions for canvas view
            CREATE TABLE IF NOT EXISTS atom_positions (
                atom_id TEXT PRIMARY KEY REFERENCES atoms(id) ON DELETE CASCADE,
                x REAL NOT NULL,
                y REAL NOT NULL,
                updated_at TEXT NOT NULL
            );

            -- Semantic edges for graph visualization
            CREATE TABLE IF NOT EXISTS semantic_edges (
                id TEXT PRIMARY KEY,
                source_atom_id TEXT NOT NULL REFERENCES atoms(id) ON DELETE CASCADE,
                target_atom_id TEXT NOT NULL REFERENCES atoms(id) ON DELETE CASCADE,
                similarity_score REAL NOT NULL,
                source_chunk_index INTEGER,
                target_chunk_index INTEGER,
                created_at TEXT NOT NULL,
                UNIQUE(source_atom_id, target_atom_id)
            );

            -- Atom cluster assignments
            CREATE TABLE IF NOT EXISTS atom_clusters (
                atom_id TEXT PRIMARY KEY REFERENCES atoms(id) ON DELETE CASCADE,
                cluster_id INTEGER NOT NULL,
                computed_at TEXT NOT NULL
            );
            "#,
        )?;

        // Create vec_chunks virtual table if it doesn't exist
        let has_vec_chunks: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='vec_chunks'",
                [],
                |_| Ok(true),
            )
            .unwrap_or(false);

        if !has_vec_chunks {
            // Default to 1536 dimensions (OpenAI text-embedding-3-small)
            conn.execute(
                "CREATE VIRTUAL TABLE vec_chunks USING vec0(chunk_id TEXT PRIMARY KEY, embedding float[1536])",
                [],
            )?;
        }

        // FTS5 table for keyword search (external content backed by atom_chunks).
        // Must declare all atom_chunks columns so positional mapping is correct:
        //   FTS5 col 0 (id)          -> atom_chunks col 0 (id)
        //   FTS5 col 1 (atom_id)     -> atom_chunks col 1 (atom_id)
        //   FTS5 col 2 (chunk_index) -> atom_chunks col 2 (chunk_index)
        //   FTS5 col 3 (content)     -> atom_chunks col 3 (content)
        //
        // Migration: d9e8f91 introduced a broken 2-column schema (chunk_id, content)
        // that caused incorrect positional mapping. Detect and fix it.
        let fts_sql: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name='atom_chunks_fts'",
                [],
                |row| row.get(0),
            )
            .unwrap_or_default();

        if fts_sql.is_empty() {
            // Table doesn't exist — create it
            conn.execute_batch(
                r#"
                CREATE VIRTUAL TABLE atom_chunks_fts USING fts5(
                    id,
                    atom_id,
                    chunk_index,
                    content,
                    content='atom_chunks',
                    content_rowid='rowid'
                );
                "#,
            )?;
        } else if !fts_sql.contains("atom_id") {
            // Old 2-column schema — drop and recreate with correct columns
            conn.execute_batch("DROP TABLE IF EXISTS atom_chunks_fts")?;
            conn.execute_batch(
                r#"
                CREATE VIRTUAL TABLE atom_chunks_fts USING fts5(
                    id,
                    atom_id,
                    chunk_index,
                    content,
                    content='atom_chunks',
                    content_rowid='rowid'
                );
                "#,
            )?;
            // Rebuild FTS index from existing atom_chunks data
            conn.execute(
                "INSERT INTO atom_chunks_fts(atom_chunks_fts) VALUES('rebuild')",
                [],
            )?;
        }

        // Migrate settings
        crate::settings::migrate_settings(conn)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_create_database() {
        let temp_file = NamedTempFile::new().unwrap();
        let db = Database::open_or_create(temp_file.path()).unwrap();

        // Verify we got a valid database
        let conn = db.conn.lock().unwrap();
        let count: i32 = conn
            .query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table'", [], |row| row.get(0))
            .unwrap();

        // Should have at least our core tables
        assert!(count >= 10, "Expected at least 10 tables, got {}", count);
    }

    #[test]
    fn test_tables_created() {
        let temp_file = NamedTempFile::new().unwrap();
        let db = Database::open_or_create(temp_file.path()).unwrap();
        let conn = db.conn.lock().unwrap();

        let expected_tables = vec![
            "atoms",
            "tags",
            "atom_tags",
            "atom_chunks",
            "settings",
            "wiki_articles",
            "wiki_citations",
            "atom_positions",
            "semantic_edges",
            "atom_clusters",
        ];

        for table in expected_tables {
            let exists: bool = conn
                .query_row(
                    "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |_| Ok(true),
                )
                .unwrap_or(false);
            assert!(exists, "Table '{}' should exist", table);
        }
    }

    #[test]
    fn test_vec_chunks_virtual_table() {
        let temp_file = NamedTempFile::new().unwrap();
        let db = Database::open_or_create(temp_file.path()).unwrap();
        let conn = db.conn.lock().unwrap();

        // Verify vec_chunks virtual table exists
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='vec_chunks'",
                [],
                |_| Ok(true),
            )
            .unwrap_or(false);
        assert!(exists, "vec_chunks virtual table should exist");
    }

    #[test]
    fn test_fts_virtual_table() {
        let temp_file = NamedTempFile::new().unwrap();
        let db = Database::open_or_create(temp_file.path()).unwrap();
        let conn = db.conn.lock().unwrap();

        // Verify atom_chunks_fts virtual table exists
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='atom_chunks_fts'",
                [],
                |_| Ok(true),
            )
            .unwrap_or(false);
        assert!(exists, "atom_chunks_fts FTS5 table should exist");
    }

    #[test]
    fn test_new_connection() {
        let temp_file = NamedTempFile::new().unwrap();
        let db = Database::open_or_create(temp_file.path()).unwrap();

        // Create a new connection - should work without errors
        let conn2 = db.new_connection().unwrap();

        // Verify we can query the new connection
        let count: i32 = conn2
            .query_row("SELECT COUNT(*) FROM atoms", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0, "New database should have 0 atoms");
    }
}
