use reqwest::Client;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

// OpenRouter API request/response types
#[derive(Serialize)]
struct OpenRouterRequest {
    model: String,
    messages: Vec<Message>,
    response_format: ResponseFormat,
    temperature: f32,
    max_tokens: u32,
    provider: ProviderPreferences,
}

#[derive(Serialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    format_type: String,
    json_schema: JsonSchemaWrapper,
}

#[derive(Serialize)]
struct JsonSchemaWrapper {
    name: String,
    strict: bool,
    schema: serde_json::Value,
}

#[derive(Serialize)]
struct ProviderPreferences {
    require_parameters: bool,
}

#[derive(Deserialize)]
struct OpenRouterResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: MessageContent,
}

#[derive(Deserialize)]
struct MessageContent {
    content: Option<String>,
}

// Extraction result types
#[derive(Debug, Clone, Deserialize)]
pub struct ExtractionResult {
    pub tags: Vec<TagApplication>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TagApplication {
    pub name: String,
    pub parent_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TagConsolidationResult {
    pub tags_to_remove: Vec<String>,
    pub tags_to_add: Vec<TagApplication>,
}

/// Result of looking up tag names in the database
pub struct TagLookupResult {
    pub found_ids: Vec<String>,
    pub missing_names: Vec<String>,
}

const TAG_CONSOLIDATION_PROMPT: &str = r#"You are reviewing tags applied to a complete document to consolidate overly specific tags into broader ones.

IMPORTANT - TAG IDENTIFICATION:
- Each tag is shown by its name only (e.g., "AI Consciousness")
- Tag names are case-insensitive
- Each tag name is globally unique across the entire system
- When removing tags, use the exact tag name as shown

RULES:
1. Look for tags that are too specific and could be merged into broader concepts
2. Prefer 2-level hierarchy: [Category] → [Specific Tag]
3. Merge similar/overlapping tags (e.g., "AI Consciousness" + "AI Systems" → "AI")
4. Keep tags that represent distinct concepts

Your job is to:
1. Review the current tags on this atom
2. Identify which tags should be REMOVED (overly specific)
3. Suggest new broader tags to ADD (if needed)

Examples:
- REMOVE: ["AI Consciousness", "AI Systems"]
- ADD: [{"name": "AI", "parent_name": "Topics"}]

Be conservative - only consolidate when truly warranted."#;

const SYSTEM_PROMPT: &str = r#"You are a knowledge management assistant that categorizes text into a tag hierarchy.

IMPORTANT:
- Return ALL tags that apply to this text
- Each tag has a name and optional parent_name
- Tag names are case-insensitive and globally unique
- Use existing tags from the hierarchy when applicable

HIERARCHY STRUCTURE:
- Level 1: Categories (e.g., "Topics", "People", "Locations", "Organizations", "Events")
- Level 2: Specific tags (e.g., "AI", "John Smith", "San Francisco")
- Keep it flat: 2 levels maximum

EXAMPLES:
- {"name": "AI", "parent_name": "Topics"}
- {"name": "Machine Learning", "parent_name": "Topics"}
- {"name": "San Francisco", "parent_name": "Locations"}

Guidelines:
- Use existing tags from the provided hierarchy when possible
- Create new tags only when needed
- Be specific but not overly granular
- Only include tags you're confident are relevant"#;

/// Extract tags from a single chunk using OpenRouter API
pub async fn extract_tags_from_chunk(
    client: &Client,
    api_key: &str,
    chunk_content: &str,
    tag_tree_json: &str,
    model: &str,
) -> Result<ExtractionResult, String> {
    let user_content = format!(
        "EXISTING TAG HIERARCHY:\n{}\n\nTEXT TO ANALYZE:\n{}",
        tag_tree_json,
        chunk_content
    );

    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "tags": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Name of the tag to apply"
                        },
                        "parent_name": {
                            "type": ["string", "null"],
                            "description": "Name of parent tag, or null for top-level categories"
                        }
                    },
                    "required": ["name", "parent_name"],
                    "additionalProperties": false
                },
                "description": "Tags to apply to this text"
            }
        },
        "required": ["tags"],
        "additionalProperties": false
    });

    let request = OpenRouterRequest {
        model: model.to_string(),
        messages: vec![
            Message {
                role: "system".to_string(),
                content: SYSTEM_PROMPT.to_string(),
            },
            Message {
                role: "user".to_string(),
                content: user_content,
            },
        ],
        response_format: ResponseFormat {
            format_type: "json_schema".to_string(),
            json_schema: JsonSchemaWrapper {
                name: "extraction_result".to_string(),
                strict: true,
                schema: schema,
            },
        },
        temperature: 0.1,
        max_tokens: 1000,
        provider: ProviderPreferences {
            require_parameters: true,
        },
    };

    // Retry logic with exponential backoff
    let mut last_error = String::new();
    for attempt in 0..3 {
        if attempt > 0 {
            // Exponential backoff: 1s, 2s, 4s
            tokio::time::sleep(std::time::Duration::from_secs(1 << attempt)).await;
        }

        let response = client
            .post("https://openrouter.ai/api/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .header("HTTP-Referer", "https://atomic.app")
            .header("X-Title", "Atomic")
            .json(&request)
            .send()
            .await;

        match response {
            Ok(resp) => {
                if resp.status().is_success() {
                    let body = resp.text().await.map_err(|e| format!("Failed to read response: {}", e))?;
                    
                    // Parse the OpenRouter response
                    let openrouter_response: OpenRouterResponse = serde_json::from_str(&body)
                        .map_err(|e| format!("Failed to parse OpenRouter response: {} - Body: {}", e, body))?;
                    
                    if let Some(choice) = openrouter_response.choices.first() {
                        if let Some(content) = &choice.message.content {
                            // Log the raw LLM output
                            eprintln!("=== TAG EXTRACTION LLM OUTPUT ===");
                            eprintln!("{}", content);
                            eprintln!("=================================");

                            // Parse the extraction result from the content
                            let result: ExtractionResult = serde_json::from_str(content)
                                .map_err(|e| format!("Failed to parse extraction result: {} - Content: {}", e, content))?;
                            return Ok(result);
                        }
                    }
                    return Err("No content in response".to_string());
                } else if resp.status().as_u16() == 429 {
                    // Rate limited, will retry
                    last_error = "Rate limited".to_string();
                    continue;
                } else {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    last_error = format!("API error ({}): {}", status, body);
                    // Don't retry on non-rate-limit errors
                    break;
                }
            }
            Err(e) => {
                last_error = format!("Network error: {}", e);
                // Will retry on network errors
                continue;
            }
        }
    }

    Err(last_error)
}

/// Get the tag tree as JSON for the LLM prompt
pub fn get_tag_tree_json(conn: &Connection) -> Result<String, String> {
    #[derive(Serialize)]
    struct TagNode {
        id: String,
        name: String,
        parent_id: Option<String>,
        children: Vec<TagNode>,
    }

    // Get all tags
    let mut stmt = conn
        .prepare("SELECT id, name, parent_id FROM tags ORDER BY name")
        .map_err(|e| format!("Failed to prepare tag query: {}", e))?;

    let tags: Vec<(String, String, Option<String>)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .map_err(|e| format!("Failed to query tags: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect tags: {}", e))?;

    if tags.is_empty() {
        return Ok(r#"{"tags": []}"#.to_string());
    }

    // Build tree structure
    fn build_tree(tags: &[(String, String, Option<String>)], parent_id: Option<&str>) -> Vec<TagNode> {
        tags.iter()
            .filter(|(_, _, pid)| pid.as_deref() == parent_id)
            .map(|(id, name, pid)| TagNode {
                id: id.clone(),
                name: name.clone(),
                parent_id: pid.clone(),
                children: build_tree(tags, Some(id)),
            })
            .collect()
    }

    let tree = build_tree(&tags, None);
    
    #[derive(Serialize)]
    struct TagTreeWrapper {
        tags: Vec<TagNode>,
    }

    serde_json::to_string(&TagTreeWrapper { tags: tree })
        .map_err(|e| format!("Failed to serialize tag tree: {}", e))
}

/// Get simplified tag tree for LLM (names only, no IDs)
/// This exposes only tag names to the LLM without internal database IDs
pub fn get_tag_tree_for_llm(conn: &Connection) -> Result<String, String> {
    #[derive(Serialize)]
    struct TagNodeSimple {
        name: String,
        children: Vec<TagNodeSimple>,
    }

    // Get all tags
    let mut stmt = conn
        .prepare("SELECT id, name, parent_id FROM tags ORDER BY name")
        .map_err(|e| format!("Failed to prepare tag query: {}", e))?;

    let tags: Vec<(String, String, Option<String>)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .map_err(|e| format!("Failed to query tags: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect tags: {}", e))?;

    if tags.is_empty() {
        return Ok(r#"{"tags": []}"#.to_string());
    }

    // Build tree structure with names only
    fn build_tree(tags: &[(String, String, Option<String>)], parent_id: Option<&str>) -> Vec<TagNodeSimple> {
        tags.iter()
            .filter(|(_, _, pid)| pid.as_deref() == parent_id)
            .map(|(id, name, _)| TagNodeSimple {
                name: name.clone(),
                children: build_tree(tags, Some(id)),
            })
            .collect()
    }

    let tree = build_tree(&tags, None);

    #[derive(Serialize)]
    struct TagTreeWrapper {
        tags: Vec<TagNodeSimple>,
    }

    serde_json::to_string_pretty(&TagTreeWrapper { tags: tree })
        .map_err(|e| format!("Failed to serialize tag tree: {}", e))
}

/// Link tags to an atom (append to existing tags)
pub fn link_tags_to_atom(conn: &Connection, atom_id: &str, tag_ids: &[String]) -> Result<(), String> {
    for tag_id in tag_ids {
        // Use INSERT OR IGNORE to avoid duplicates
        conn.execute(
            "INSERT OR IGNORE INTO atom_tags (atom_id, tag_id) VALUES (?1, ?2)",
            rusqlite::params![atom_id, tag_id],
        )
        .map_err(|e| format!("Failed to link tag to atom: {}", e))?;
    }
    Ok(())
}

/// Convert tag names to IDs (case-insensitive lookup)
/// Used to translate LLM responses (which use names) to database IDs
pub fn tag_names_to_ids(conn: &Connection, names: &[String]) -> Result<TagLookupResult, String> {
    let mut found_ids = Vec::new();
    let mut missing_names = Vec::new();

    for name in names {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            continue;
        }

        let tag_id: Option<String> = conn
            .query_row(
                "SELECT id FROM tags WHERE LOWER(name) = LOWER(?1)",
                [trimmed],
                |row| row.get(0),
            )
            .ok();

        if let Some(id) = tag_id {
            found_ids.push(id);
        } else {
            missing_names.push(trimmed.to_string());
        }
    }

    Ok(TagLookupResult {
        found_ids,
        missing_names,
    })
}

/// Convert tag IDs to names
/// Used to show tag names to users or for debugging
pub fn tag_ids_to_names(conn: &Connection, ids: &[String]) -> Result<Vec<String>, String> {
    let mut tag_names = Vec::new();

    for id in ids {
        let name: Option<String> = conn
            .query_row("SELECT name FROM tags WHERE id = ?1", [id], |row| row.get(0))
            .ok();

        if let Some(n) = name {
            tag_names.push(n);
        }
    }

    Ok(tag_names)
}

/// Deduplicate tag names case-insensitively
/// Keeps first occurrence of each unique name (case-insensitive)
pub fn deduplicate_tag_names(names: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();

    for name in names {
        let lower = name.to_lowercase();
        if seen.insert(lower) {
            result.push(name);
        }
    }

    result
}

/// Get tag ID by name, or create it if it doesn't exist
/// Also ensures parent tag exists if parent_name is provided (recursive)
pub fn get_or_create_tag(
    conn: &Connection,
    tag_name: &str,
    parent_name: &Option<String>,
) -> Result<String, String> {
    let trimmed_name = tag_name.trim();

    // Validate tag name
    if trimmed_name.is_empty() || trimmed_name.eq_ignore_ascii_case("null") {
        return Err(format!("Invalid tag name: '{}'", tag_name));
    }

    // Try to find existing tag
    if let Some(existing_id) = conn
        .query_row(
            "SELECT id FROM tags WHERE LOWER(name) = LOWER(?1)",
            [trimmed_name],
            |row| row.get(0),
        )
        .ok()
    {
        return Ok(existing_id);
    }

    // Tag doesn't exist, create it
    let parent_id = if let Some(parent) = parent_name {
        let trimmed_parent = parent.trim();
        // Skip invalid parent names
        if !trimmed_parent.is_empty() && !trimmed_parent.eq_ignore_ascii_case("null") {
            // Ensure parent exists (recursively create if needed)
            Some(get_or_create_tag(conn, trimmed_parent, &None)?)
        } else {
            None
        }
    } else {
        None
    };

    // Create the tag
    let tag_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO tags (id, name, parent_id, created_at) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![&tag_id, trimmed_name, parent_id, &now],
    )
    .map_err(|e| format!("Failed to create tag '{}': {}", trimmed_name, e))?;

    Ok(tag_id)
}

/// Recursively clean up unused parent tags
/// Called after deleting a tag to check if parent becomes orphaned
pub fn cleanup_orphaned_parents(conn: &Connection, tag_id: &str) -> Result<(), String> {
    // Get parent of this tag
    let parent_id: Option<String> = conn
        .query_row(
            "SELECT parent_id FROM tags WHERE id = ?1",
            [tag_id],
            |row| row.get(0),
        )
        .ok()
        .and_then(|opt| opt);  // Handle NULL parent_id

    if let Some(parent) = parent_id {
        // Check if parent has any children left
        let child_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM tags WHERE parent_id = ?1",
                [&parent],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Check if parent is linked to any atoms
        let atom_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM atom_tags WHERE tag_id = ?1",
                [&parent],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Check if parent has a wiki article
        let has_wiki: bool = conn
            .query_row(
                "SELECT 1 FROM wiki_articles WHERE tag_id = ?1",
                [&parent],
                |_| Ok(true),
            )
            .unwrap_or(false);

        // If parent is unused and has no wiki, delete it and recurse
        if child_count == 0 && atom_count == 0 && !has_wiki {
            eprintln!("Cleaning up orphaned parent tag: {}", parent);
            conn.execute("DELETE FROM tags WHERE id = ?1", [&parent])
                .map_err(|e| format!("Failed to delete orphaned parent: {}", e))?;
            cleanup_orphaned_parents(conn, &parent)?;  // Recurse to grandparent
        }
    }

    Ok(())
}

/// Build tag info string for consolidation prompt (synchronous, for use before async call)
pub fn build_tag_info_for_consolidation(
    conn: &Connection,
    current_tag_ids: &[String],
) -> Result<String, String> {
    let mut current_tags_info = String::from("CURRENT TAGS ON THIS ATOM:\n");

    for tag_id in current_tag_ids {
        let tag_name: Result<String, rusqlite::Error> = conn.query_row(
            "SELECT name FROM tags WHERE id = ?1",
            [tag_id],
            |row| row.get(0)
        );

        match tag_name {
            Ok(name) => {
                current_tags_info.push_str(&format!("- {}\n", name));
            },
            Err(e) => {
                eprintln!("Warning: Failed to get tag info for {}: {}", tag_id, e);
                continue;
            }
        }
    }

    Ok(current_tags_info)
}

/// Consolidate tags on an atom by merging overly specific tags into broader ones
pub async fn consolidate_atom_tags(
    client: &Client,
    api_key: &str,
    tag_info: String,
    model: &str,
) -> Result<TagConsolidationResult, String> {
    let user_content = format!("{}\n\nProvide your consolidation recommendations.", tag_info);

    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "tags_to_remove": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Names of tags to remove"
            },
            "tags_to_add": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Name of the tag to add"
                        },
                        "parent_name": {
                            "type": ["string", "null"],
                            "description": "Name of parent tag, or null for top-level categories"
                        }
                    },
                    "required": ["name", "parent_name"],
                    "additionalProperties": false
                },
                "description": "New broader tags to create and add"
            }
        },
        "required": ["tags_to_remove", "tags_to_add"],
        "additionalProperties": false
    });

    let request = OpenRouterRequest {
        model: model.to_string(),
        messages: vec![
            Message {
                role: "system".to_string(),
                content: TAG_CONSOLIDATION_PROMPT.to_string(),
            },
            Message {
                role: "user".to_string(),
                content: user_content,
            },
        ],
        response_format: ResponseFormat {
            format_type: "json_schema".to_string(),
            json_schema: JsonSchemaWrapper {
                name: "consolidation_result".to_string(),
                strict: true,
                schema: schema,
            },
        },
        temperature: 0.1,
        max_tokens: 1000,
        provider: ProviderPreferences {
            require_parameters: true,
        },
    };

    // Retry logic with exponential backoff
    let mut last_error = String::new();
    for attempt in 0..3 {
        if attempt > 0 {
            // Exponential backoff: 1s, 2s, 4s
            tokio::time::sleep(std::time::Duration::from_secs(1 << attempt)).await;
        }

        let response = client
            .post("https://openrouter.ai/api/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .header("HTTP-Referer", "https://atomic.app")
            .header("X-Title", "Atomic")
            .json(&request)
            .send()
            .await;

        match response {
            Ok(resp) => {
                if resp.status().is_success() {
                    let body = resp.text().await.map_err(|e| format!("Failed to read response: {}", e))?;

                    // Parse the OpenRouter response
                    let openrouter_response: OpenRouterResponse = serde_json::from_str(&body)
                        .map_err(|e| format!("Failed to parse OpenRouter response: {} - Body: {}", e, body))?;

                    if let Some(choice) = openrouter_response.choices.first() {
                        if let Some(content) = &choice.message.content {
                            // Log the raw LLM output
                            eprintln!("=== TAG CONSOLIDATION LLM OUTPUT ===");
                            eprintln!("{}", content);
                            eprintln!("====================================");

                            // Parse the consolidation result from the content
                            let result: TagConsolidationResult = serde_json::from_str(content)
                                .map_err(|e| format!("Failed to parse consolidation result: {} - Content: {}", e, content))?;
                            return Ok(result);
                        }
                    }
                    return Err("No content in response".to_string());
                } else if resp.status().as_u16() == 429 {
                    // Rate limited, will retry
                    last_error = "Rate limited".to_string();
                    continue;
                } else {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    last_error = format!("API error ({}): {}", status, body);
                    // Don't retry on non-rate-limit errors
                    break;
                }
            }
            Err(e) => {
                last_error = format!("Network error: {}", e);
                // Will retry on network errors
                continue;
            }
        }
    }

    Err(last_error)
}

