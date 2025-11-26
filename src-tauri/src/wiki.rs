use crate::models::{ChunkWithContext, WikiArticle, WikiArticleWithCitations, WikiCitation};
use chrono::Utc;
use reqwest::Client;
use rusqlite::{Connection, params_from_iter};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use regex::Regex;

// OpenRouter API request/response types (similar to extraction.rs)
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

#[derive(Deserialize)]
struct WikiGenerationResult {
    article_content: String,
    #[allow(dead_code)]
    citations_used: Vec<i32>,
}

const WIKI_GENERATION_SYSTEM_PROMPT: &str = r#"You are synthesizing a wiki article based on the user's personal knowledge base. Write a well-structured, informative article that summarizes what is known about the topic.

Guidelines:
- Use markdown formatting with ## for main sections and ### for subsections
- Every factual claim MUST have a citation using [N] notation
- Place citations immediately after the relevant statement
- If sources contain contradictions, note them
- Structure logically: overview first, then thematic sections
- Keep tone informative and neutral
- Do not invent information not present in the sources"#;

const WIKI_UPDATE_SYSTEM_PROMPT: &str = r#"You are updating an existing wiki article with new information from additional sources. Integrate the new information naturally into the existing article.

Guidelines:
- Maintain the existing structure where sensible
- Add new sections if needed for new topics
- Do not remove existing content unless directly contradicted by new sources
- Use [N] notation for citations, continuing from the existing numbering
- Every new factual claim MUST have a citation
- Keep tone consistent with the existing article"#;

/// Data needed for wiki article generation (extracted before async call)
pub struct WikiGenerationInput {
    pub chunks: Vec<ChunkWithContext>,
    pub atom_count: i32,
    pub tag_id: String,
    pub tag_name: String,
}

/// Data needed for wiki article update (extracted before async call)
pub struct WikiUpdateInput {
    pub new_chunks: Vec<ChunkWithContext>,
    pub existing_article: WikiArticle,
    pub existing_citations: Vec<WikiCitation>,
    pub atom_count: i32,
    pub tag_id: String,
    pub tag_name: String,
}

/// Prepare data for wiki article generation
/// Restructured to avoid holding db connection across await
pub async fn prepare_wiki_generation(
    db: &crate::db::Database,
    api_key: &str,
    tag_id: &str,
    tag_name: &str,
) -> Result<WikiGenerationInput, String> {
    // Generate embedding for tag name first (no db lock)
    let client = reqwest::Client::new();
    let embeddings = crate::embedding::generate_openrouter_embeddings_public(
        &client,
        api_key,
        &vec![tag_name.to_string()],
    )
    .await
    .map_err(|e| format!("Failed to generate tag embedding: {}", e))?;

    let tag_embedding = crate::embedding::f32_vec_to_blob_public(&embeddings[0]);

    // Now get chunks from database with the pre-generated embedding
    let (chunks, atom_count) = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;

        let chunks = get_relevant_chunks_for_article_sync(&conn, &tag_embedding, tag_id, 30, 0.3)?;

        if chunks.is_empty() {
            return Err("No content found for this tag".to_string());
        }

        let atom_count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM atom_tags WHERE tag_id = ?1",
                [tag_id],
                |row| row.get(0),
            )
            .map_err(|e| format!("Failed to count atoms: {}", e))?;

        (chunks, atom_count)
    };

    Ok(WikiGenerationInput {
        chunks,
        atom_count,
        tag_id: tag_id.to_string(),
        tag_name: tag_name.to_string(),
    })
}

/// Prepare data for wiki article update (sync, needs db connection)
pub fn prepare_wiki_update(
    conn: &Connection,
    tag_id: &str,
    tag_name: &str,
    existing_article: &WikiArticle,
    existing_citations: &[WikiCitation],
) -> Result<Option<WikiUpdateInput>, String> {
    let last_update = &existing_article.updated_at;

    // Get atoms added after the last update
    let mut new_atom_stmt = conn
        .prepare(
            "SELECT DISTINCT a.id FROM atoms a
             INNER JOIN atom_tags at ON a.id = at.atom_id
             WHERE at.tag_id = ?1 AND a.created_at > ?2"
        )
        .map_err(|e| format!("Failed to prepare new atoms query: {}", e))?;

    let new_atom_ids: Vec<String> = new_atom_stmt
        .query_map(rusqlite::params![tag_id, last_update], |row| row.get(0))
        .map_err(|e| format!("Failed to query new atoms: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect new atom IDs: {}", e))?;

    if new_atom_ids.is_empty() {
        return Ok(None); // No new atoms
    }

    // Get chunks from new atoms only
    let placeholders: String = new_atom_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let query = format!(
        "SELECT id, atom_id, chunk_index, content FROM atom_chunks WHERE atom_id IN ({})",
        placeholders
    );
    
    let mut chunk_stmt = conn
        .prepare(&query)
        .map_err(|e| format!("Failed to prepare chunk query: {}", e))?;
    
    let new_chunks: Vec<ChunkWithContext> = chunk_stmt
        .query_map(rusqlite::params_from_iter(new_atom_ids.iter()), |row| {
            Ok(ChunkWithContext {
                atom_id: row.get(1)?,
                chunk_index: row.get(2)?,
                content: row.get(3)?,
                similarity_score: 1.0,
            })
        })
        .map_err(|e| format!("Failed to query new chunks: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect new chunks: {}", e))?;

    if new_chunks.is_empty() {
        return Ok(None);
    }

    let atom_count: i32 = conn
        .query_row(
            "SELECT COUNT(*) FROM atom_tags WHERE tag_id = ?1",
            [tag_id],
            |row| row.get(0),
        )
        .map_err(|e| format!("Failed to count atoms: {}", e))?;

    Ok(Some(WikiUpdateInput {
        new_chunks,
        existing_article: existing_article.clone(),
        existing_citations: existing_citations.to_vec(),
        atom_count,
        tag_id: tag_id.to_string(),
        tag_name: tag_name.to_string(),
    }))
}

/// Generate wiki article content via API (async, no db needed)
pub async fn generate_wiki_content(
    client: &Client,
    api_key: &str,
    input: &WikiGenerationInput,
) -> Result<WikiArticleWithCitations, String> {
    // Build source materials for prompt
    let mut source_materials = String::new();
    for (i, chunk) in input.chunks.iter().enumerate() {
        source_materials.push_str(&format!("[{}] {}\n\n", i + 1, chunk.content));
    }

    let user_content = format!(
        "Write a wiki article about \"{}\".\n\nSOURCE MATERIALS:\n{}\nWrite the article now, citing sources with [N] notation.",
        input.tag_name,
        source_materials
    );

    // Call OpenRouter API
    let result = call_openrouter_for_wiki(client, api_key, WIKI_GENERATION_SYSTEM_PROMPT, &user_content).await?;

    // Create article
    let article_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    let article = WikiArticle {
        id: article_id.clone(),
        tag_id: input.tag_id.clone(),
        content: result.article_content.clone(),
        created_at: now.clone(),
        updated_at: now,
        atom_count: input.atom_count,
    };

    // Extract citations from the article content
    let citations = extract_citations(&article_id, &result.article_content, &input.chunks)?;

    Ok(WikiArticleWithCitations { article, citations })
}

/// Update wiki article content via API (async, no db needed)
pub async fn update_wiki_content(
    client: &Client,
    api_key: &str,
    input: &WikiUpdateInput,
) -> Result<WikiArticleWithCitations, String> {
    // Build existing sources section
    let mut existing_sources = String::new();
    for citation in &input.existing_citations {
        existing_sources.push_str(&format!("[{}] {}\n\n", citation.citation_index, citation.excerpt));
    }

    // Build new sources section (continuing numbering)
    let start_index = input.existing_citations.len() as i32 + 1;
    let mut new_sources = String::new();
    for (i, chunk) in input.new_chunks.iter().enumerate() {
        new_sources.push_str(&format!("[{}] {}\n\n", start_index + i as i32, chunk.content));
    }

    let user_content = format!(
        "CURRENT ARTICLE:\n{}\n\nEXISTING SOURCES (already cited as [1] through [{}]):\n{}\nNEW SOURCES TO INCORPORATE (cite as [{}] onwards):\n{}\nUpdate the article to incorporate the new information.",
        input.existing_article.content,
        input.existing_citations.len(),
        existing_sources,
        start_index,
        new_sources
    );

    // Call OpenRouter API
    let result = call_openrouter_for_wiki(client, api_key, WIKI_UPDATE_SYSTEM_PROMPT, &user_content).await?;

    // Create updated article
    let now = Utc::now().to_rfc3339();
    let article = WikiArticle {
        id: input.existing_article.id.clone(),
        tag_id: input.tag_id.clone(),
        content: result.article_content.clone(),
        created_at: input.existing_article.created_at.clone(),
        updated_at: now,
        atom_count: input.atom_count,
    };

    // Extract all citations from the updated content
    // Combine existing chunks with new chunks for citation mapping
    let mut all_chunks: Vec<ChunkWithContext> = input.existing_citations
        .iter()
        .map(|c| ChunkWithContext {
            atom_id: c.atom_id.clone(),
            chunk_index: c.chunk_index.unwrap_or(0),
            content: c.excerpt.clone(),
            similarity_score: 1.0,
        })
        .collect();
    all_chunks.extend(input.new_chunks.clone());

    let citations = extract_citations(&article.id, &result.article_content, &all_chunks)?;

    Ok(WikiArticleWithCitations { article, citations })
}

/// Get relevant chunks (sync version that takes pre-generated embedding)
fn get_relevant_chunks_for_article_sync(
    conn: &Connection,
    tag_embedding: &[u8],
    tag_id: &str,
    max_chunks: usize,
    similarity_threshold: f32,
) -> Result<Vec<ChunkWithContext>, String> {

    // 2. Get all descendant tag IDs (including the tag itself)
    let mut all_tag_ids = vec![tag_id.to_string()];
    let mut to_process = vec![tag_id.to_string()];

    while let Some(current_id) = to_process.pop() {
        let mut child_stmt = conn
            .prepare("SELECT id FROM tags WHERE parent_id = ?1")
            .map_err(|e| format!("Failed to prepare child query: {}", e))?;

        let children: Vec<String> = child_stmt
            .query_map([&current_id], |row| row.get(0))
            .map_err(|e| format!("Failed to query children: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect children: {}", e))?;

        for child_id in children {
            all_tag_ids.push(child_id.clone());
            to_process.push(child_id);
        }
    }

    // 3. Get all atom IDs with any of these tags (deduplicated)
    let tag_placeholders = all_tag_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let atom_query = format!(
        "SELECT DISTINCT atom_id FROM atom_tags WHERE tag_id IN ({})",
        tag_placeholders
    );

    let mut atom_stmt = conn
        .prepare(&atom_query)
        .map_err(|e| format!("Failed to prepare atom query: {}", e))?;

    let atom_ids: Vec<String> = atom_stmt
        .query_map(rusqlite::params_from_iter(all_tag_ids.iter()), |row| row.get(0))
        .map_err(|e| format!("Failed to query atoms: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect atom IDs: {}", e))?;

    if atom_ids.is_empty() {
        return Ok(Vec::new());
    }

    // 3. Get all chunk IDs for these atoms
    let placeholders: String = atom_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let query = format!(
        "SELECT id, atom_id, chunk_index, content FROM atom_chunks WHERE atom_id IN ({})",
        placeholders
    );
    
    let mut chunk_stmt = conn
        .prepare(&query)
        .map_err(|e| format!("Failed to prepare chunk query: {}", e))?;
    
    let chunks: Vec<(String, String, i32, String)> = chunk_stmt
        .query_map(rusqlite::params_from_iter(atom_ids.iter()), |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .map_err(|e| format!("Failed to query chunks: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect chunks: {}", e))?;

    if chunks.is_empty() {
        return Ok(Vec::new());
    }

    // 4. Query vec_chunks for similarity to tag name embedding
    // We need to get similarity scores for all chunks
    let mut results: Vec<ChunkWithContext> = Vec::new();
    
    let mut vec_stmt = conn
        .prepare(
            "SELECT chunk_id, distance 
             FROM vec_chunks 
             WHERE embedding MATCH ?1
             ORDER BY distance
             LIMIT ?2",
        )
        .map_err(|e| format!("Failed to prepare vec query: {}", e))?;

    let similar_chunks: Vec<(String, f32)> = vec_stmt
        .query_map(rusqlite::params![&tag_embedding, max_chunks * 3], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .map_err(|e| format!("Failed to query similar chunks: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect similar chunks: {}", e))?;

    // 5. Filter to only chunks from our tagged atoms and apply threshold
    let chunk_map: std::collections::HashMap<String, (String, i32, String)> = chunks
        .into_iter()
        .map(|(id, atom_id, chunk_index, content)| (id, (atom_id, chunk_index, content)))
        .collect();

    for (chunk_id, distance) in similar_chunks {
        let similarity = distance_to_similarity(distance);
        
        if similarity < similarity_threshold {
            continue;
        }

        if let Some((atom_id, chunk_index, content)) = chunk_map.get(&chunk_id) {
            results.push(ChunkWithContext {
                atom_id: atom_id.clone(),
                chunk_index: *chunk_index,
                content: content.clone(),
                similarity_score: similarity,
            });
        }

        if results.len() >= max_chunks {
            break;
        }
    }

    // Sort by similarity score descending
    results.sort_by(|a, b| b.similarity_score.partial_cmp(&a.similarity_score).unwrap_or(std::cmp::Ordering::Equal));

    Ok(results)
}

/// Convert distance to similarity score (0-1 scale)
fn distance_to_similarity(distance: f32) -> f32 {
    (1.0 - (distance / 2.0)).max(0.0).min(1.0)
}

/// Generate a wiki article for a tag
pub async fn generate_wiki_article(
    conn: &Connection,
    client: &Client,
    api_key: &str,
    tag_id: &str,
    tag_name: &str,
) -> Result<WikiArticleWithCitations, String> {
    // Generate embedding for tag name
    let embeddings = crate::embedding::generate_openrouter_embeddings_public(
        client,
        api_key,
        &vec![tag_name.to_string()],
    )
    .await
    .map_err(|e| format!("Failed to generate tag embedding: {}", e))?;

    let tag_embedding = crate::embedding::f32_vec_to_blob_public(&embeddings[0]);

    // Get relevant chunks with pre-generated embedding
    let chunks = get_relevant_chunks_for_article_sync(conn, &tag_embedding, tag_id, 30, 0.3)?;
    
    if chunks.is_empty() {
        return Err("No content found for this tag".to_string());
    }

    // Build source materials for prompt
    let mut source_materials = String::new();
    for (i, chunk) in chunks.iter().enumerate() {
        source_materials.push_str(&format!("[{}] {}\n\n", i + 1, chunk.content));
    }

    let user_content = format!(
        "Write a wiki article about \"{}\".\n\nSOURCE MATERIALS:\n{}\nWrite the article now, citing sources with [N] notation.",
        tag_name,
        source_materials
    );

    // Call OpenRouter API
    let result = call_openrouter_for_wiki(client, api_key, WIKI_GENERATION_SYSTEM_PROMPT, &user_content).await?;

    // Get current atom count for this tag (including child tags)
    // Get all descendant tag IDs (including the tag itself)
    let mut all_tag_ids = vec![tag_id.to_string()];
    let mut to_process = vec![tag_id.to_string()];

    while let Some(current_id) = to_process.pop() {
        let children: Vec<String> = conn
            .prepare("SELECT id FROM tags WHERE parent_id = ?1")
            .and_then(|mut stmt| {
                stmt.query_map([&current_id], |row| row.get(0))?
                    .collect::<Result<Vec<String>, _>>()
            })
            .map_err(|e| format!("Failed to get child tags: {}", e))?;

        for child_id in children {
            all_tag_ids.push(child_id.clone());
            to_process.push(child_id);
        }
    }

    // Count distinct atoms across this tag and all descendants
    let tag_placeholders = all_tag_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let count_query = format!(
        "SELECT COUNT(DISTINCT atom_id) FROM atom_tags WHERE tag_id IN ({})",
        tag_placeholders
    );

    let atom_count: i32 = conn
        .query_row(&count_query, params_from_iter(all_tag_ids.iter()), |row| row.get(0))
        .map_err(|e| format!("Failed to count atoms: {}", e))?;

    // Create article
    let article_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    let article = WikiArticle {
        id: article_id.clone(),
        tag_id: tag_id.to_string(),
        content: result.article_content.clone(),
        created_at: now.clone(),
        updated_at: now,
        atom_count,
    };

    // Extract citations from the article content
    let citations = extract_citations(&article_id, &result.article_content, &chunks)?;

    Ok(WikiArticleWithCitations { article, citations })
}

/// Update an existing wiki article with new atoms
pub async fn update_wiki_article(
    conn: &Connection,
    client: &Client,
    api_key: &str,
    tag_id: &str,
    _tag_name: &str,
    existing_article: &WikiArticle,
    existing_citations: &[WikiCitation],
) -> Result<WikiArticleWithCitations, String> {
    // Get the timestamp of the last update
    let last_update = &existing_article.updated_at;

    // Get atoms added after the last update
    let mut new_atom_stmt = conn
        .prepare(
            "SELECT DISTINCT a.id FROM atoms a
             INNER JOIN atom_tags at ON a.id = at.atom_id
             WHERE at.tag_id = ?1 AND a.created_at > ?2"
        )
        .map_err(|e| format!("Failed to prepare new atoms query: {}", e))?;

    let new_atom_ids: Vec<String> = new_atom_stmt
        .query_map(rusqlite::params![tag_id, last_update], |row| row.get(0))
        .map_err(|e| format!("Failed to query new atoms: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect new atom IDs: {}", e))?;

    if new_atom_ids.is_empty() {
        // No new atoms, return existing article
        return Ok(WikiArticleWithCitations {
            article: existing_article.clone(),
            citations: existing_citations.to_vec(),
        });
    }

    // Get chunks from new atoms only
    let placeholders: String = new_atom_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let query = format!(
        "SELECT id, atom_id, chunk_index, content FROM atom_chunks WHERE atom_id IN ({})",
        placeholders
    );
    
    let mut chunk_stmt = conn
        .prepare(&query)
        .map_err(|e| format!("Failed to prepare chunk query: {}", e))?;
    
    let new_chunks: Vec<ChunkWithContext> = chunk_stmt
        .query_map(rusqlite::params_from_iter(new_atom_ids.iter()), |row| {
            Ok(ChunkWithContext {
                atom_id: row.get(1)?,
                chunk_index: row.get(2)?,
                content: row.get(3)?,
                similarity_score: 1.0, // All new chunks are relevant
            })
        })
        .map_err(|e| format!("Failed to query new chunks: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect new chunks: {}", e))?;

    if new_chunks.is_empty() {
        return Ok(WikiArticleWithCitations {
            article: existing_article.clone(),
            citations: existing_citations.to_vec(),
        });
    }

    // Build existing sources section
    let mut existing_sources = String::new();
    for citation in existing_citations {
        existing_sources.push_str(&format!("[{}] {}\n\n", citation.citation_index, citation.excerpt));
    }

    // Build new sources section (continuing numbering)
    let start_index = existing_citations.len() as i32 + 1;
    let mut new_sources = String::new();
    for (i, chunk) in new_chunks.iter().enumerate() {
        new_sources.push_str(&format!("[{}] {}\n\n", start_index + i as i32, chunk.content));
    }

    let user_content = format!(
        "CURRENT ARTICLE:\n{}\n\nEXISTING SOURCES (already cited as [1] through [{}]):\n{}\nNEW SOURCES TO INCORPORATE (cite as [{}] onwards):\n{}\nUpdate the article to incorporate the new information.",
        existing_article.content,
        existing_citations.len(),
        existing_sources,
        start_index,
        new_sources
    );

    // Call OpenRouter API
    let result = call_openrouter_for_wiki(client, api_key, WIKI_UPDATE_SYSTEM_PROMPT, &user_content).await?;

    // Get current atom count
    let atom_count: i32 = conn
        .query_row(
            "SELECT COUNT(*) FROM atom_tags WHERE tag_id = ?1",
            [tag_id],
            |row| row.get(0),
        )
        .map_err(|e| format!("Failed to count atoms: {}", e))?;

    // Create updated article
    let now = Utc::now().to_rfc3339();
    let article = WikiArticle {
        id: existing_article.id.clone(),
        tag_id: tag_id.to_string(),
        content: result.article_content.clone(),
        created_at: existing_article.created_at.clone(),
        updated_at: now,
        atom_count,
    };

    // Extract all citations from the updated content
    // Combine existing chunks with new chunks for citation mapping
    let mut all_chunks: Vec<ChunkWithContext> = existing_citations
        .iter()
        .map(|c| ChunkWithContext {
            atom_id: c.atom_id.clone(),
            chunk_index: c.chunk_index.unwrap_or(0),
            content: c.excerpt.clone(),
            similarity_score: 1.0,
        })
        .collect();
    all_chunks.extend(new_chunks);

    let citations = extract_citations(&article.id, &result.article_content, &all_chunks)?;

    Ok(WikiArticleWithCitations { article, citations })
}

/// Call OpenRouter API for wiki generation
async fn call_openrouter_for_wiki(
    client: &Client,
    api_key: &str,
    system_prompt: &str,
    user_content: &str,
) -> Result<WikiGenerationResult, String> {
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "article_content": {
                "type": "string",
                "description": "The full wiki article in markdown format with [N] citations"
            },
            "citations_used": {
                "type": "array",
                "items": { "type": "integer" },
                "description": "List of citation numbers actually used in the article"
            }
        },
        "required": ["article_content", "citations_used"],
        "additionalProperties": false
    });

    let request = OpenRouterRequest {
        model: "anthropic/claude-sonnet-4.5".to_string(),
        messages: vec![
            Message {
                role: "system".to_string(),
                content: system_prompt.to_string(),
            },
            Message {
                role: "user".to_string(),
                content: user_content.to_string(),
            },
        ],
        response_format: ResponseFormat {
            format_type: "json_schema".to_string(),
            json_schema: JsonSchemaWrapper {
                name: "wiki_generation_result".to_string(),
                strict: true,
                schema,
            },
        },
        temperature: 0.3,
        max_tokens: 4000,
        provider: ProviderPreferences {
            require_parameters: true,
        },
    };

    // Retry logic with exponential backoff
    let mut last_error = String::new();
    for attempt in 0..3 {
        if attempt > 0 {
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
                    
                    let openrouter_response: OpenRouterResponse = serde_json::from_str(&body)
                        .map_err(|e| format!("Failed to parse OpenRouter response: {} - Body: {}", e, body))?;
                    
                    if let Some(choice) = openrouter_response.choices.first() {
                        if let Some(content) = &choice.message.content {
                            let result: WikiGenerationResult = serde_json::from_str(content)
                                .map_err(|e| format!("Failed to parse wiki result: {} - Content: {}", e, content))?;
                            return Ok(result);
                        }
                    }
                    return Err("No content in response".to_string());
                } else if resp.status().as_u16() == 429 {
                    last_error = "Rate limited".to_string();
                    continue;
                } else {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    last_error = format!("API error ({}): {}", status, body);
                    break;
                }
            }
            Err(e) => {
                last_error = format!("Network error: {}", e);
                continue;
            }
        }
    }

    Err(last_error)
}

/// Extract citations from article content and map to source chunks
fn extract_citations(
    _article_id: &str,
    content: &str,
    chunks: &[ChunkWithContext],
) -> Result<Vec<WikiCitation>, String> {
    let re = Regex::new(r"\[(\d+)\]").map_err(|e| format!("Failed to compile regex: {}", e))?;
    
    let mut citations: Vec<WikiCitation> = Vec::new();
    let mut seen_indices: std::collections::HashSet<i32> = std::collections::HashSet::new();

    for cap in re.captures_iter(content) {
        if let Some(num_match) = cap.get(1) {
            if let Ok(index) = num_match.as_str().parse::<i32>() {
                // Skip if we've already processed this citation index
                if seen_indices.contains(&index) {
                    continue;
                }
                seen_indices.insert(index);

                // Map to chunk (1-indexed)
                let chunk_idx = (index - 1) as usize;
                if chunk_idx < chunks.len() {
                    let chunk = &chunks[chunk_idx];
                    // Truncate excerpt to ~300 chars
                    let excerpt = if chunk.content.len() > 300 {
                        format!("{}...", &chunk.content[..297])
                    } else {
                        chunk.content.clone()
                    };

                    citations.push(WikiCitation {
                        id: Uuid::new_v4().to_string(),
                        citation_index: index,
                        atom_id: chunk.atom_id.clone(),
                        chunk_index: Some(chunk.chunk_index),
                        excerpt,
                    });
                }
            }
        }
    }

    // Sort by citation index
    citations.sort_by_key(|c| c.citation_index);

    Ok(citations)
}

/// Save a wiki article and its citations to the database
pub fn save_wiki_article(
    conn: &Connection,
    article: &WikiArticle,
    citations: &[WikiCitation],
) -> Result<(), String> {
    // Delete existing article for this tag (if any)
    conn.execute("DELETE FROM wiki_articles WHERE tag_id = ?1", [&article.tag_id])
        .map_err(|e| format!("Failed to delete existing article: {}", e))?;

    // Insert new article
    conn.execute(
        "INSERT INTO wiki_articles (id, tag_id, content, created_at, updated_at, atom_count) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            &article.id,
            &article.tag_id,
            &article.content,
            &article.created_at,
            &article.updated_at,
            article.atom_count
        ],
    )
    .map_err(|e| format!("Failed to insert article: {}", e))?;

    // Insert citations
    for citation in citations {
        conn.execute(
            "INSERT INTO wiki_citations (id, wiki_article_id, citation_index, atom_id, chunk_index, excerpt) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                &citation.id,
                &article.id,
                citation.citation_index,
                &citation.atom_id,
                citation.chunk_index,
                &citation.excerpt
            ],
        )
        .map_err(|e| format!("Failed to insert citation: {}", e))?;
    }

    Ok(())
}

/// Load a wiki article with its citations from the database
pub fn load_wiki_article(conn: &Connection, tag_id: &str) -> Result<Option<WikiArticleWithCitations>, String> {
    // Get article
    let article: Option<WikiArticle> = conn
        .query_row(
            "SELECT id, tag_id, content, created_at, updated_at, atom_count FROM wiki_articles WHERE tag_id = ?1",
            [tag_id],
            |row| {
                Ok(WikiArticle {
                    id: row.get(0)?,
                    tag_id: row.get(1)?,
                    content: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                    atom_count: row.get(5)?,
                })
            },
        )
        .ok();

    let article = match article {
        Some(a) => a,
        None => return Ok(None),
    };

    // Get citations
    let mut stmt = conn
        .prepare(
            "SELECT id, citation_index, atom_id, chunk_index, excerpt FROM wiki_citations WHERE wiki_article_id = ?1 ORDER BY citation_index"
        )
        .map_err(|e| format!("Failed to prepare citations query: {}", e))?;

    let citations: Vec<WikiCitation> = stmt
        .query_map([&article.id], |row| {
            Ok(WikiCitation {
                id: row.get(0)?,
                citation_index: row.get(1)?,
                atom_id: row.get(2)?,
                chunk_index: row.get(3)?,
                excerpt: row.get(4)?,
            })
        })
        .map_err(|e| format!("Failed to query citations: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect citations: {}", e))?;

    Ok(Some(WikiArticleWithCitations { article, citations }))
}

/// Get the status of a wiki article for a tag
pub fn get_article_status(conn: &Connection, tag_id: &str) -> Result<crate::models::WikiArticleStatus, String> {
    // Get current atom count for this tag (including child tags)
    // Get all descendant tag IDs (including the tag itself)
    let mut all_tag_ids = vec![tag_id.to_string()];
    let mut to_process = vec![tag_id.to_string()];

    while let Some(current_id) = to_process.pop() {
        let children: Vec<String> = conn
            .prepare("SELECT id FROM tags WHERE parent_id = ?1")
            .and_then(|mut stmt| {
                stmt.query_map([&current_id], |row| row.get(0))?
                    .collect::<Result<Vec<String>, _>>()
            })
            .map_err(|e| format!("Failed to get child tags: {}", e))?;

        for child_id in children {
            all_tag_ids.push(child_id.clone());
            to_process.push(child_id);
        }
    }

    // Count distinct atoms across this tag and all descendants
    let tag_placeholders = all_tag_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let count_query = format!(
        "SELECT COUNT(DISTINCT atom_id) FROM atom_tags WHERE tag_id IN ({})",
        tag_placeholders
    );

    let current_atom_count: i32 = conn
        .query_row(&count_query, params_from_iter(all_tag_ids.iter()), |row| row.get(0))
        .map_err(|e| format!("Failed to count atoms: {}", e))?;

    // Get article info if exists
    let article_info: Option<(i32, String)> = conn
        .query_row(
            "SELECT atom_count, updated_at FROM wiki_articles WHERE tag_id = ?1",
            [tag_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok();

    match article_info {
        Some((article_atom_count, updated_at)) => {
            let new_atoms = (current_atom_count - article_atom_count).max(0);
            Ok(crate::models::WikiArticleStatus {
                has_article: true,
                article_atom_count,
                current_atom_count,
                new_atoms_available: new_atoms,
                updated_at: Some(updated_at),
            })
        }
        None => Ok(crate::models::WikiArticleStatus {
            has_article: false,
            article_atom_count: 0,
            current_atom_count,
            new_atoms_available: 0,
            updated_at: None,
        }),
    }
}

/// Delete a wiki article for a tag
pub fn delete_article(conn: &Connection, tag_id: &str) -> Result<(), String> {
    conn.execute("DELETE FROM wiki_articles WHERE tag_id = ?1", [tag_id])
        .map_err(|e| format!("Failed to delete article: {}", e))?;
    Ok(())
}

