//! Chat agent loop with tool calling and streaming
//!
//! Provides the agentic chat loop that searches the knowledge base,
//! retrieves atoms, and generates responses with citations.
//! Uses a callback-based event system (same pattern as EmbeddingEvent).

use crate::chat;
use crate::db::Database;
use crate::models::{ChatCitation, ChatMessage, ChatMessageWithContext, ChatToolCall, SemanticSearchResult};
use crate::chunking::count_tokens;
use crate::providers::traits::LlmConfig;
use crate::providers::types::{GenerationParams, Message, StreamDelta, ToolDefinition};
use crate::providers::{create_streaming_llm_provider, ProviderConfig, ProviderType};
use crate::search::{SearchMode, SearchOptions};
use chrono::Utc;
use serde::Serialize;
use serde_json::json;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

// ==================== Chat Events ====================

/// Events emitted during the chat agent loop.
/// Consumers (Tauri, HTTP server) bridge these to their own event systems.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum ChatEvent {
    /// Streaming content delta (accumulated)
    StreamDelta {
        conversation_id: String,
        content: String,
    },
    /// Tool execution started
    ToolStart {
        conversation_id: String,
        tool_call_id: String,
        tool_name: String,
        tool_input: serde_json::Value,
    },
    /// Tool execution completed
    ToolComplete {
        conversation_id: String,
        tool_call_id: String,
        results_count: i32,
    },
    /// Full message completed
    Complete {
        conversation_id: String,
        message: ChatMessageWithContext,
    },
    /// Error during chat
    Error {
        conversation_id: String,
        error: String,
    },
}

// ==================== Tool Definitions ====================

fn get_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition::new(
            "search_atoms",
            "Search for relevant atoms using hybrid keyword and semantic search. Use this to find information related to a specific topic or question.",
            json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search query to find relevant atoms"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results to return (default: 5)",
                        "default": 5
                    }
                },
                "required": ["query"]
            }),
        ),
        ToolDefinition::new(
            "get_atom",
            "Get the full content of a specific atom by its ID. Use this when you need more detail about an atom returned from search.",
            json!({
                "type": "object",
                "properties": {
                    "atom_id": {
                        "type": "string",
                        "description": "The ID of the atom to retrieve"
                    }
                },
                "required": ["atom_id"]
            }),
        ),
    ]
}

// ==================== Tool Execution ====================

async fn execute_search_atoms(
    db: &Database,
    query: &str,
    limit: i32,
    scope_tag_ids: &[String],
    external_settings: Option<std::collections::HashMap<String, String>>,
) -> Result<Vec<SemanticSearchResult>, String> {
    let options = SearchOptions::new(query, SearchMode::Hybrid, limit)
        .with_threshold(0.3)
        .with_scope(scope_tag_ids.to_vec());
    crate::search::search_atoms_with_settings(db, options, external_settings).await
}

fn execute_get_atom(db: &Database, atom_id: &str) -> Result<Option<String>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    conn.query_row(
        "SELECT content FROM atoms WHERE id = ?1",
        [atom_id],
        |row| row.get::<_, String>(0),
    )
    .map(Some)
    .or_else(|e| {
        if matches!(e, rusqlite::Error::QueryReturnedNoRows) {
            Ok(None)
        } else {
            Err(format!("Failed to get atom: {}", e))
        }
    })
}

// ==================== System Prompt ====================

fn get_system_prompt(scope_description: &str) -> String {
    format!(
        r#"You are a helpful AI assistant with access to the user's personal knowledge base. Your role is to answer questions by searching through and referencing the user's stored information.

{}

Guidelines:
- ALWAYS use the search_atoms tool first to find relevant information before answering
- If the initial search doesn't find enough, try different search queries
- When you find relevant information, cite it using [N] notation where N is a sequential number
- Be honest if you cannot find information - do not make things up
- Keep responses concise but informative
- If the user asks about something not in their knowledge base, say so

When citing sources:
- Use [1], [2], etc. for each unique source
- Place citations immediately after the relevant claim
- You can cite the same source multiple times if needed"#,
        scope_description
    )
}

// ==================== Context Window Management ====================

/// Truncate message history to fit within the provider's context window.
/// Keeps the system prompt (first message) and the most recent user message,
/// then includes as many recent messages as fit in the remaining budget.
/// Reserves ~30% of context for the assistant's response and tool results.
fn truncate_messages_to_context(messages: Vec<Message>, context_length: Option<usize>) -> Vec<Message> {
    let max_tokens = match context_length {
        Some(ctx_len) => (ctx_len as f64 * 0.7) as usize, // Reserve 30% for response
        None => return messages, // No limit
    };

    if messages.len() <= 2 {
        return messages; // System + user message, nothing to truncate
    }

    // Count total tokens (content + tool calls)
    let message_tokens: Vec<usize> = messages
        .iter()
        .map(|m| {
            let content_tokens = count_tokens(m.content.as_deref().unwrap_or(""));
            let tool_call_tokens = m.tool_calls.as_ref().map_or(0, |tcs| {
                tcs.iter().map(|tc| {
                    let args = tc.get_arguments().unwrap_or("");
                    let name = tc.get_name().unwrap_or("");
                    // Count tokens for function name, arguments JSON, and ~10 tokens overhead per call
                    count_tokens(name) + count_tokens(args) + 10
                }).sum()
            });
            content_tokens + tool_call_tokens
        })
        .collect();

    let total: usize = message_tokens.iter().sum();
    if total <= max_tokens {
        return messages; // Fits within budget
    }

    // Always keep the system prompt (first) and most recent message (last)
    let system_tokens = message_tokens[0];
    let last_tokens = message_tokens[messages.len() - 1];
    let mut budget = max_tokens.saturating_sub(system_tokens + last_tokens);

    // Work backwards from the second-to-last message, keeping as many as fit
    let mut keep_from = messages.len() - 1; // Start just before the last message
    for i in (1..messages.len() - 1).rev() {
        if message_tokens[i] > budget {
            break;
        }
        budget -= message_tokens[i];
        keep_from = i;
    }

    let mut result = vec![messages[0].clone()];
    result.extend(messages[keep_from..].to_vec());

    eprintln!(
        "[chat] Truncated message history from {} to {} messages to fit context window ({} tokens)",
        messages.len(),
        result.len(),
        max_tokens
    );

    result
}

// ==================== Agent Loop ====================

struct AgentContext {
    conversation_id: String,
    scope_tag_ids: Vec<String>,
    messages: Vec<Message>,
    citations: Vec<(String, i32, String)>, // (atom_id, chunk_index, excerpt)
    tool_calls_record: Vec<ChatToolCall>,
}

async fn run_agent_loop<F>(
    on_event: &F,
    db: Arc<Database>,
    provider_config: ProviderConfig,
    model: String,
    mut ctx: AgentContext,
    external_settings: Option<std::collections::HashMap<String, String>>,
) -> Result<ChatMessageWithContext, String>
where
    F: Fn(ChatEvent) + Send + Sync,
{
    let provider = create_streaming_llm_provider(&provider_config)
        .map_err(|e| format!("Failed to create streaming provider: {}", e))?;
    let tools = get_tools();
    let max_iterations = 10;

    for _iteration in 0..max_iterations {
        let config = LlmConfig::new(&model).with_params(
            GenerationParams::new()
                .with_temperature(0.7)
                .with_max_tokens(4000),
        );

        // Accumulate streaming content. The Box callback captures an Arc<Mutex<String>>
        // because we can't capture `on_event` (lifetime/Send issues with Box<dyn Fn>).
        // We emit the accumulated content as a StreamDelta after the call completes.
        let accumulated_content = Arc::new(Mutex::new(String::new()));
        let accumulated_clone = Arc::clone(&accumulated_content);

        let on_delta = Box::new(move |delta: StreamDelta| {
            if let StreamDelta::Content(text) = delta {
                let mut content = accumulated_clone.lock().unwrap();
                content.push_str(&text);
            }
        });

        // Truncate messages if they've grown beyond context window (from tool results)
        let call_messages = truncate_messages_to_context(
            ctx.messages.clone(),
            provider_config.context_length_for_model(&model),
        );

        let response = provider
            .complete_streaming_with_tools(&call_messages, &tools, &config, on_delta)
            .await
            .map_err(|e| format!("API request failed: {}", e))?;

        // Emit the accumulated content as a stream delta
        if let Ok(content) = accumulated_content.lock() {
            if !content.is_empty() {
                on_event(ChatEvent::StreamDelta {
                    conversation_id: ctx.conversation_id.clone(),
                    content: content.clone(),
                });
            }
        }

        // Check if there are tool calls
        if let Some(tool_calls) = &response.tool_calls {
            // Add assistant message with tool calls to history
            if response.content.is_empty() {
                ctx.messages
                    .push(Message::assistant_with_tool_calls(tool_calls.clone()));
            } else {
                let mut msg = Message::assistant(&response.content);
                msg.tool_calls = Some(tool_calls.clone());
                ctx.messages.push(msg);
            }

            // Execute each tool call
            for tool_call in tool_calls {
                let tool_name = tool_call.get_name().unwrap_or_default();
                let tool_args_str = tool_call.get_arguments().unwrap_or_default();
                let tool_args: serde_json::Value =
                    serde_json::from_str(tool_args_str).unwrap_or(serde_json::Value::Null);

                // Emit tool start event
                on_event(ChatEvent::ToolStart {
                    conversation_id: ctx.conversation_id.clone(),
                    tool_call_id: tool_call.id.clone(),
                    tool_name: tool_name.to_string(),
                    tool_input: tool_args.clone(),
                });

                // Execute tool
                let (tool_result, results_count) = match tool_name {
                    "search_atoms" => {
                        let query = tool_args["query"].as_str().unwrap_or("");
                        let limit = tool_args["limit"].as_i64().unwrap_or(5) as i32;
                        match execute_search_atoms(&db, query, limit, &ctx.scope_tag_ids, external_settings.clone()).await {
                            Ok(results) => {
                                let count = results.len() as i32;
                                for result in results.iter() {
                                    ctx.citations.push((
                                        result.atom.atom.id.clone(),
                                        result.matching_chunk_index,
                                        result.matching_chunk_content.chars().take(200).collect(),
                                    ));
                                }
                                let result_text = results
                                    .iter()
                                    .enumerate()
                                    .map(|(i, r)| {
                                        format!(
                                            "[{}] (atom_id: {}, similarity: {:.2})\n{}",
                                            ctx.citations.len() - results.len() + i + 1,
                                            r.atom.atom.id,
                                            r.similarity_score,
                                            r.matching_chunk_content
                                        )
                                    })
                                    .collect::<Vec<_>>()
                                    .join("\n\n");
                                (result_text, count)
                            }
                            Err(e) => (format!("Error: {}", e), 0),
                        }
                    }
                    "get_atom" => {
                        let atom_id = tool_args["atom_id"].as_str().unwrap_or("");
                        match execute_get_atom(&db, atom_id) {
                            Ok(Some(content)) => (content, 1),
                            Ok(None) => ("Atom not found".to_string(), 0),
                            Err(e) => (format!("Error: {}", e), 0),
                        }
                    }
                    _ => (format!("Unknown tool: {}", tool_name), 0),
                };

                // Record tool call
                ctx.tool_calls_record.push(ChatToolCall {
                    id: tool_call.id.clone(),
                    message_id: String::new(), // Set when saving
                    tool_name: tool_name.to_string(),
                    tool_input: tool_args,
                    tool_output: Some(serde_json::Value::String(tool_result.clone())),
                    status: "complete".to_string(),
                    created_at: Utc::now().to_rfc3339(),
                    completed_at: Some(Utc::now().to_rfc3339()),
                });

                // Emit tool complete event
                on_event(ChatEvent::ToolComplete {
                    conversation_id: ctx.conversation_id.clone(),
                    tool_call_id: tool_call.id.clone(),
                    results_count,
                });

                // Add tool result to messages
                ctx.messages
                    .push(Message::tool_result(&tool_call.id, tool_result));
            }
        } else {
            // No tool calls - we have the final answer
            let content = response.content;

            // Build citations from collected data
            let citations: Vec<ChatCitation> = ctx
                .citations
                .iter()
                .enumerate()
                .map(|(i, (atom_id, chunk_index, excerpt))| ChatCitation {
                    id: Uuid::new_v4().to_string(),
                    message_id: String::new(), // Set when saving
                    citation_index: (i + 1) as i32,
                    atom_id: atom_id.clone(),
                    chunk_index: Some(*chunk_index),
                    excerpt: excerpt.clone(),
                    relevance_score: None,
                })
                .collect();

            return Ok(ChatMessageWithContext {
                message: ChatMessage {
                    id: Uuid::new_v4().to_string(),
                    conversation_id: ctx.conversation_id.clone(),
                    role: "assistant".to_string(),
                    content,
                    created_at: Utc::now().to_rfc3339(),
                    message_index: 0, // Set when saving
                },
                tool_calls: ctx.tool_calls_record,
                citations,
            });
        }
    }

    Err("Max iterations reached without completing".to_string())
}

// ==================== Public API ====================

/// Send a chat message and run the agent loop.
///
/// The `on_event` callback is invoked with streaming deltas, tool call events,
/// and completion/error events. This is the same pattern as `EmbeddingEvent`.
///
/// Returns the final assistant message with tool calls and citations.
pub async fn send_chat_message<F>(
    db: Arc<Database>,
    conversation_id: &str,
    content: &str,
    on_event: F,
) -> Result<ChatMessageWithContext, String>
where
    F: Fn(ChatEvent) + Send + Sync,
{
    send_chat_message_with_settings(db, conversation_id, content, on_event, None).await
}

/// Like `send_chat_message` but with externally-provided settings (from registry).
pub async fn send_chat_message_with_settings<F>(
    db: Arc<Database>,
    conversation_id: &str,
    content: &str,
    on_event: F,
    external_settings: Option<std::collections::HashMap<String, String>>,
) -> Result<ChatMessageWithContext, String>
where
    F: Fn(ChatEvent) + Send + Sync,
{
    // Resolve settings (from registry if provided, otherwise from data db)
    let settings_map = match external_settings {
        Some(s) => s,
        None => {
            let conn = db.conn.lock().map_err(|e| e.to_string())?;
            crate::settings::get_all_settings(&conn)
                .map_err(|e| e.to_string())?
        }
    };

    // Get provider config and model from settings
    let (provider_config, model) = {
        let provider_config = ProviderConfig::from_settings(&settings_map);

        if provider_config.provider_type == ProviderType::OpenRouter
            && provider_config.openrouter_api_key.is_none()
        {
            return Err(
                "OpenRouter API key not configured. Please set it in Settings.".to_string(),
            );
        }

        let model = match provider_config.provider_type {
            ProviderType::Ollama => provider_config.llm_model().to_string(),
            ProviderType::OpenAICompat => provider_config.llm_model().to_string(),
            ProviderType::OpenRouter => settings_map
                .get("chat_model")
                .cloned()
                .unwrap_or_else(|| "anthropic/claude-sonnet-4".to_string()),
        };

        (provider_config, model)
    };

    // Save user message
    {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        chat::save_message(&conn, conversation_id, "user", content)
            .map_err(|e| e.to_string())?;
    }

    // Get conversation context
    let (messages, scope_tag_ids, scope_description) = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        let messages = chat::get_conversation_messages(&conn, conversation_id)
            .map_err(|e| e.to_string())?;
        let scope_tag_ids = chat::get_scope_tag_ids(&conn, conversation_id)
            .map_err(|e| e.to_string())?;
        let scope_description = chat::get_scope_description(&conn, &scope_tag_ids);
        (messages, scope_tag_ids, scope_description)
    };

    // Build message history for API
    let mut api_messages = vec![Message::system(get_system_prompt(&scope_description))];
    api_messages.extend(messages);

    // Truncate to fit context window for providers with limited context
    let api_messages = truncate_messages_to_context(api_messages, provider_config.context_length_for_model(&model));

    // Create agent context
    let ctx = AgentContext {
        conversation_id: conversation_id.to_string(),
        scope_tag_ids,
        messages: api_messages,
        citations: Vec::new(),
        tool_calls_record: Vec::new(),
    };

    // Need a separate DB connection for the async agent loop
    // (the main connection's mutex can't be held across await points)
    let agent_db = Arc::new(
        Database::open(&db.db_path).map_err(|e| format!("Failed to create agent DB connection: {}", e))?,
    );

    // Run agent loop
    let mut result =
        run_agent_loop(&on_event, agent_db, provider_config, model, ctx, Some(settings_map)).await?;

    // Save assistant message
    {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        let (msg_id, msg_idx) =
            chat::save_message(&conn, conversation_id, "assistant", &result.message.content)
                .map_err(|e| e.to_string())?;

        result.message.id = msg_id.clone();
        result.message.message_index = msg_idx;

        for tool_call in &mut result.tool_calls {
            tool_call.message_id = msg_id.clone();
        }
        chat::save_tool_calls(&conn, &msg_id, &result.tool_calls)
            .map_err(|e| e.to_string())?;

        for citation in &mut result.citations {
            citation.message_id = msg_id.clone();
        }
        chat::save_citations(&conn, &msg_id, &result.citations)
            .map_err(|e| e.to_string())?;
    }

    // Emit completion event
    on_event(ChatEvent::Complete {
        conversation_id: conversation_id.to_string(),
        message: result.clone(),
    });

    Ok(result)
}
