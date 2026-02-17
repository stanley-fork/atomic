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

            -- Chat conversations
            CREATE TABLE IF NOT EXISTS conversations (
                id TEXT PRIMARY KEY,
                title TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                is_archived INTEGER DEFAULT 0
            );

            -- Many-to-many: conversation tag scope
            CREATE TABLE IF NOT EXISTS conversation_tags (
                conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
                tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
                PRIMARY KEY (conversation_id, tag_id)
            );

            -- Chat messages
            CREATE TABLE IF NOT EXISTS chat_messages (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                created_at TEXT NOT NULL,
                message_index INTEGER NOT NULL
            );

            -- Tool calls for transparency
            CREATE TABLE IF NOT EXISTS chat_tool_calls (
                id TEXT PRIMARY KEY,
                message_id TEXT NOT NULL REFERENCES chat_messages(id) ON DELETE CASCADE,
                tool_name TEXT NOT NULL,
                tool_input TEXT NOT NULL,
                tool_output TEXT,
                status TEXT NOT NULL DEFAULT 'pending',
                created_at TEXT NOT NULL,
                completed_at TEXT
            );

            -- Chat citations
            CREATE TABLE IF NOT EXISTS chat_citations (
                id TEXT PRIMARY KEY,
                message_id TEXT NOT NULL REFERENCES chat_messages(id) ON DELETE CASCADE,
                citation_index INTEGER NOT NULL,
                atom_id TEXT NOT NULL REFERENCES atoms(id) ON DELETE CASCADE,
                chunk_index INTEGER,
                excerpt TEXT NOT NULL,
                relevance_score REAL
            );

            -- Core table indexes
            CREATE INDEX IF NOT EXISTS idx_atoms_updated_at ON atoms(updated_at DESC);
            CREATE INDEX IF NOT EXISTS idx_atoms_embedding_status ON atoms(embedding_status);
            CREATE INDEX IF NOT EXISTS idx_atoms_tagging_status ON atoms(tagging_status);
            CREATE INDEX IF NOT EXISTS idx_atom_tags_tag_atom ON atom_tags(tag_id, atom_id);
            CREATE INDEX IF NOT EXISTS idx_atom_tags_atom_id ON atom_tags(atom_id);
            CREATE INDEX IF NOT EXISTS idx_atom_chunks_atom_id ON atom_chunks(atom_id);
            CREATE INDEX IF NOT EXISTS idx_semantic_edges_source ON semantic_edges(source_atom_id);
            CREATE INDEX IF NOT EXISTS idx_semantic_edges_target ON semantic_edges(target_atom_id);
            CREATE INDEX IF NOT EXISTS idx_semantic_edges_similarity ON semantic_edges(similarity_score DESC);
            CREATE INDEX IF NOT EXISTS idx_tags_parent_id ON tags(parent_id);
            CREATE INDEX IF NOT EXISTS idx_wiki_citations_article ON wiki_citations(wiki_article_id);

            -- Indexes for chat tables
            CREATE INDEX IF NOT EXISTS idx_conversations_updated ON conversations(updated_at DESC);
            CREATE INDEX IF NOT EXISTS idx_conversation_tags_conv ON conversation_tags(conversation_id);
            CREATE INDEX IF NOT EXISTS idx_conversation_tags_tag ON conversation_tags(tag_id);
            CREATE INDEX IF NOT EXISTS idx_chat_messages_conversation ON chat_messages(conversation_id, message_index);
            CREATE INDEX IF NOT EXISTS idx_chat_tool_calls_message ON chat_tool_calls(message_id);
            CREATE INDEX IF NOT EXISTS idx_chat_citations_message ON chat_citations(message_id);
            CREATE INDEX IF NOT EXISTS idx_chat_citations_atom ON chat_citations(atom_id);

            -- API tokens for authentication
            CREATE TABLE IF NOT EXISTS api_tokens (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                token_hash TEXT NOT NULL,
                token_prefix TEXT NOT NULL,
                created_at TEXT NOT NULL,
                last_used_at TEXT,
                is_revoked INTEGER DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_api_tokens_hash ON api_tokens(token_hash);

            -- OAuth 2.0 tables (for MCP remote auth)
            CREATE TABLE IF NOT EXISTS oauth_clients (
                id TEXT PRIMARY KEY,
                client_id TEXT UNIQUE NOT NULL,
                client_secret_hash TEXT NOT NULL,
                client_name TEXT NOT NULL,
                redirect_uris TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS oauth_codes (
                code_hash TEXT PRIMARY KEY,
                client_id TEXT NOT NULL,
                code_challenge TEXT NOT NULL,
                code_challenge_method TEXT NOT NULL DEFAULT 'S256',
                redirect_uri TEXT NOT NULL,
                created_at TEXT NOT NULL,
                expires_at TEXT NOT NULL,
                used INTEGER NOT NULL DEFAULT 0,
                token_id TEXT
            );

            -- Wiki inter-article links (cross-references between wiki articles)
            CREATE TABLE IF NOT EXISTS wiki_links (
                id TEXT PRIMARY KEY,
                source_article_id TEXT NOT NULL REFERENCES wiki_articles(id) ON DELETE CASCADE,
                target_tag_name TEXT NOT NULL COLLATE NOCASE,
                target_tag_id TEXT REFERENCES tags(id) ON DELETE SET NULL,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_wiki_links_source ON wiki_links(source_article_id);
            CREATE INDEX IF NOT EXISTS idx_wiki_links_target_tag ON wiki_links(target_tag_id);

            -- Tag-level centroid embeddings (average of atom chunk embeddings)
            CREATE TABLE IF NOT EXISTS tag_embeddings (
                tag_id TEXT PRIMARY KEY REFERENCES tags(id) ON DELETE CASCADE,
                embedding BLOB NOT NULL,
                atom_count INTEGER NOT NULL,
                updated_at TEXT NOT NULL
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

        // Create vec_tags virtual table for tag centroid similarity search.
        // Must match vec_chunks dimension — extract it from vec_chunks CREATE statement.
        let vec_chunks_dim: usize = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name='vec_chunks'",
                [],
                |row| row.get::<_, String>(0),
            )
            .ok()
            .and_then(|sql| {
                // Parse "float[N]" from the CREATE statement
                let start = sql.find("float[")?;
                let after = &sql[start + 6..];
                let end = after.find(']')?;
                after[..end].parse::<usize>().ok()
            })
            .unwrap_or(1536);

        // Check if vec_tags exists and has the right dimension
        let vec_tags_sql: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name='vec_tags'",
                [],
                |row| row.get(0),
            )
            .unwrap_or_default();

        let vec_tags_correct = !vec_tags_sql.is_empty()
            && vec_tags_sql.contains(&format!("float[{}]", vec_chunks_dim));

        if !vec_tags_correct {
            conn.execute("DROP TABLE IF EXISTS vec_tags", []).ok();
            conn.execute("DELETE FROM tag_embeddings", []).ok();
            let create_sql = format!(
                "CREATE VIRTUAL TABLE vec_tags USING vec0(tag_id TEXT PRIMARY KEY, embedding float[{}])",
                vec_chunks_dim
            );
            conn.execute(&create_sql, [])?;
        }

        // FTS5 table for keyword search (external content backed by atom_chunks).
        // Column names MUST match atom_chunks columns (FTS5 uses names during rebuild).
        // Positional mapping to atom_chunks for external content reads:
        //   FTS5 col 0 (id)          -> atom_chunks col 0 (id)
        //   FTS5 col 1 (atom_id)     -> atom_chunks col 1 (atom_id)
        //   FTS5 col 2 (chunk_index) -> atom_chunks col 2 (chunk_index)
        //   FTS5 col 3 (content)     -> atom_chunks col 3 (content)
        //
        // Correct schema requires: content='atom_chunks' AND all 4 column names.
        // Recreate if missing, standalone, or has wrong column names.
        let fts_sql: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name='atom_chunks_fts'",
                [],
                |row| row.get(0),
            )
            .unwrap_or_default();

        let has_correct_fts = fts_sql.contains("content='atom_chunks'")
            && fts_sql.contains("atom_id")
            && fts_sql.contains("chunk_index");

        if !has_correct_fts {
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

// ==================== Dimension Change Helpers ====================

/// Get embedding dimension based on current settings
pub fn get_current_embedding_dimension(conn: &Connection) -> usize {
    use crate::providers::ProviderConfig;

    let settings_map = crate::settings::get_all_settings(conn).unwrap_or_default();
    let config = ProviderConfig::from_settings(&settings_map);
    config.embedding_dimension()
}

/// Check if dimension will change with new settings
pub fn will_dimension_change(conn: &Connection, key: &str, new_value: &str) -> (bool, usize) {
    use crate::providers::ProviderConfig;

    let current_dim = get_current_embedding_dimension(conn);

    // Get current settings and apply the change
    let mut settings_map = crate::settings::get_all_settings(conn).unwrap_or_default();
    settings_map.insert(key.to_string(), new_value.to_string());

    let new_config = ProviderConfig::from_settings(&settings_map);
    let new_dim = new_config.embedding_dimension();

    (current_dim != new_dim, new_dim)
}

/// Recreate vec_chunks table with a new dimension and reset embedding status
pub fn recreate_vec_chunks_with_dimension(
    conn: &Connection,
    dimension: usize,
) -> Result<(), AtomicCoreError> {
    conn.execute("DROP TABLE IF EXISTS vec_chunks", [])?;

    let create_sql = format!(
        "CREATE VIRTUAL TABLE vec_chunks USING vec0(chunk_id TEXT PRIMARY KEY, embedding float[{}])",
        dimension
    );
    conn.execute(&create_sql, [])?;

    // Reset ONLY embedding status to pending
    conn.execute(
        "UPDATE atoms SET embedding_status = 'pending'",
        [],
    )?;

    // Set tagging_status to 'skipped' - existing tags are preserved
    conn.execute(
        "UPDATE atoms SET tagging_status = 'skipped'",
        [],
    )?;

    // Clear all existing chunk data
    conn.execute("DELETE FROM atom_chunks", [])?;

    // Clear FTS5 table
    conn.execute("DELETE FROM atom_chunks_fts", [])?;

    // Clear semantic edges
    conn.execute("DELETE FROM semantic_edges", [])?;

    // Clear canvas positions
    conn.execute("DELETE FROM atom_positions", [])?;

    // Clear tag embeddings and recreate vec_tags with new dimension
    conn.execute("DELETE FROM tag_embeddings", []).ok();
    conn.execute("DROP TABLE IF EXISTS vec_tags", [])?;
    let vec_tags_sql = format!(
        "CREATE VIRTUAL TABLE vec_tags USING vec0(tag_id TEXT PRIMARY KEY, embedding float[{}])",
        dimension
    );
    conn.execute(&vec_tags_sql, [])?;

    Ok(())
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

        // Should have at least our core tables (16 regular + 2 virtual)
        assert!(count >= 16, "Expected at least 16 tables, got {}", count);
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
            "conversations",
            "conversation_tags",
            "chat_messages",
            "chat_tool_calls",
            "chat_citations",
            "api_tokens",
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
