mod agent;
mod chat;
mod chunking;
mod clustering;
mod commands;
mod db;
mod embedding;
mod extraction;
mod http_server;
mod models;
mod providers;
mod settings;
mod wiki;

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
            app.manage(shared_db.clone());

            // Start HTTP server in background for browser extension
            let server_shared_db = shared_db.clone();
            let server_app_handle = app.handle().clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
                rt.block_on(async move {
                    if let Err(e) = http_server::start_server(server_shared_db, server_app_handle).await {
                        eprintln!("HTTP server error: {}", e);
                    }
                });
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_all_atoms,
            commands::get_atom_by_id,
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
            commands::get_available_llm_models,
            commands::get_wiki_article,
            commands::get_wiki_article_status,
            commands::generate_wiki_article,
            commands::update_wiki_article,
            commands::delete_wiki_article,
            commands::get_atom_positions,
            commands::save_atom_positions,
            commands::get_atoms_with_embeddings,
            // Semantic graph commands
            commands::get_semantic_edges,
            commands::get_atom_neighborhood,
            commands::rebuild_semantic_edges,
            // Clustering commands
            commands::compute_clusters,
            commands::get_clusters,
            commands::get_connection_counts,
            // Chat commands
            chat::create_conversation,
            chat::get_conversations,
            chat::get_conversation,
            chat::update_conversation,
            chat::delete_conversation,
            chat::set_conversation_scope,
            chat::add_tag_to_scope,
            chat::remove_tag_from_scope,
            // Agent/messaging
            agent::send_chat_message,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

