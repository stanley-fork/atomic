//! Settings management for atomic-core
//!
//! This module provides key-value storage for application configuration.

use crate::error::AtomicCoreError;
use rusqlite::Connection;
use std::collections::HashMap;

/// Default Ollama host URL
pub const DEFAULT_OLLAMA_HOST: &str = "http://127.0.0.1:11434";

/// Default settings with their values
pub const DEFAULT_SETTINGS: &[(&str, &str)] = &[
    ("provider", "openrouter"),
    ("ollama_host", DEFAULT_OLLAMA_HOST),
    ("ollama_embedding_model", "nomic-embed-text"),
    ("ollama_llm_model", "llama3.2"),
    ("ollama_context_length", "65536"),
    ("embedding_model", "openai/text-embedding-3-small"),
    ("tagging_model", "openai/gpt-4o-mini"),
    ("wiki_model", "anthropic/claude-sonnet-4.5"),
    ("wiki_strategy", "centroid"),
    ("chat_model", "anthropic/claude-sonnet-4.5"),
    ("auto_tagging_enabled", "true"),
    ("openai_compat_base_url", ""),
    ("openai_compat_embedding_model", ""),
    ("openai_compat_llm_model", ""),
    ("openai_compat_embedding_dimension", "1536"),
    ("openai_compat_context_length", "65536"),
];

/// Migrate settings - add any missing default settings
pub fn migrate_settings(conn: &Connection) -> Result<(), AtomicCoreError> {
    for (key, default_value) in DEFAULT_SETTINGS {
        // Only set if the key doesn't exist
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM settings WHERE key = ?1",
                [key],
                |_| Ok(true),
            )
            .unwrap_or(false);

        if !exists {
            set_setting(conn, key, default_value)?;
        }
    }
    Ok(())
}

/// Get a setting with a default fallback
pub fn get_setting_or_default(conn: &Connection, key: &str) -> String {
    get_setting(conn, key).unwrap_or_else(|_| {
        DEFAULT_SETTINGS
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v.to_string())
            .unwrap_or_default()
    })
}

/// Get all settings as a HashMap
pub fn get_all_settings(conn: &Connection) -> Result<HashMap<String, String>, AtomicCoreError> {
    let mut stmt = conn
        .prepare("SELECT key, value FROM settings")?;

    let settings = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<HashMap<_, _>, _>>()?;

    Ok(settings)
}

/// Get a single setting by key
pub fn get_setting(conn: &Connection, key: &str) -> Result<String, AtomicCoreError> {
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        [key],
        |row| row.get(0),
    )
    .map_err(|e| AtomicCoreError::Configuration(format!("Failed to get setting '{}': {}", key, e)))
}

/// Migrate settings into a connection that has a `settings` table.
/// Used by the registry to seed defaults into registry.db.
pub fn migrate_settings_to(conn: &Connection) -> Result<(), AtomicCoreError> {
    migrate_settings(conn)
}

/// Set a setting (upsert)
pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<(), AtomicCoreError> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [key, value],
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use tempfile::NamedTempFile;

    fn create_test_db() -> (Database, NamedTempFile) {
        let temp_file = NamedTempFile::new().unwrap();
        let db = Database::open_or_create(temp_file.path()).unwrap();
        (db, temp_file)
    }

    #[test]
    fn test_get_all_settings_has_defaults() {
        let (db, _temp) = create_test_db();
        let conn = db.conn.lock().unwrap();

        let settings = get_all_settings(&conn).unwrap();

        // After migration, should have default settings
        assert!(!settings.is_empty(), "Should have default settings after migration");
        assert!(settings.contains_key("provider"), "Should have provider setting");
        assert_eq!(settings.get("provider").unwrap(), "openrouter");
    }

    #[test]
    fn test_set_and_get_setting() {
        let (db, _temp) = create_test_db();
        let conn = db.conn.lock().unwrap();

        // Set a custom setting
        set_setting(&conn, "my_custom_key", "my_custom_value").unwrap();

        // Get it back
        let value = get_setting(&conn, "my_custom_key").unwrap();
        assert_eq!(value, "my_custom_value");
    }

    #[test]
    fn test_update_existing_setting() {
        let (db, _temp) = create_test_db();
        let conn = db.conn.lock().unwrap();

        // Set initial value
        set_setting(&conn, "test_key", "initial_value").unwrap();
        let value1 = get_setting(&conn, "test_key").unwrap();
        assert_eq!(value1, "initial_value");

        // Update to new value (upsert)
        set_setting(&conn, "test_key", "updated_value").unwrap();
        let value2 = get_setting(&conn, "test_key").unwrap();
        assert_eq!(value2, "updated_value");
    }

    #[test]
    fn test_get_setting_or_default() {
        let (db, _temp) = create_test_db();
        let conn = db.conn.lock().unwrap();

        // For a key that doesn't exist, should return default
        let value = get_setting_or_default(&conn, "embedding_model");
        assert_eq!(value, "openai/text-embedding-3-small");

        // For a key with no default, should return empty string
        let unknown = get_setting_or_default(&conn, "unknown_key");
        assert_eq!(unknown, "");
    }

    #[test]
    fn test_migrate_settings_idempotent() {
        let (db, _temp) = create_test_db();
        let conn = db.conn.lock().unwrap();

        // Get settings after first migration (done in open_or_create)
        let settings1 = get_all_settings(&conn).unwrap();

        // Run migration again
        migrate_settings(&conn).unwrap();

        // Settings should be the same
        let settings2 = get_all_settings(&conn).unwrap();
        assert_eq!(settings1.len(), settings2.len());
    }
}
