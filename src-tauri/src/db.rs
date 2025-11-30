use rusqlite::ffi::sqlite3_auto_extension;
use rusqlite::Connection;
use sqlite_vec::sqlite3_vec_init;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub struct Database {
    pub conn: Mutex<Connection>,
    pub db_path: PathBuf,
    pub resource_dir: PathBuf, // Kept for potential future use
}

/// Thread-safe wrapper around Database using Arc
pub type SharedDatabase = Arc<Database>;

impl Database {
    pub fn new(app_data_dir: PathBuf, resource_dir: PathBuf) -> Result<Self, String> {
        // Register sqlite-vec extension
        unsafe {
            #[allow(clippy::missing_transmute_annotations)]
            sqlite3_auto_extension(Some(std::mem::transmute(sqlite3_vec_init as *const ())));
        }

        // Create database directory if it doesn't exist
        std::fs::create_dir_all(&app_data_dir)
            .map_err(|e| format!("Failed to create app data directory: {}", e))?;

        let db_path = app_data_dir.join("atomic.db");
        let conn = Connection::open(&db_path)
            .map_err(|e| format!("Failed to open database: {}", e))?;

        // Run migrations
        Self::run_migrations(&conn)?;

        Ok(Database {
            conn: Mutex::new(conn),
            db_path,
            resource_dir,
        })
    }

    /// Create a new connection to the same database
    /// This is useful for background tasks that need their own connection
    pub fn new_connection(&self) -> Result<Connection, String> {
        let conn = Connection::open(&self.db_path)
            .map_err(|e| format!("Failed to open database connection: {}", e))?;

        Ok(conn)
    }

    fn run_migrations(conn: &Connection) -> Result<(), String> {
        conn.execute_batch(
            r#"
            -- Atoms are the core content units
            CREATE TABLE IF NOT EXISTS atoms (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                source_url TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                embedding_status TEXT DEFAULT 'pending'
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

            -- Indexes
            CREATE INDEX IF NOT EXISTS idx_atom_chunks_atom_id ON atom_chunks(atom_id);
            CREATE INDEX IF NOT EXISTS idx_atom_tags_atom_id ON atom_tags(atom_id);
            CREATE INDEX IF NOT EXISTS idx_atom_tags_tag_id ON atom_tags(tag_id);
            CREATE INDEX IF NOT EXISTS idx_tags_parent_id ON tags(parent_id);
            CREATE INDEX IF NOT EXISTS idx_tags_name_nocase ON tags(name COLLATE NOCASE);
            CREATE INDEX IF NOT EXISTS idx_atoms_updated_at ON atoms(updated_at);
            CREATE INDEX IF NOT EXISTS idx_atom_chunks_composite ON atom_chunks(atom_id, chunk_index);

            -- Wiki articles for tags
            CREATE TABLE IF NOT EXISTS wiki_articles (
              id TEXT PRIMARY KEY,
              tag_id TEXT UNIQUE REFERENCES tags(id) ON DELETE CASCADE,
              content TEXT NOT NULL,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL,
              atom_count INTEGER NOT NULL
            );

            -- Citations linking article content to source atoms/chunks
            CREATE TABLE IF NOT EXISTS wiki_citations (
              id TEXT PRIMARY KEY,
              wiki_article_id TEXT REFERENCES wiki_articles(id) ON DELETE CASCADE,
              citation_index INTEGER NOT NULL,
              atom_id TEXT REFERENCES atoms(id) ON DELETE CASCADE,
              chunk_index INTEGER,
              excerpt TEXT NOT NULL
            );

            -- Indexes for wiki tables
            CREATE INDEX IF NOT EXISTS idx_wiki_articles_tag ON wiki_articles(tag_id);
            CREATE INDEX IF NOT EXISTS idx_wiki_citations_article ON wiki_citations(wiki_article_id);
            CREATE INDEX IF NOT EXISTS idx_wiki_citations_atom ON wiki_citations(atom_id);

            -- Atom positions for canvas view
            CREATE TABLE IF NOT EXISTS atom_positions (
              atom_id TEXT PRIMARY KEY REFERENCES atoms(id) ON DELETE CASCADE,
              x REAL NOT NULL,
              y REAL NOT NULL,
              updated_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_atom_positions_atom ON atom_positions(atom_id);

            -- Chat conversations
            CREATE TABLE IF NOT EXISTS conversations (
                id TEXT PRIMARY KEY,
                title TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                is_archived INTEGER DEFAULT 0
            );

            -- Many-to-many: conversation tag scope (editable at any time)
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

            -- Tool calls for transparency and debugging
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

            -- Chat citations (mirrors wiki_citations pattern)
            CREATE TABLE IF NOT EXISTS chat_citations (
                id TEXT PRIMARY KEY,
                message_id TEXT NOT NULL REFERENCES chat_messages(id) ON DELETE CASCADE,
                citation_index INTEGER NOT NULL,
                atom_id TEXT NOT NULL REFERENCES atoms(id) ON DELETE CASCADE,
                chunk_index INTEGER,
                excerpt TEXT NOT NULL,
                relevance_score REAL
            );

            -- Indexes for chat tables
            CREATE INDEX IF NOT EXISTS idx_conversations_updated ON conversations(updated_at DESC);
            CREATE INDEX IF NOT EXISTS idx_conversation_tags_conv ON conversation_tags(conversation_id);
            CREATE INDEX IF NOT EXISTS idx_conversation_tags_tag ON conversation_tags(tag_id);
            CREATE INDEX IF NOT EXISTS idx_chat_messages_conversation ON chat_messages(conversation_id, message_index);
            CREATE INDEX IF NOT EXISTS idx_chat_tool_calls_message ON chat_tool_calls(message_id);
            CREATE INDEX IF NOT EXISTS idx_chat_citations_message ON chat_citations(message_id);
            CREATE INDEX IF NOT EXISTS idx_chat_citations_atom ON chat_citations(atom_id);

            -- Semantic edges for graph visualization (pre-computed during embedding)
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

            CREATE INDEX IF NOT EXISTS idx_semantic_edges_source ON semantic_edges(source_atom_id);
            CREATE INDEX IF NOT EXISTS idx_semantic_edges_target ON semantic_edges(target_atom_id);
            CREATE INDEX IF NOT EXISTS idx_semantic_edges_score ON semantic_edges(similarity_score DESC);

            -- Atom cluster assignments for visual grouping
            CREATE TABLE IF NOT EXISTS atom_clusters (
                atom_id TEXT PRIMARY KEY REFERENCES atoms(id) ON DELETE CASCADE,
                cluster_id INTEGER NOT NULL,
                computed_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_atom_clusters_cluster ON atom_clusters(cluster_id);
            "#,
        )
        .map_err(|e| format!("Failed to run migrations: {}", e))?;

        // Add embedding_status column to atoms table if it doesn't exist
        Self::add_embedding_status_column(conn)?;

        // Create vec_chunks virtual table for sqlite-vec similarity search
        Self::create_vec_chunks_table(conn)?;

        // Insert default top-level tags if they don't exist
        Self::insert_default_tags(conn)?;

        Ok(())
    }

    fn add_embedding_status_column(conn: &Connection) -> Result<(), String> {
        // Check if embedding_status column exists
        let column_exists: bool = conn
            .prepare("SELECT 1 FROM pragma_table_info('atoms') WHERE name = 'embedding_status'")
            .map_err(|e| format!("Failed to prepare column check: {}", e))?
            .exists([])
            .map_err(|e| format!("Failed to check column existence: {}", e))?;

        if !column_exists {
            conn.execute(
                "ALTER TABLE atoms ADD COLUMN embedding_status TEXT DEFAULT 'pending'",
                [],
            )
            .map_err(|e| format!("Failed to add embedding_status column: {}", e))?;
        }

        // Create index for embedding_status (safe to run even if it exists)
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_atoms_embedding_status ON atoms(embedding_status)",
            [],
        )
        .map_err(|e| format!("Failed to create embedding_status index: {}", e))?;

        Ok(())
    }

    fn create_vec_chunks_table(conn: &Connection) -> Result<(), String> {
        // Create vec_chunks virtual table for sqlite-vec similarity search
        // This uses the vec0 module from sqlite-vec for vector similarity
        // Using 1536 dimensions for OpenRouter's text-embedding-3-small model
        conn.execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS vec_chunks USING vec0(
                chunk_id TEXT PRIMARY KEY,
                embedding float[1536]
            )",
            [],
        )
        .map_err(|e| format!("Failed to create vec_chunks table: {}", e))?;

        Ok(())
    }

    fn insert_default_tags(conn: &Connection) -> Result<(), String> {
        // Default top-level tags to guide the LLM when creating new tags
        let default_tags = vec!["People", "Concepts", "Places", "Organizations"];
        let now = chrono::Utc::now().to_rfc3339();

        for tag_name in default_tags {
            // Check if tag already exists
            let exists: bool = conn
                .prepare("SELECT 1 FROM tags WHERE name = ?1 COLLATE NOCASE")
                .map_err(|e| format!("Failed to prepare tag check: {}", e))?
                .exists([tag_name])
                .map_err(|e| format!("Failed to check tag existence: {}", e))?;

            if !exists {
                // Generate a simple UUID-like ID for the tag
                let tag_id = uuid::Uuid::new_v4().to_string();

                conn.execute(
                    "INSERT INTO tags (id, name, parent_id, created_at) VALUES (?1, ?2, NULL, ?3)",
                    [&tag_id, tag_name, &now],
                )
                .map_err(|e| format!("Failed to insert default tag '{}': {}", tag_name, e))?;
            }
        }

        Ok(())
    }
}

/// Get the embedding dimension for a given model
pub fn get_embedding_dimension(model: &str) -> usize {
    match model {
        "openai/text-embedding-3-small" => 1536,
        "openai/text-embedding-3-large" => 3072,
        _ => 1536, // Default to small model dimension
    }
}

/// Recreate vec_chunks table with a new dimension and reset all atom embedding status
pub fn recreate_vec_chunks_with_dimension(conn: &Connection, dimension: usize) -> Result<(), String> {
    // Drop existing vec_chunks table
    conn.execute("DROP TABLE IF EXISTS vec_chunks", [])
        .map_err(|e| format!("Failed to drop vec_chunks table: {}", e))?;

    // Create new vec_chunks table with the specified dimension
    let create_sql = format!(
        "CREATE VIRTUAL TABLE vec_chunks USING vec0(chunk_id TEXT PRIMARY KEY, embedding float[{}])",
        dimension
    );
    conn.execute(&create_sql, [])
        .map_err(|e| format!("Failed to create vec_chunks table: {}", e))?;

    // Reset all atom embedding status to pending so they get re-embedded
    conn.execute("UPDATE atoms SET embedding_status = 'pending'", [])
        .map_err(|e| format!("Failed to reset atom embedding status: {}", e))?;

    // Clear all existing chunk data since it's invalid with new dimensions
    conn.execute("DELETE FROM atom_chunks", [])
        .map_err(|e| format!("Failed to clear atom_chunks: {}", e))?;

    Ok(())
}
