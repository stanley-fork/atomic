mod chunking;
mod commands;
mod db;
mod embedding;
mod extraction;
mod models;
mod settings;

use db::{Database, SharedDatabase};
use std::sync::Arc;
use tauri::Manager;
use tauri::path::BaseDirectory;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("Failed to get app data directory");

            // Get the resource directory where model and extension are bundled
            let resource_dir = app
                .path()
                .resolve("resources", BaseDirectory::Resource)
                .expect("Failed to resolve resource directory");

            let database = Database::new(app_data_dir.clone(), resource_dir.clone())
                .expect("Failed to initialize database");

            // Create a shared database reference for embedding tasks
            // This creates a new connection to the same database file
            let shared_conn = database
                .new_connection()
                .expect("Failed to create shared database connection");
            let shared_db: SharedDatabase = Arc::new(Database {
                conn: std::sync::Mutex::new(shared_conn),
                db_path: database.db_path.clone(),
                resource_dir: database.resource_dir.clone(),
            });

            app.manage(database);
            app.manage(shared_db);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_all_atoms,
            commands::get_atom,
            commands::create_atom,
            commands::update_atom,
            commands::delete_atom,
            commands::get_all_tags,
            commands::create_tag,
            commands::update_tag,
            commands::delete_tag,
            commands::get_atoms_by_tag,
            commands::check_sqlite_vec,
            commands::find_similar_atoms,
            commands::search_atoms_semantic,
            commands::retry_embedding,
            commands::process_pending_embeddings,
            commands::get_embedding_status,
            commands::get_settings,
            commands::set_setting,
            commands::test_openrouter_connection,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

