//! Route configuration — registers all API route groups

mod auth;
mod atoms;
mod canvas;
mod chat;
mod clustering;
mod embedding;
mod graph;
mod import;
pub mod oauth;
mod ollama;
mod search;
mod settings;
mod utils;
mod wiki;

use actix_web::web;

pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    // Atoms
    cfg.route("/atoms", web::get().to(atoms::get_atoms));
    cfg.route("/atoms", web::post().to(atoms::create_atom));
    cfg.route("/atoms/{id}", web::get().to(atoms::get_atom));
    cfg.route("/atoms/{id}", web::put().to(atoms::update_atom));
    cfg.route("/atoms/{id}", web::delete().to(atoms::delete_atom));
    cfg.route("/atoms/{id}/similar", web::get().to(search::find_similar));
    cfg.route(
        "/atoms/{id}/embedding-status",
        web::get().to(embedding::get_embedding_status),
    );

    // Tags
    cfg.route("/tags", web::get().to(atoms::get_tags));
    cfg.route("/tags", web::post().to(atoms::create_tag));
    cfg.route("/tags/{id}/children", web::get().to(atoms::get_tag_children));
    cfg.route("/tags/{id}", web::put().to(atoms::update_tag));
    cfg.route("/tags/{id}", web::delete().to(atoms::delete_tag));

    // Search
    cfg.route("/search", web::post().to(search::search));

    // Wiki
    cfg.route("/wiki", web::get().to(wiki::get_all_wiki_articles));
    cfg.route("/wiki/suggestions", web::get().to(wiki::get_wiki_suggestions));
    cfg.route("/wiki/{tag_id}", web::get().to(wiki::get_wiki));
    cfg.route("/wiki/{tag_id}/status", web::get().to(wiki::get_wiki_status));
    cfg.route(
        "/wiki/{tag_id}/generate",
        web::post().to(wiki::generate_wiki),
    );
    cfg.route("/wiki/{tag_id}/update", web::post().to(wiki::update_wiki));
    cfg.route("/wiki/{tag_id}", web::delete().to(wiki::delete_wiki));
    cfg.route(
        "/wiki/{tag_id}/related",
        web::get().to(wiki::get_related_tags),
    );
    cfg.route(
        "/wiki/{tag_id}/links",
        web::get().to(wiki::get_wiki_links),
    );
    cfg.route(
        "/wiki/recompute-tag-embeddings",
        web::post().to(wiki::recompute_all_tag_embeddings),
    );

    // Settings
    cfg.route("/settings", web::get().to(settings::get_settings));
    cfg.route("/settings/{key}", web::put().to(settings::set_setting));
    cfg.route(
        "/settings/test-openrouter",
        web::post().to(settings::test_openrouter_connection),
    );
    cfg.route("/settings/models", web::get().to(settings::get_available_llm_models));

    // Embedding management
    cfg.route(
        "/embeddings/process-pending",
        web::post().to(embedding::process_pending_embeddings),
    );
    cfg.route(
        "/embeddings/process-tagging",
        web::post().to(embedding::process_pending_tagging),
    );
    cfg.route(
        "/embeddings/retry/{atom_id}",
        web::post().to(embedding::retry_embedding),
    );
    cfg.route(
        "/embeddings/reset-stuck",
        web::post().to(embedding::reset_stuck_processing),
    );

    // Canvas
    cfg.route("/canvas/positions", web::get().to(canvas::get_positions));
    cfg.route("/canvas/positions", web::put().to(canvas::save_positions));
    cfg.route(
        "/canvas/atoms-with-embeddings",
        web::get().to(canvas::get_atoms_with_embeddings),
    );
    cfg.route(
        "/canvas/level",
        web::post().to(canvas::get_canvas_level),
    );

    // Graph
    cfg.route("/graph/edges", web::get().to(graph::get_semantic_edges));
    cfg.route(
        "/graph/neighborhood/{atom_id}",
        web::get().to(graph::get_atom_neighborhood),
    );
    cfg.route(
        "/graph/rebuild-edges",
        web::post().to(graph::rebuild_semantic_edges),
    );

    // Clustering
    cfg.route("/clustering/compute", web::post().to(clustering::compute_clusters));
    cfg.route("/clustering", web::get().to(clustering::get_clusters));
    cfg.route(
        "/clustering/connection-counts",
        web::get().to(clustering::get_connection_counts),
    );

    // Chat / Conversations
    cfg.route("/conversations", web::post().to(chat::create_conversation));
    cfg.route("/conversations", web::get().to(chat::get_conversations));
    cfg.route(
        "/conversations/{id}",
        web::get().to(chat::get_conversation),
    );
    cfg.route(
        "/conversations/{id}",
        web::put().to(chat::update_conversation),
    );
    cfg.route(
        "/conversations/{id}",
        web::delete().to(chat::delete_conversation),
    );
    cfg.route(
        "/conversations/{id}/scope",
        web::put().to(chat::set_conversation_scope),
    );
    cfg.route(
        "/conversations/{id}/scope/tags",
        web::post().to(chat::add_tag_to_scope),
    );
    cfg.route(
        "/conversations/{id}/scope/tags/{tag_id}",
        web::delete().to(chat::remove_tag_from_scope),
    );
    cfg.route(
        "/conversations/{id}/messages",
        web::post().to(chat::send_chat_message),
    );

    // Ollama
    cfg.route("/ollama/test", web::post().to(ollama::test_ollama));
    cfg.route("/ollama/models", web::get().to(ollama::get_ollama_models));
    cfg.route(
        "/ollama/embedding-models",
        web::get().to(ollama::get_ollama_embedding_models),
    );
    cfg.route(
        "/ollama/llm-models",
        web::get().to(ollama::get_ollama_llm_models),
    );
    cfg.route(
        "/provider/verify",
        web::get().to(ollama::verify_provider_configured),
    );

    // Utils
    cfg.route("/utils/sqlite-vec", web::get().to(utils::check_sqlite_vec));
    cfg.route("/utils/compact-tags", web::post().to(utils::compact_tags));

    // Auth / Token management
    cfg.route("/auth/tokens", web::post().to(auth::create_token));
    cfg.route("/auth/tokens", web::get().to(auth::list_tokens));
    cfg.route("/auth/tokens/{id}", web::delete().to(auth::revoke_token));

    // Import
    cfg.route(
        "/import/obsidian",
        web::post().to(import::import_obsidian_vault),
    );
}
