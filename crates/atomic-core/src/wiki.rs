//! Wiki article synthesis
//!
//! This module handles generating and updating wiki articles for tags.

use crate::db::Database;
use crate::models::{
    ChunkWithContext, RelatedTag, SuggestedArticle, WikiArticle, WikiArticleSummary,
    WikiArticleStatus, WikiArticleWithCitations, WikiCitation, WikiLink,
};
use crate::providers::traits::LlmConfig;
use crate::providers::types::{GenerationParams, Message, StructuredOutputSchema};
use crate::providers::{get_llm_provider, ProviderConfig};

use chrono::Utc;
use regex::Regex;
use rusqlite::Connection;
use serde::Deserialize;
use uuid::Uuid;

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
- Do not invent information not present in the sources
- When mentioning topics that have their own articles in the knowledge base, use [[Topic Name]] wiki-link notation to cross-reference them
- Only use [[wiki links]] for topics listed in the EXISTING WIKI ARTICLES section provided
- Do not force wiki links where they don't fit naturally"#;

const WIKI_UPDATE_SYSTEM_PROMPT: &str = r#"You are updating an existing wiki article with new information from additional sources. Integrate the new information naturally into the existing article.

Guidelines:
- Maintain the existing structure where sensible
- Add new sections if needed for new topics
- Do not remove existing content unless directly contradicted by new sources
- Use [N] notation for citations, continuing from the existing numbering
- Every new factual claim MUST have a citation
- Keep tone consistent with the existing article
- When mentioning topics that have their own articles, use [[Topic Name]] wiki-link notation
- Only use [[wiki links]] for topics listed in the EXISTING WIKI ARTICLES section provided
- Do not force wiki links where they don't fit naturally"#;

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
}

/// Prepare data for wiki article generation
/// Fetches ALL chunks for atoms under the tag hierarchy so the article covers everything
pub async fn prepare_wiki_generation(
    db: &Database,
    _provider_config: &ProviderConfig,
    tag_id: &str,
    tag_name: &str,
) -> Result<WikiGenerationInput, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;

    // Get all descendant tag IDs (including the tag itself)
    let all_tag_ids = get_tag_hierarchy(&conn, tag_id)?;

    if all_tag_ids.is_empty() {
        return Err("No content found for this tag".to_string());
    }

    let placeholders = all_tag_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");

    // Fetch all chunks for atoms under this tag hierarchy
    let query = format!(
        "SELECT ac.atom_id, ac.chunk_index, ac.content
         FROM atom_chunks ac
         INNER JOIN atom_tags at ON ac.atom_id = at.atom_id
         WHERE at.tag_id IN ({})
         ORDER BY ac.atom_id, ac.chunk_index",
        placeholders
    );

    let mut stmt = conn.prepare(&query)
        .map_err(|e| format!("Failed to prepare chunks query: {}", e))?;

    let chunks: Vec<ChunkWithContext> = stmt
        .query_map(rusqlite::params_from_iter(all_tag_ids.iter()), |row| {
            Ok(ChunkWithContext {
                atom_id: row.get(0)?,
                chunk_index: row.get(1)?,
                content: row.get(2)?,
                similarity_score: 1.0,
            })
        })
        .map_err(|e| format!("Failed to query chunks: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect chunks: {}", e))?;

    if chunks.is_empty() {
        return Err("No content found for this tag".to_string());
    }

    let atom_count = count_atoms_with_tags(&conn, &all_tag_ids)?;

    Ok(WikiGenerationInput {
        chunks,
        atom_count,
        tag_id: tag_id.to_string(),
        tag_name: tag_name.to_string(),
    })
}

/// Get all tag IDs in hierarchy (tag + all descendants) using recursive CTE
fn get_tag_hierarchy(conn: &Connection, tag_id: &str) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare(
            "WITH RECURSIVE descendant_tags(id) AS (
                SELECT ?1
                UNION ALL
                SELECT t.id FROM tags t
                INNER JOIN descendant_tags dt ON t.parent_id = dt.id
            )
            SELECT id FROM descendant_tags",
        )
        .map_err(|e| format!("Failed to prepare hierarchy query: {}", e))?;

    let tag_ids: Vec<String> = stmt
        .query_map([tag_id], |row| row.get(0))
        .map_err(|e| format!("Failed to query hierarchy: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect hierarchy: {}", e))?;

    Ok(tag_ids)
}

/// Count atoms with any of the given tags
fn count_atoms_with_tags(conn: &Connection, tag_ids: &[String]) -> Result<i32, String> {
    let placeholders = tag_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let query = format!(
        "SELECT COUNT(DISTINCT atom_id) FROM atom_tags WHERE tag_id IN ({})",
        placeholders
    );
    conn.query_row(&query, rusqlite::params_from_iter(tag_ids), |row| {
        row.get(0)
    })
    .map_err(|e| format!("Failed to count atoms: {}", e))
}

/// Prepare data for wiki article update (sync, needs db connection)
pub fn prepare_wiki_update(
    conn: &Connection,
    tag_id: &str,
    _tag_name: &str,
    existing_article: &WikiArticle,
    existing_citations: &[WikiCitation],
) -> Result<Option<WikiUpdateInput>, String> {
    let last_update = &existing_article.updated_at;

    // Get atoms added after the last update
    let mut new_atom_stmt = conn
        .prepare(
            "SELECT DISTINCT a.id FROM atoms a
             INNER JOIN atom_tags at ON a.id = at.atom_id
             WHERE at.tag_id = ?1 AND a.created_at > ?2",
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
    let placeholders: String = new_atom_ids
        .iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(",");
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
    }))
}

/// Generate wiki article content via API (async, no db needed)
/// `existing_article_names` is a list of (tag_id, tag_name) for all tags that already have wiki articles,
/// used to instruct the LLM to create [[wiki links]] to related articles.
pub async fn generate_wiki_content(
    provider_config: &ProviderConfig,
    input: &WikiGenerationInput,
    model: &str,
    existing_article_names: &[(String, String)],
) -> Result<WikiArticleWithCitations, String> {
    // Build source materials for prompt
    let mut source_materials = String::new();
    for (i, chunk) in input.chunks.iter().enumerate() {
        source_materials.push_str(&format!("[{}] {}\n\n", i + 1, chunk.content));
    }

    // Build existing articles list for cross-linking
    let articles_section = if existing_article_names.is_empty() {
        String::new()
    } else {
        let names: Vec<&str> = existing_article_names
            .iter()
            .filter(|(tid, _)| tid != &input.tag_id) // Exclude self
            .map(|(_, name)| name.as_str())
            .collect();
        if names.is_empty() {
            String::new()
        } else {
            format!(
                "EXISTING WIKI ARTICLES IN THIS KNOWLEDGE BASE:\n{}\n\n",
                names.join(", ")
            )
        }
    };

    let user_content = format!(
        "Write a wiki article about \"{}\".\n\n{}\
         SOURCE MATERIALS:\n{}\
         Write the article now, citing sources with [N] notation.{}",
        input.tag_name,
        articles_section,
        source_materials,
        if articles_section.is_empty() {
            ""
        } else {
            " Use [[Article Name]] to link to other articles listed above where relevant."
        }
    );

    // Call LLM API
    let result =
        call_llm_for_wiki(provider_config, WIKI_GENERATION_SYSTEM_PROMPT, &user_content, model)
            .await?;

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
    provider_config: &ProviderConfig,
    input: &WikiUpdateInput,
    model: &str,
    existing_article_names: &[(String, String)],
) -> Result<WikiArticleWithCitations, String> {
    // Build existing sources section
    let mut existing_sources = String::new();
    for citation in &input.existing_citations {
        existing_sources.push_str(&format!(
            "[{}] {}\n\n",
            citation.citation_index, citation.excerpt
        ));
    }

    // Build new sources section (continuing numbering)
    let start_index = input.existing_citations.len() as i32 + 1;
    let mut new_sources = String::new();
    for (i, chunk) in input.new_chunks.iter().enumerate() {
        new_sources.push_str(&format!(
            "[{}] {}\n\n",
            start_index + i as i32,
            chunk.content
        ));
    }

    // Build existing articles list for cross-linking
    let articles_section = if existing_article_names.is_empty() {
        String::new()
    } else {
        let names: Vec<&str> = existing_article_names
            .iter()
            .filter(|(tid, _)| tid != &input.tag_id)
            .map(|(_, name)| name.as_str())
            .collect();
        if names.is_empty() {
            String::new()
        } else {
            format!(
                "\nEXISTING WIKI ARTICLES IN THIS KNOWLEDGE BASE:\n{}\n",
                names.join(", ")
            )
        }
    };

    let user_content = format!(
        "CURRENT ARTICLE:\n{}\n\nEXISTING SOURCES (already cited as [1] through [{}]):\n{}\nNEW SOURCES TO INCORPORATE (cite as [{}] onwards):\n{}{}\nUpdate the article to incorporate the new information.{}",
        input.existing_article.content,
        input.existing_citations.len(),
        existing_sources,
        start_index,
        new_sources,
        articles_section,
        if articles_section.is_empty() {
            ""
        } else {
            " Use [[Article Name]] to link to other articles listed above where relevant."
        }
    );

    // Call LLM API
    let result =
        call_llm_for_wiki(provider_config, WIKI_UPDATE_SYSTEM_PROMPT, &user_content, model).await?;

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
    let mut all_chunks: Vec<ChunkWithContext> = input
        .existing_citations
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

/// Call LLM provider for wiki generation
async fn call_llm_for_wiki(
    provider_config: &ProviderConfig,
    system_prompt: &str,
    user_content: &str,
    model: &str,
) -> Result<WikiGenerationResult, String> {
    let input_chars = user_content.len();
    eprintln!("[wiki] Starting generation with model={}, input_chars={}", model, input_chars);

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

    let messages = vec![Message::system(system_prompt), Message::user(user_content)];

    let llm_config = LlmConfig::new(model).with_params(
        GenerationParams::new()
            .with_temperature(0.3)
            .with_max_tokens(4000)
            .with_structured_output(StructuredOutputSchema::new(
                "wiki_generation_result",
                schema,
            )),
    );

    let provider = get_llm_provider(provider_config).map_err(|e| e.to_string())?;

    // Only retry on transient errors (rate limits, network). Never retry on
    // content/parse errors — those waste tokens on calls doomed to fail the same way.
    let max_retries = 2;
    let mut last_error = String::new();
    for attempt in 0..=max_retries {
        if attempt > 0 {
            let delay = 1u64 << attempt;
            eprintln!("[wiki] Retry attempt {}/{} after {}s delay (last error: {})", attempt, max_retries, delay, last_error);
            tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
        }

        let start = std::time::Instant::now();
        match provider.complete(&messages, &llm_config).await {
            Ok(response) => {
                let elapsed = start.elapsed();
                let content = &response.content;
                eprintln!("[wiki] LLM responded in {:.1}s, output_chars={}", elapsed.as_secs_f64(), content.len());

                if content.is_empty() {
                    eprintln!("[wiki] ERROR: LLM returned empty content");
                    return Err("LLM returned empty content".to_string());
                }

                // Parse the structured JSON response
                match serde_json::from_str::<WikiGenerationResult>(content) {
                    Ok(result) => {
                        eprintln!("[wiki] Successfully parsed article ({} chars, {} citations)", result.article_content.len(), result.citations_used.len());
                        return Ok(result);
                    }
                    Err(parse_err) => {
                        // Log the parse failure with enough context to debug, but don't retry —
                        // the same prompt will produce the same unparseable output.
                        let preview = if content.len() > 500 {
                            format!("{}...[truncated]", &content[..500])
                        } else {
                            content.clone()
                        };
                        eprintln!("[wiki] ERROR: Failed to parse LLM response as JSON: {}", parse_err);
                        eprintln!("[wiki] Response preview: {}", preview);
                        return Err(format!("Failed to parse wiki result: {}", parse_err));
                    }
                }
            }
            Err(e) => {
                let elapsed = start.elapsed();
                eprintln!("[wiki] LLM call failed after {:.1}s: {}", elapsed.as_secs_f64(), e);

                if e.is_retryable() && attempt < max_retries {
                    last_error = e.to_string();
                    continue;
                } else {
                    if !e.is_retryable() {
                        eprintln!("[wiki] Non-retryable error, giving up immediately");
                    } else {
                        eprintln!("[wiki] Max retries exhausted");
                    }
                    return Err(e.to_string());
                }
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
                    // Truncate excerpt to ~300 chars, respecting UTF-8 char boundaries
                    let excerpt = if chunk.content.len() > 300 {
                        // Find a safe character boundary near 297 bytes
                        let truncate_pos = chunk
                            .content
                            .char_indices()
                            .take_while(|(i, _)| *i < 297)
                            .last()
                            .map(|(i, c)| i + c.len_utf8())
                            .unwrap_or(0);
                        format!("{}...", &chunk.content[..truncate_pos])
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

/// Save a wiki article, its citations, and wiki links to the database
pub fn save_wiki_article(
    conn: &Connection,
    article: &WikiArticle,
    citations: &[WikiCitation],
    wiki_links: &[WikiLink],
) -> Result<(), String> {
    // Delete existing article for this tag (if any)
    conn.execute(
        "DELETE FROM wiki_articles WHERE tag_id = ?1",
        [&article.tag_id],
    )
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

    // Insert wiki links
    for link in wiki_links {
        conn.execute(
            "INSERT INTO wiki_links (id, source_article_id, target_tag_name, target_tag_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                &link.id,
                &article.id,
                &link.target_tag_name,
                &link.target_tag_id,
                Utc::now().to_rfc3339()
            ],
        )
        .map_err(|e| format!("Failed to insert wiki link: {}", e))?;
    }

    Ok(())
}

/// Load a wiki article with its citations from the database
pub fn load_wiki_article(
    conn: &Connection,
    tag_id: &str,
) -> Result<Option<WikiArticleWithCitations>, String> {
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
pub fn get_article_status(conn: &Connection, tag_id: &str) -> Result<WikiArticleStatus, String> {
    // Count distinct atoms across this tag and all descendants using recursive CTE
    let current_atom_count: i32 = conn
        .query_row(
            "WITH RECURSIVE descendant_tags(id) AS (
                SELECT ?1
                UNION ALL
                SELECT t.id FROM tags t
                INNER JOIN descendant_tags dt ON t.parent_id = dt.id
            )
            SELECT COUNT(DISTINCT atom_id) FROM atom_tags
            WHERE tag_id IN (SELECT id FROM descendant_tags)",
            [tag_id],
            |row| row.get(0),
        )
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
            Ok(WikiArticleStatus {
                has_article: true,
                article_atom_count,
                current_atom_count,
                new_atoms_available: new_atoms,
                updated_at: Some(updated_at),
            })
        }
        None => Ok(WikiArticleStatus {
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

/// Load all wiki articles with tag names for list view, sorted by importance
pub fn load_all_wiki_articles(conn: &Connection) -> Result<Vec<WikiArticleSummary>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT w.id, w.tag_id, t.name as tag_name, w.updated_at, w.atom_count,
                    (SELECT COUNT(*) FROM wiki_links wl WHERE wl.target_tag_id = w.tag_id) as inbound_links
             FROM wiki_articles w
             JOIN tags t ON w.tag_id = t.id
             ORDER BY inbound_links DESC, w.atom_count DESC, w.updated_at DESC",
        )
        .map_err(|e| format!("Failed to prepare wiki articles query: {}", e))?;

    let articles: Vec<WikiArticleSummary> = stmt
        .query_map([], |row| {
            Ok(WikiArticleSummary {
                id: row.get(0)?,
                tag_id: row.get(1)?,
                tag_name: row.get(2)?,
                updated_at: row.get(3)?,
                atom_count: row.get(4)?,
                inbound_links: row.get(5)?,
            })
        })
        .map_err(|e| format!("Failed to query wiki articles: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect wiki articles: {}", e))?;

    Ok(articles)
}

/// Get all tags that have wiki articles (for cross-linking during generation)
pub fn get_existing_article_names(conn: &Connection) -> Result<Vec<(String, String)>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT w.tag_id, t.name FROM wiki_articles w JOIN tags t ON w.tag_id = t.id",
        )
        .map_err(|e| format!("Failed to prepare article names query: {}", e))?;

    let names: Vec<(String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|e| format!("Failed to query article names: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect article names: {}", e))?;

    Ok(names)
}

/// Extract [[wiki links]] from article content and map to known tags
pub fn extract_wiki_links(
    article_id: &str,
    content: &str,
    known_tags: &[(String, String)], // (tag_id, tag_name)
) -> Vec<WikiLink> {
    let re = match Regex::new(r"\[\[([^\]]+)\]\]") {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };

    let mut links: Vec<WikiLink> = Vec::new();
    let mut seen_names: std::collections::HashSet<String> = std::collections::HashSet::new();

    for cap in re.captures_iter(content) {
        if let Some(name_match) = cap.get(1) {
            let link_name = name_match.as_str().trim().to_string();
            let lower_name = link_name.to_lowercase();

            // Deduplicate
            if seen_names.contains(&lower_name) {
                continue;
            }
            seen_names.insert(lower_name.clone());

            // Try to resolve to a known tag (case-insensitive)
            let resolved = known_tags
                .iter()
                .find(|(_, name)| name.to_lowercase() == lower_name);

            let (target_tag_id, has_article) = match resolved {
                Some((tag_id, _)) => (Some(tag_id.clone()), true),
                None => {
                    // Check if there's a tag with this name but no article
                    // We'll just store None for target_tag_id in this case
                    (None, false)
                }
            };

            links.push(WikiLink {
                id: Uuid::new_v4().to_string(),
                source_article_id: article_id.to_string(),
                target_tag_name: link_name,
                target_tag_id,
                has_article,
            });
        }
    }

    links
}

/// Load wiki links for an article (outgoing cross-references)
pub fn load_wiki_links(conn: &Connection, tag_id: &str) -> Result<Vec<WikiLink>, String> {
    // Resolve wiki links dynamically: if target_tag_id is NULL, try to match by name.
    // Also recheck has_article against current state (articles may have been created since link was stored).
    let mut stmt = conn
        .prepare(
            "SELECT wl.id, wl.source_article_id, wl.target_tag_name,
                    COALESCE(wl.target_tag_id, t.id) as resolved_tag_id,
                    CASE WHEN wa.id IS NOT NULL THEN 1 ELSE 0 END as has_article
             FROM wiki_links wl
             JOIN wiki_articles w ON wl.source_article_id = w.id
             LEFT JOIN tags t ON t.name = wl.target_tag_name COLLATE NOCASE AND wl.target_tag_id IS NULL
             LEFT JOIN wiki_articles wa ON wa.tag_id = COALESCE(wl.target_tag_id, t.id)
             WHERE w.tag_id = ?1",
        )
        .map_err(|e| format!("Failed to prepare wiki links query: {}", e))?;

    let links: Vec<WikiLink> = stmt
        .query_map([tag_id], |row| {
            Ok(WikiLink {
                id: row.get(0)?,
                source_article_id: row.get(1)?,
                target_tag_name: row.get(2)?,
                target_tag_id: row.get(3)?,
                has_article: row.get::<_, i32>(4)? == 1,
            })
        })
        .map_err(|e| format!("Failed to query wiki links: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect wiki links: {}", e))?;

    Ok(links)
}

/// Get tags related to a given tag, ranked by semantic connectivity
///
/// Uses three signals:
/// 1. Semantic edges between atoms in different tags (weight: 0.4)
/// 2. Shared atoms (tagged with both tags) (weight: 0.3)
/// 3. Tag centroid embedding similarity (weight: 0.3)
pub fn get_related_tags(
    conn: &Connection,
    tag_id: &str,
    limit: usize,
) -> Result<Vec<RelatedTag>, String> {
    use crate::embedding::distance_to_similarity;

    // Get all atom IDs in this tag's hierarchy
    let source_tag_ids = get_tag_hierarchy(conn, tag_id)?;
    if source_tag_ids.is_empty() {
        return Ok(Vec::new());
    }

    let placeholders = source_tag_ids
        .iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(",");

    // === Signal 1 & 2: Semantic edges + shared atoms (existing query) ===
    let query = format!(
        "WITH source_atoms AS (
            SELECT DISTINCT atom_id FROM atom_tags WHERE tag_id IN ({placeholders})
        ),
        cross_edges AS (
            SELECT
                at_target.tag_id as target_tag_id,
                COUNT(*) as edge_count,
                AVG(se.similarity_score) as avg_similarity
            FROM semantic_edges se
            JOIN source_atoms sa ON se.source_atom_id = sa.atom_id
            JOIN atom_tags at_target ON se.target_atom_id = at_target.atom_id
            WHERE at_target.tag_id NOT IN ({placeholders})
            GROUP BY at_target.tag_id

            UNION ALL

            SELECT
                at_target.tag_id as target_tag_id,
                COUNT(*) as edge_count,
                AVG(se.similarity_score) as avg_similarity
            FROM semantic_edges se
            JOIN source_atoms sa ON se.target_atom_id = sa.atom_id
            JOIN atom_tags at_target ON se.source_atom_id = at_target.atom_id
            WHERE at_target.tag_id NOT IN ({placeholders})
            GROUP BY at_target.tag_id
        ),
        aggregated_edges AS (
            SELECT target_tag_id, SUM(edge_count) as total_edges, AVG(avg_similarity) as avg_sim
            FROM cross_edges
            GROUP BY target_tag_id
        ),
        shared AS (
            SELECT at2.tag_id as target_tag_id, COUNT(DISTINCT at1.atom_id) as shared_count
            FROM atom_tags at1
            JOIN atom_tags at2 ON at1.atom_id = at2.atom_id
            WHERE at1.tag_id IN ({placeholders})
              AND at2.tag_id NOT IN ({placeholders})
            GROUP BY at2.tag_id
        )
        SELECT
            t.id as tag_id,
            t.name as tag_name,
            COALESCE(ae.total_edges, 0) as semantic_edges,
            COALESCE(ae.avg_sim, 0.0) as avg_similarity,
            COALESCE(s.shared_count, 0) as shared_atoms,
            CASE WHEN wa.id IS NOT NULL THEN 1 ELSE 0 END as has_article
        FROM tags t
        LEFT JOIN aggregated_edges ae ON t.id = ae.target_tag_id
        LEFT JOIN shared s ON t.id = s.target_tag_id
        LEFT JOIN wiki_articles wa ON t.id = wa.tag_id
        WHERE (ae.total_edges > 0 OR s.shared_count > 0)
          AND t.parent_id IS NOT NULL
        ORDER BY (COALESCE(ae.total_edges, 0) * COALESCE(ae.avg_sim, 0.0) * 0.4 + COALESCE(s.shared_count, 0) * 0.3) DESC
        LIMIT ?",
        placeholders = placeholders
    );

    // Build params: source_tag_ids repeated 5 times + limit
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    for _ in 0..5 {
        for id in &source_tag_ids {
            params.push(Box::new(id.clone()));
        }
    }
    params.push(Box::new(limit as i32));

    let mut stmt = conn
        .prepare(&query)
        .map_err(|e| format!("Failed to prepare related tags query: {}", e))?;

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();

    let mut tags: Vec<RelatedTag> = stmt
        .query_map(param_refs.as_slice(), |row| {
            let semantic_edges: i32 = row.get(2)?;
            let avg_similarity: f64 = row.get(3)?;
            let shared_atoms: i32 = row.get(4)?;
            let score = (semantic_edges as f64) * avg_similarity * 0.4
                + (shared_atoms as f64) * 0.3;
            Ok(RelatedTag {
                tag_id: row.get(0)?,
                tag_name: row.get(1)?,
                score,
                shared_atoms,
                semantic_edges,
                has_article: row.get::<_, i32>(5)? == 1,
            })
        })
        .map_err(|e| format!("Failed to query related tags: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect related tags: {}", e))?;

    // === Signal 3: Tag centroid embedding similarity ===
    // Look up source tag's centroid from tag_embeddings
    let source_embedding: Option<Vec<u8>> = conn
        .query_row(
            "SELECT embedding FROM tag_embeddings WHERE tag_id = ?1",
            [tag_id],
            |row| row.get(0),
        )
        .ok();

    if let Some(ref source_blob) = source_embedding {
        // Query vec_tags for similar tag centroids
        let centroid_limit = (limit * 3).max(30) as i32;
        let mut vec_stmt = conn
            .prepare(
                "SELECT tag_id, distance
                 FROM vec_tags
                 WHERE embedding MATCH ?1
                 ORDER BY distance
                 LIMIT ?2",
            )
            .map_err(|e| format!("Failed to prepare vec_tags query: {}", e))?;

        let centroid_results: Vec<(String, f32)> = vec_stmt
            .query_map(rusqlite::params![source_blob, centroid_limit], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .map_err(|e| format!("Failed to query vec_tags: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect vec_tags results: {}", e))?;

        // Build a set of source hierarchy tag IDs for exclusion
        let exclude_set: std::collections::HashSet<&str> =
            source_tag_ids.iter().map(|s| s.as_str()).collect();

        // Build a map of existing tags for score merging
        let mut tag_map: std::collections::HashMap<String, usize> =
            tags.iter().enumerate().map(|(i, t)| (t.tag_id.clone(), i)).collect();

        for (candidate_tag_id, distance) in centroid_results {
            // Skip self and hierarchy tags
            if exclude_set.contains(candidate_tag_id.as_str()) {
                continue;
            }

            let centroid_sim = distance_to_similarity(distance) as f64;
            if centroid_sim < 0.3 {
                continue;
            }

            let centroid_score = centroid_sim * 0.3;

            if let Some(&idx) = tag_map.get(&candidate_tag_id) {
                // Merge centroid score into existing entry
                tags[idx].score += centroid_score;
            } else {
                // New tag found only via centroid similarity — look up metadata
                let tag_info: Option<(String, bool)> = conn
                    .query_row(
                        "SELECT t.name, CASE WHEN wa.id IS NOT NULL THEN 1 ELSE 0 END
                         FROM tags t
                         LEFT JOIN wiki_articles wa ON t.id = wa.tag_id
                         WHERE t.id = ?1 AND t.parent_id IS NOT NULL",
                        [&candidate_tag_id],
                        |row| Ok((row.get(0)?, row.get::<_, i32>(1)? == 1)),
                    )
                    .ok();

                if let Some((tag_name, has_article)) = tag_info {
                    let new_idx = tags.len();
                    tags.push(RelatedTag {
                        tag_id: candidate_tag_id.clone(),
                        tag_name,
                        score: centroid_score,
                        shared_atoms: 0,
                        semantic_edges: 0,
                        has_article,
                    });
                    tag_map.insert(candidate_tag_id, new_idx);
                }
            }
        }
    }

    // Second pass: find tags whose names appear in this article's content.
    // This catches relationships that semantic edges miss (e.g. "South Africa" mentioned
    // in the "Africa" article). Cheap: one article's content vs N tag names.
    let content_tags = find_tags_mentioned_in_article(conn, tag_id, &source_tag_ids, limit)?;
    let existing_ids: std::collections::HashSet<String> =
        tags.iter().map(|t| t.tag_id.clone()).collect();
    for ct in content_tags {
        if !existing_ids.contains(&ct.tag_id) {
            tags.push(ct);
        }
    }

    // Re-sort by score and truncate
    tags.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    tags.truncate(limit);

    Ok(tags)
}

/// Find tags whose names appear in a wiki article's content
fn find_tags_mentioned_in_article(
    conn: &Connection,
    tag_id: &str,
    exclude_tag_ids: &[String],
    limit: usize,
) -> Result<Vec<RelatedTag>, String> {
    // Get article content
    let content: Option<String> = conn
        .query_row(
            "SELECT content FROM wiki_articles WHERE tag_id = ?1",
            [tag_id],
            |row| row.get(0),
        )
        .ok();

    let content = match content {
        Some(c) => c,
        None => return Ok(Vec::new()),
    };
    let content_lower = content.to_lowercase();

    // Get all candidate tags (not in source hierarchy, not root categories, name >= 3 chars)
    let placeholders = exclude_tag_ids
        .iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(",");
    let query = format!(
        "SELECT t.id, t.name,
                (SELECT COUNT(*) FROM atom_tags at WHERE at.tag_id = t.id) as atom_count,
                CASE WHEN wa.id IS NOT NULL THEN 1 ELSE 0 END as has_article
         FROM tags t
         LEFT JOIN wiki_articles wa ON t.id = wa.tag_id
         WHERE t.parent_id IS NOT NULL
           AND t.id NOT IN ({})
           AND length(t.name) >= 3
           AND t.name GLOB '*[^0-9]*'",
        placeholders
    );

    let mut stmt = conn
        .prepare(&query)
        .map_err(|e| format!("Failed to prepare content mention query: {}", e))?;

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = exclude_tag_ids
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();

    let mut mentioned: Vec<RelatedTag> = stmt
        .query_map(param_refs.as_slice(), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i32>(2)?,
                row.get::<_, i32>(3)? == 1,
            ))
        })
        .map_err(|e| format!("Failed to query content mentions: {}", e))?
        .filter_map(|r| r.ok())
        .filter(|(_, name, _, _)| {
            // Check if tag name appears as a whole word in content
            let name_lower = name.to_lowercase();
            if let Some(pos) = content_lower.find(&name_lower) {
                let before_ok = pos == 0
                    || !content_lower.as_bytes()[pos - 1].is_ascii_alphanumeric();
                let end = pos + name_lower.len();
                let after_ok = end >= content_lower.len()
                    || !content_lower.as_bytes()[end].is_ascii_alphanumeric();
                before_ok && after_ok
            } else {
                false
            }
        })
        .map(|(id, name, atom_count, has_article)| RelatedTag {
            tag_id: id,
            tag_name: name,
            score: atom_count as f64 * 0.1, // Lower score so semantic results rank higher
            shared_atoms: 0,
            semantic_edges: 0,
            has_article,
        })
        .collect();

    mentioned.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    mentioned.truncate(limit);

    Ok(mentioned)
}

/// Get suggested wiki articles: tags without articles ranked by demand + content richness
pub fn get_suggested_wiki_articles(
    conn: &Connection,
    limit: i32,
) -> Result<Vec<SuggestedArticle>, String> {
    let mut stmt = conn
        .prepare(
            "WITH candidate_tags AS (
                SELECT t.id, t.name
                FROM tags t
                WHERE t.parent_id IS NOT NULL
                  AND NOT EXISTS (SELECT 1 FROM wiki_articles wa WHERE wa.tag_id = t.id)
                  AND t.name GLOB '*[^0-9]*'
                  AND length(t.name) >= 2
            ),
            atom_counts AS (
                SELECT ct.id as tag_id, COUNT(DISTINCT at.atom_id) as atom_count
                FROM candidate_tags ct
                JOIN atom_tags at ON at.tag_id = ct.id
                GROUP BY ct.id
            ),
            link_mentions AS (
                SELECT ct.id as tag_id, COUNT(*) as link_count
                FROM candidate_tags ct
                JOIN wiki_links wl ON (wl.target_tag_id = ct.id OR wl.target_tag_name = ct.name COLLATE NOCASE)
                GROUP BY ct.id
            )
            SELECT
                ct.id,
                ct.name,
                COALESCE(ac.atom_count, 0) as atom_count,
                COALESCE(lm.link_count, 0) as mention_count,
                COALESCE(ac.atom_count, 0) * 1.0 + COALESCE(lm.link_count, 0) * 3.0 as score
            FROM candidate_tags ct
            LEFT JOIN atom_counts ac ON ct.id = ac.tag_id
            LEFT JOIN link_mentions lm ON ct.id = lm.tag_id
            WHERE COALESCE(ac.atom_count, 0) > 0
            ORDER BY score DESC
            LIMIT ?1",
        )
        .map_err(|e| format!("Failed to prepare suggestions query: {}", e))?;

    let suggestions: Vec<SuggestedArticle> = stmt
        .query_map([limit], |row| {
            Ok(SuggestedArticle {
                tag_id: row.get(0)?,
                tag_name: row.get(1)?,
                atom_count: row.get(2)?,
                mention_count: row.get(3)?,
                score: row.get(4)?,
            })
        })
        .map_err(|e| format!("Failed to query suggestions: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect suggestions: {}", e))?;

    Ok(suggestions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database as CoreDatabase;
    use tempfile::NamedTempFile;

    fn create_test_db() -> (CoreDatabase, NamedTempFile) {
        let temp_file = NamedTempFile::new().unwrap();
        let db = CoreDatabase::open_or_create(temp_file.path()).unwrap();
        (db, temp_file)
    }

    fn insert_tag(conn: &Connection, id: &str, name: &str) {
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO tags (id, name, created_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![id, name, now],
        )
        .unwrap();
    }

    fn insert_atom(conn: &Connection, id: &str) {
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO atoms (id, content, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, "test content", now, now],
        )
        .unwrap();
    }

    #[test]
    fn test_save_and_load_wiki_article() {
        let (db, _temp) = create_test_db();
        let conn = db.conn.lock().unwrap();

        // Create a tag and atom first
        insert_tag(&conn, "tag1", "TestTopic");
        insert_atom(&conn, "atom1");

        // Create article
        let now = chrono::Utc::now().to_rfc3339();
        let article = WikiArticle {
            id: "article1".to_string(),
            tag_id: "tag1".to_string(),
            content: "This is a test article with [1] citation.".to_string(),
            created_at: now.clone(),
            updated_at: now,
            atom_count: 5,
        };

        let citations = vec![WikiCitation {
            id: "citation1".to_string(),
            citation_index: 1,
            atom_id: "atom1".to_string(),
            chunk_index: Some(0),
            excerpt: "Source text here".to_string(),
        }];

        // Save
        save_wiki_article(&conn, &article, &citations, &[]).unwrap();

        // Load
        let loaded = load_wiki_article(&conn, "tag1").unwrap();
        assert!(loaded.is_some(), "Article should be found");

        let loaded = loaded.unwrap();
        assert_eq!(loaded.article.content, article.content);
        assert_eq!(loaded.citations.len(), 1);
        assert_eq!(loaded.citations[0].excerpt, "Source text here");
    }

    #[test]
    fn test_get_article_status_no_article() {
        let (db, _temp) = create_test_db();
        let conn = db.conn.lock().unwrap();

        // Create a tag without an article
        insert_tag(&conn, "tag1", "TestTopic");

        let status = get_article_status(&conn, "tag1").unwrap();

        assert!(!status.has_article, "Should have no article");
        assert_eq!(status.article_atom_count, 0);
        assert!(status.updated_at.is_none(), "Should have no update time");
    }

    #[test]
    fn test_extract_citations_basic() {
        let chunks = vec![
            ChunkWithContext {
                atom_id: "atom1".to_string(),
                chunk_index: 0,
                content: "First chunk content".to_string(),
                similarity_score: 0.9,
            },
            ChunkWithContext {
                atom_id: "atom2".to_string(),
                chunk_index: 0,
                content: "Second chunk content".to_string(),
                similarity_score: 0.85,
            },
        ];

        let content = "This is text [1] and more text [2].";
        let citations = extract_citations("article1", content, &chunks).unwrap();

        assert_eq!(citations.len(), 2, "Should find 2 citations");
        assert_eq!(citations[0].citation_index, 1);
        assert_eq!(citations[0].atom_id, "atom1");
        assert_eq!(citations[1].citation_index, 2);
        assert_eq!(citations[1].atom_id, "atom2");
    }

    #[test]
    fn test_extract_citations_deduplicates() {
        let chunks = vec![ChunkWithContext {
            atom_id: "atom1".to_string(),
            chunk_index: 0,
            content: "Chunk content".to_string(),
            similarity_score: 0.9,
        }];

        // Same citation appears multiple times
        let content = "Statement one [1] and statement two [1] and statement three [1].";
        let citations = extract_citations("article1", content, &chunks).unwrap();

        assert_eq!(
            citations.len(),
            1,
            "Should deduplicate repeated citation indices"
        );
        assert_eq!(citations[0].citation_index, 1);
    }
}
