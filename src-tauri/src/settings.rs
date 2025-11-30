use rusqlite::Connection;
use std::collections::HashMap;

/// Get all settings as a HashMap
pub fn get_all_settings(conn: &Connection) -> Result<HashMap<String, String>, String> {
    let mut stmt = conn
        .prepare("SELECT key, value FROM settings")
        .map_err(|e| format!("Failed to prepare settings query: {}", e))?;

    let settings = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| format!("Failed to query settings: {}", e))?
        .collect::<Result<HashMap<_, _>, _>>()
        .map_err(|e| format!("Failed to collect settings: {}", e))?;

    Ok(settings)
}

/// Get a single setting by key
pub fn get_setting(conn: &Connection, key: &str) -> Result<String, String> {
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        [key],
        |row| row.get(0),
    )
    .map_err(|e| format!("Failed to get setting '{}': {}", key, e))
}

/// Set a setting (upsert)
pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [key, value],
    )
    .map_err(|e| format!("Failed to set setting: {}", e))?;

    Ok(())
}

