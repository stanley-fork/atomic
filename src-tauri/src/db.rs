use rusqlite::ffi::sqlite3_auto_extension;
use rusqlite::Connection;
use sqlite_vec::sqlite3_vec_init;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub struct Database {
    pub conn: Mutex<Connection>,
    pub db_path: PathBuf,
    pub resource_dir: PathBuf,
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

        // Enable extension loading and load sqlite-lembed
        Self::load_lembed_extension(&conn, &resource_dir)?;

        // Run migrations
        Self::run_migrations(&conn)?;

        // Register the embedding model
        Self::register_embedding_model(&conn, &resource_dir)?;

        Ok(Database {
            conn: Mutex::new(conn),
            db_path,
            resource_dir,
        })
    }

    /// Load the sqlite-lembed extension into a connection
    fn load_lembed_extension(conn: &Connection, resource_dir: &PathBuf) -> Result<(), String> {
        // Enable extension loading and load sqlite-lembed extension
        // Both operations are unsafe as they involve loading external code
        unsafe {
            conn.load_extension_enable()
                .map_err(|e| format!("Failed to enable extension loading: {}", e))?;

            // Determine the extension filename based on OS and architecture
            let extension_filename = Self::get_lembed_extension_filename();
            let lembed_path = resource_dir.join(&extension_filename);

            // For load_extension, we need to strip the extension as SQLite adds it automatically
            // But since we have architecture-specific names, we'll use the full path
            // and strip just the platform extension (.so, .dylib, .dll)
            let lembed_path_str = lembed_path.to_str()
                .ok_or("Invalid lembed path")?;

            // Strip the extension (.so, .dylib, .dll) from the path
            let lembed_path_without_ext = if lembed_path_str.ends_with(".so") {
                &lembed_path_str[..lembed_path_str.len() - 3]
            } else if lembed_path_str.ends_with(".dylib") {
                &lembed_path_str[..lembed_path_str.len() - 6]
            } else if lembed_path_str.ends_with(".dll") {
                &lembed_path_str[..lembed_path_str.len() - 4]
            } else {
                lembed_path_str
            };

            // Specify the entry point explicitly since we use architecture-specific filenames
            conn.load_extension(lembed_path_without_ext, Some("sqlite3_lembed_init"))
                .map_err(|e| format!("Failed to load sqlite-lembed extension from {}: {}", lembed_path_str, e))?;
        }

        Ok(())
    }

    /// Get the platform-specific extension filename for sqlite-lembed
    fn get_lembed_extension_filename() -> String {
        #[cfg(target_os = "linux")]
        {
            "lembed0.so".to_string()
        }
        
        #[cfg(target_os = "macos")]
        {
            // On macOS, use architecture-specific binaries
            match std::env::consts::ARCH {
                "aarch64" => "lembed0-aarch64.dylib".to_string(),
                "x86_64" => "lembed0-x86_64.dylib".to_string(),
                arch => panic!("Unsupported macOS architecture: {}", arch),
            }
        }
        
        #[cfg(target_os = "windows")]
        {
            "lembed0.dll".to_string()
        }
        
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            compile_error!("Unsupported operating system");
        }
    }

    /// Register the embedding model for a connection
    /// The model is registered in temp.lembed_models which is a temporary table,
    /// so it needs to be re-registered for each new connection
    fn register_embedding_model(conn: &Connection, resource_dir: &PathBuf) -> Result<(), String> {
        let model_path = resource_dir.join("all-MiniLM-L6-v2.q8_0.gguf");
        let model_path_str = model_path
            .to_str()
            .ok_or("Invalid model path")?;

        conn.execute(
            "INSERT INTO temp.lembed_models(name, model) SELECT 'all-MiniLM-L6-v2', lembed_model_from_file(?1)",
            [model_path_str],
        )
        .map_err(|e| format!("Failed to register embedding model: {}", e))?;

        Ok(())
    }

    /// Create a new connection to the same database
    /// This is useful for background tasks that need their own connection
    /// The connection will have sqlite-lembed loaded and the model registered
    pub fn new_connection(&self) -> Result<Connection, String> {
        let conn = Connection::open(&self.db_path)
            .map_err(|e| format!("Failed to open database connection: {}", e))?;

        // Load sqlite-lembed extension and register model for this connection
        Self::load_lembed_extension(&conn, &self.resource_dir)?;
        Self::register_embedding_model(&conn, &self.resource_dir)?;

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
                updated_at TEXT NOT NULL
            );

            -- Hierarchical tags
            CREATE TABLE IF NOT EXISTS tags (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                parent_id TEXT REFERENCES tags(id) ON DELETE SET NULL,
                created_at TEXT NOT NULL
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
            "#,
        )
        .map_err(|e| format!("Failed to run migrations: {}", e))?;

        // Add embedding_status column to atoms table if it doesn't exist
        Self::add_embedding_status_column(conn)?;

        // Create vec_chunks virtual table for sqlite-vec similarity search
        Self::create_vec_chunks_table(conn)?;

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

        Ok(())
    }

    fn create_vec_chunks_table(conn: &Connection) -> Result<(), String> {
        // Create vec_chunks virtual table for sqlite-vec similarity search
        // This uses the vec0 module from sqlite-vec for vector similarity
        conn.execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS vec_chunks USING vec0(
                chunk_id TEXT PRIMARY KEY,
                embedding float[384]
            )",
            [],
        )
        .map_err(|e| format!("Failed to create vec_chunks table: {}", e))?;

        Ok(())
    }
}

