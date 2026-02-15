use crate::event_bridge::embedding_event_callback;
use crate::mcp::types::*;
use crate::state::ServerEvent;
use atomic_core::AtomicCore;
use rmcp::{
    handler::server::tool::ToolRouter,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router, ErrorData, ServerHandler,
};
use tokio::sync::broadcast;

/// MCP Server for Atomic knowledge base (standalone server variant)
#[derive(Clone)]
pub struct AtomicMcpServer {
    core: AtomicCore,
    event_tx: broadcast::Sender<ServerEvent>,
    tool_router: ToolRouter<Self>,
}

impl AtomicMcpServer {
    pub fn new(core: AtomicCore, event_tx: broadcast::Sender<ServerEvent>) -> Self {
        Self {
            core,
            event_tx,
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl AtomicMcpServer {
    /// Search for atoms using semantic vector similarity
    #[tool(
        description = "Search for atoms using semantic vector similarity. Returns atoms with content relevant to the query, ranked by similarity score. Use this to find information in the knowledge base."
    )]
    async fn semantic_search(
        &self,
        Parameters(params): Parameters<SemanticSearchParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let limit = params.limit.unwrap_or(10).min(50);
        let threshold = params.threshold.unwrap_or(0.3).clamp(0.0, 1.0);

        let options = atomic_core::SearchOptions::new(
            params.query,
            atomic_core::SearchMode::Semantic,
            limit,
        )
        .with_threshold(threshold);

        let results = self
            .core
            .search(options)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let search_results: Vec<SearchResult> = results
            .into_iter()
            .map(|r| SearchResult {
                atom_id: r.atom.atom.id.clone(),
                content_preview: r.atom.atom.content.chars().take(200).collect(),
                similarity_score: r.similarity_score,
                matching_chunk: r.matching_chunk_content,
            })
            .collect();

        let response_text = serde_json::to_string_pretty(&search_results)
            .map_err(|e| ErrorData::internal_error(format!("Serialization error: {}", e), None))?;

        Ok(CallToolResult::success(vec![Content::text(response_text)]))
    }

    /// Read a single atom with optional line-based pagination
    #[tool(
        description = "Read the full content of a specific atom by its ID. Supports line-based pagination for large atoms. Returns the atom content, metadata, and tags."
    )]
    async fn read_atom(
        &self,
        Parameters(params): Parameters<ReadAtomParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let limit = params.limit.unwrap_or(100).min(500) as usize;
        let offset = params.offset.unwrap_or(0).max(0) as usize;

        let db = self.core.database();
        let conn = db
            .new_connection()
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let atom_result: Result<(String, String, String, String), rusqlite::Error> = conn
            .query_row(
                "SELECT id, content, created_at, updated_at FROM atoms WHERE id = ?1",
                [&params.atom_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            );

        match atom_result {
            Ok((id, content, created_at, updated_at)) => {
                let lines: Vec<&str> = content.lines().collect();
                let total_lines = lines.len() as i32;
                let start = offset.min(lines.len());
                let end = (start + limit).min(lines.len());
                let paginated_lines = &lines[start..end];
                let returned_lines = paginated_lines.len() as i32;
                let has_more = end < lines.len();

                let mut paginated_content = paginated_lines.join("\n");

                if has_more {
                    paginated_content.push_str(&format!(
                        "\n\n(Atom content continues. Use offset {} to read more lines.)",
                        end
                    ));
                }

                let response = AtomContent {
                    atom_id: id,
                    content: paginated_content,
                    total_lines,
                    returned_lines,
                    offset: offset as i32,
                    has_more,
                    created_at,
                    updated_at,
                };

                let response_text = serde_json::to_string_pretty(&response)
                    .map_err(|e| {
                        ErrorData::internal_error(format!("Serialization error: {}", e), None)
                    })?;

                Ok(CallToolResult::success(vec![Content::text(response_text)]))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(CallToolResult::success(vec![
                Content::text(format!("Atom not found: {}", params.atom_id)),
            ])),
            Err(e) => Err(ErrorData::internal_error(e.to_string(), None)),
        }
    }

    /// Create a new atom with markdown content
    #[tool(
        description = "Create a new atom with markdown content. The atom will be automatically processed for embeddings and tag extraction. Returns the created atom ID."
    )]
    async fn create_atom(
        &self,
        Parameters(params): Parameters<CreateAtomParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let request = atomic_core::CreateAtomRequest {
            content: params.content.clone(),
            source_url: params.source_url,
            tag_ids: params.tag_ids.unwrap_or_default(),
        };

        let on_event = embedding_event_callback(self.event_tx.clone());

        let result = self
            .core
            .create_atom(request, on_event)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        // Broadcast atom creation event
        let _ = self.event_tx.send(ServerEvent::AtomCreated {
            atom: result.clone(),
        });

        let response = CreatedAtom {
            atom_id: result.atom.id.clone(),
            content_preview: result.atom.content.chars().take(200).collect(),
            tags: result.tags.iter().map(|t| t.name.clone()).collect(),
            embedding_status: result.atom.embedding_status.clone(),
        };

        let response_text = serde_json::to_string_pretty(&response)
            .map_err(|e| ErrorData::internal_error(format!("Serialization error: {}", e), None))?;

        Ok(CallToolResult::success(vec![Content::text(response_text)]))
    }
}

#[tool_handler]
impl ServerHandler for AtomicMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Atomic is a personal knowledge base with semantic search capabilities. \
                 Use semantic_search to find relevant information, read_atom to get full content, \
                 and create_atom to add new notes."
                    .to_string(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}
