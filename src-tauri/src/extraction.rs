use reqwest::Client;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;
use chrono::Utc;

// OpenRouter API request/response types
#[derive(Serialize)]
struct OpenRouterRequest {
    model: String,
    messages: Vec<Message>,
    response_format: ResponseFormat,
    temperature: f32,
    max_tokens: u32,
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
    pub existing_tag_ids: Vec<String>,
    pub new_tags: Vec<NewTagSuggestion>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewTagSuggestion {
    pub name: String,
    pub parent_id: Option<String>,
    pub suggested_category: String,
}

// Merged result after processing all chunks
pub struct MergedExtractionResult {
    pub existing_tag_ids: HashSet<String>,
    pub new_tags: Vec<NewTagSuggestion>,
}

const SYSTEM_PROMPT: &str = r#"You are a knowledge management assistant that categorizes text into a structured tag hierarchy. Your job is to:

1. FIRST, identify which existing tags apply to this text. Be selective—only choose tags that are clearly and directly relevant.

2. ONLY IF the text contains important entities, topics, or concepts NOT adequately covered by existing tags, suggest new tags.

3. For new tags, specify where they fit in the hierarchy by providing a parent_id, or suggest a new top-level category if truly needed.

Guidelines for new tags:
- Prefer using existing tags over creating new ones
- New top-level categories should be broad (e.g., "Locations", "People", "Organizations", "Topics", "Events")
- Be consistent with the existing hierarchy's style and granularity
- Only suggest tags you are confident are relevant

Output valid JSON only, no other text. Use this exact format:
{
  "existing_tag_ids": ["uuid-1", "uuid-2"],
  "new_tags": [
    {
      "name": "Tag Name",
      "parent_id": "uuid-of-parent-or-null",
      "suggested_category": "location|person|organization|topic|event|other"
    }
  ]
}"#;

/// Extract tags from a single chunk using OpenRouter API
pub async fn extract_tags_from_chunk(
    client: &Client,
    api_key: &str,
    chunk_content: &str,
    tag_tree_json: &str,
) -> Result<ExtractionResult, String> {
    let user_content = format!(
        "EXISTING TAG HIERARCHY:\n{}\n\nTEXT TO ANALYZE:\n{}\n\nRespond with JSON only.",
        tag_tree_json,
        chunk_content
    );

    let request = OpenRouterRequest {
        model: "anthropic/claude-haiku-4.5".to_string(),
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
            format_type: "json_object".to_string(),
        },
        temperature: 0.1,
        max_tokens: 1000,
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

/// Merge extraction results from multiple chunks
pub fn merge_chunk_extractions(results: Vec<ExtractionResult>) -> MergedExtractionResult {
    let mut existing_ids: HashSet<String> = HashSet::new();
    let mut new_tags_map: HashMap<String, NewTagSuggestion> = HashMap::new();

    for result in results {
        // Collect all existing tag IDs
        for id in result.existing_tag_ids {
            existing_ids.insert(id);
        }

        // Deduplicate new tags by normalized name (lowercase)
        for tag in result.new_tags {
            let normalized_name = tag.name.to_lowercase();
            // If duplicate, prefer the one with a more specific parent_id
            if let Some(existing) = new_tags_map.get(&normalized_name) {
                if existing.parent_id.is_none() && tag.parent_id.is_some() {
                    new_tags_map.insert(normalized_name, tag);
                }
            } else {
                new_tags_map.insert(normalized_name, tag);
            }
        }
    }

    MergedExtractionResult {
        existing_tag_ids: existing_ids,
        new_tags: new_tags_map.into_values().collect(),
    }
}

/// Validate that tag IDs exist in the database
pub fn validate_tag_ids(conn: &Connection, tag_ids: &HashSet<String>) -> Vec<String> {
    tag_ids
        .iter()
        .filter(|id| {
            conn.query_row("SELECT 1 FROM tags WHERE id = ?1", [id], |_| Ok(true))
                .unwrap_or(false)
        })
        .cloned()
        .collect()
}

/// Create new tags from extraction suggestions
pub fn create_extracted_tags(
    conn: &Connection,
    new_tags: Vec<NewTagSuggestion>,
) -> Result<Vec<String>, String> {
    let mut created_tag_ids: Vec<String> = Vec::new();

    for tag_suggestion in new_tags {
        // 1. Check if tag with same name already exists (case-insensitive)
        let existing: Option<String> = conn
            .query_row(
                "SELECT id FROM tags WHERE LOWER(name) = LOWER(?1)",
                [&tag_suggestion.name],
                |row| row.get(0),
            )
            .ok();

        if let Some(existing_id) = existing {
            // Tag already exists, use it
            created_tag_ids.push(existing_id);
            continue;
        }

        // 2. Determine parent_id
        let parent_id = if let Some(pid) = &tag_suggestion.parent_id {
            // Validate parent exists
            let parent_exists: bool = conn
                .query_row("SELECT 1 FROM tags WHERE id = ?1", [pid], |_| Ok(true))
                .unwrap_or(false);

            if parent_exists {
                Some(pid.clone())
            } else {
                None
            }
        } else if !tag_suggestion.suggested_category.is_empty() {
            // Find or create top-level category
            let category_name = match tag_suggestion.suggested_category.as_str() {
                "location" => "Locations",
                "person" => "People",
                "organization" => "Organizations",
                "topic" => "Topics",
                "event" => "Events",
                _ => "Other",
            };

            // Find existing category
            let category_id: Option<String> = conn
                .query_row(
                    "SELECT id FROM tags WHERE LOWER(name) = LOWER(?1) AND parent_id IS NULL",
                    [category_name],
                    |row| row.get(0),
                )
                .ok();

            if let Some(cid) = category_id {
                Some(cid)
            } else {
                // Create the category
                let new_category_id = Uuid::new_v4().to_string();
                let now = Utc::now().to_rfc3339();
                conn.execute(
                    "INSERT INTO tags (id, name, parent_id, created_at) VALUES (?1, ?2, NULL, ?3)",
                    rusqlite::params![new_category_id, category_name, now],
                )
                .map_err(|e| format!("Failed to create category: {}", e))?;
                Some(new_category_id)
            }
        } else {
            None
        };

        // 3. Create the new tag
        let new_tag_id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO tags (id, name, parent_id, created_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![new_tag_id, tag_suggestion.name, parent_id, now],
        )
        .map_err(|e| format!("Failed to create tag: {}", e))?;

        created_tag_ids.push(new_tag_id);
    }

    Ok(created_tag_ids)
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

