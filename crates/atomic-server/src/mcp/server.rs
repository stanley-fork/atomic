use crate::event_bridge::embedding_event_callback;
use crate::mcp::types::*;
use crate::state::ServerEvent;
use atomic_core::manager::DatabaseManager;
use atomic_core::AtomicCore;
use rmcp::{
    handler::server::tool::ToolRouter,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, Content, ServerCapabilities, ServerInfo},
    service::RequestContext,
    RoleServer,
    tool, tool_handler, tool_router, ErrorData, ServerHandler,
};
use std::sync::Arc;
use tokio::sync::broadcast;

/// Extension type inserted by the `on_request` hook to carry the `?db=` selection.
#[derive(Clone, Debug)]
pub struct DbSelection(pub Option<String>);

/// MCP Server for Atomic knowledge base
#[derive(Clone)]
pub struct AtomicMcpServer {
    manager: Arc<DatabaseManager>,
    event_tx: broadcast::Sender<ServerEvent>,
    tool_router: ToolRouter<Self>,
}

impl AtomicMcpServer {
    pub fn new(manager: Arc<DatabaseManager>, event_tx: broadcast::Sender<ServerEvent>) -> Self {
        Self {
            manager,
            event_tx,
            tool_router: Self::tool_router(),
        }
    }

    /// Resolve the correct AtomicCore from the request context's DbSelection extension.
    fn resolve_core(&self, context: &RequestContext<RoleServer>) -> Result<AtomicCore, ErrorData> {
        let db_id = context.extensions.get::<DbSelection>().and_then(|s| s.0.clone());
        match db_id {
            Some(id) => self
                .manager
                .get_core(&id)
                .map_err(|e| ErrorData::internal_error(format!("Database not found: {}", e), None)),
            None => self
                .manager
                .active_core()
                .map_err(|e| ErrorData::internal_error(e.to_string(), None)),
        }
    }
}

#[tool_router]
impl AtomicMcpServer {
    /// Search for atoms using hybrid keyword + semantic search
    #[tool(
        description = "Search your memory for relevant knowledge. Use this before answering questions that may relate to previously stored information. Returns matching atoms ranked by relevance. Set since_days to constrain to recent atoms (e.g., 7 for last week, 30 for last month) when the question is time-sensitive."
    )]
    async fn semantic_search(
        &self,
        context: RequestContext<RoleServer>,
        Parameters(params): Parameters<SemanticSearchParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let core = self.resolve_core(&context)?;
        let limit = params.limit.unwrap_or(10).min(50);
        let options = atomic_core::SearchOptions::new(
            params.query,
            atomic_core::SearchMode::Hybrid,
            limit,
        )
        .with_threshold(0.3)
        .with_since_days(params.since_days);

        let results = core
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
        description = "Read the full content of a specific atom. Use this after semantic_search returns a relevant result and you need the complete text. Supports pagination for large atoms."
    )]
    async fn read_atom(
        &self,
        context: RequestContext<RoleServer>,
        Parameters(params): Parameters<ReadAtomParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let core = self.resolve_core(&context)?;
        let limit = params.limit.unwrap_or(500).min(500) as usize;
        let offset = params.offset.unwrap_or(0).max(0) as usize;

        let atom_with_tags = match core.get_atom(&params.atom_id) {
            Ok(Some(a)) => a,
            Ok(None) => {
                return Ok(CallToolResult::success(vec![
                    Content::text(format!("Atom not found: {}", params.atom_id)),
                ]));
            }
            Err(e) => return Err(ErrorData::internal_error(e.to_string(), None)),
        };

        let content = &atom_with_tags.atom.content;
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
            atom_id: atom_with_tags.atom.id,
            content: paginated_content,
            total_lines,
            returned_lines,
            offset: offset as i32,
            has_more,
            created_at: atom_with_tags.atom.created_at,
            updated_at: atom_with_tags.atom.updated_at,
        };

        let response_text = serde_json::to_string_pretty(&response)
            .map_err(|e| {
                ErrorData::internal_error(format!("Serialization error: {}", e), None)
            })?;

        Ok(CallToolResult::success(vec![Content::text(response_text)]))
    }

    /// Create a new atom with markdown content
    #[tool(
        description = "Remember something new. Create an atom when you learn information worth retaining across conversations — user preferences, decisions, project context, or important facts. Write concise, self-contained markdown."
    )]
    async fn create_atom(
        &self,
        context: RequestContext<RoleServer>,
        Parameters(params): Parameters<CreateAtomParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let core = self.resolve_core(&context)?;
        let request = atomic_core::CreateAtomRequest {
            content: params.content.clone(),
            source_url: params.source_url,
            published_at: None,
            tag_ids: vec![],
            skip_if_source_exists: false,
        };

        let on_event = embedding_event_callback(self.event_tx.clone());

        let result = core
            .create_atom(request, on_event)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?
            .ok_or_else(|| ErrorData::internal_error("Atom creation returned None".to_string(), None))?;

        // Broadcast atom creation event
        let _ = self.event_tx.send(ServerEvent::AtomCreated {
            atom: result.clone(),
        });

        let response = AtomResponse {
            atom_id: result.atom.id.clone(),
            content_preview: result.atom.content.chars().take(200).collect(),
            tags: result.tags.iter().map(|t| t.name.clone()).collect(),
            embedding_status: result.atom.embedding_status.clone(),
        };

        let response_text = serde_json::to_string_pretty(&response)
            .map_err(|e| ErrorData::internal_error(format!("Serialization error: {}", e), None))?;

        Ok(CallToolResult::success(vec![Content::text(response_text)]))
    }

    /// Update an existing atom's content
    #[tool(
        description = "Revise an existing atom. Use this when you find an atom with outdated or incomplete information instead of creating a duplicate. Search first to find the atom to update."
    )]
    async fn update_atom(
        &self,
        context: RequestContext<RoleServer>,
        Parameters(params): Parameters<UpdateAtomParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let core = self.resolve_core(&context)?;

        // Verify the atom exists first
        match core.get_atom(&params.atom_id) {
            Ok(Some(_)) => {}
            Ok(None) => {
                return Ok(CallToolResult::success(vec![Content::text(format!(
                    "Atom not found: {}",
                    params.atom_id
                ))]));
            }
            Err(e) => return Err(ErrorData::internal_error(e.to_string(), None)),
        }

        let request = atomic_core::UpdateAtomRequest {
            content: params.content,
            source_url: params.source_url,
            published_at: None,
            tag_ids: None,
        };

        let on_event = embedding_event_callback(self.event_tx.clone());

        let result = core
            .update_atom(&params.atom_id, request, on_event)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let response = AtomResponse {
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
                "Atomic is your long-term memory. Search before answering from recall. \
                 Remember what's worth retaining. Update what's gone stale."
                    .to_string(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}
