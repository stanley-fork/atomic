//! Wiki article synthesis
//!
//! This module handles generating and updating wiki articles for tags.

use crate::db::Database;
use crate::models::{
    ChunkWithContext, WikiArticle, WikiArticleSummary, WikiArticleStatus, WikiArticleWithCitations,
    WikiCitation,
};
use crate::providers::traits::LlmConfig;
use crate::providers::types::{GenerationParams, Message, StructuredOutputSchema};
use crate::providers::{get_llm_provider, ProviderConfig};
use crate::search::{search_chunks, SearchMode, SearchOptions};
use chrono::Utc;
use regex::Regex;
use rusqlite::{params_from_iter, Connection};
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
}

/// Prepare data for wiki article generation
/// Uses hybrid search (keyword + semantic) scoped to the tag hierarchy
pub async fn prepare_wiki_generation(
    db: &Database,
    _provider_config: &ProviderConfig,
    tag_id: &str,
    tag_name: &str,
) -> Result<WikiGenerationInput, String> {
    // Get all descendant tag IDs (including the tag itself) for scoping
    let all_tag_ids = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        get_tag_hierarchy(&conn, tag_id)?
    };

    // Use hybrid search scoped to this tag hierarchy
    let options = SearchOptions::new(tag_name, SearchMode::Hybrid, 30)
        .with_threshold(0.3)
        .with_scope(all_tag_ids.clone());

    let chunk_results = search_chunks(db, options).await?;

    if chunk_results.is_empty() {
        return Err("No content found for this tag".to_string());
    }

    // Convert ChunkResult to ChunkWithContext
    let chunks: Vec<ChunkWithContext> = chunk_results
        .into_iter()
        .map(|cr| ChunkWithContext {
            atom_id: cr.atom_id,
            chunk_index: cr.chunk_index,
            content: cr.content,
            similarity_score: cr.score,
        })
        .collect();

    // Count atoms with this tag hierarchy
    let atom_count = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        count_atoms_with_tags(&conn, &all_tag_ids)?
    };

    Ok(WikiGenerationInput {
        chunks,
        atom_count,
        tag_id: tag_id.to_string(),
        tag_name: tag_name.to_string(),
    })
}

/// Get all tag IDs in hierarchy (tag + all descendants)
fn get_tag_hierarchy(conn: &Connection, tag_id: &str) -> Result<Vec<String>, String> {
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
    Ok(all_tag_ids)
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
pub async fn generate_wiki_content(
    provider_config: &ProviderConfig,
    input: &WikiGenerationInput,
    model: &str,
) -> Result<WikiArticleWithCitations, String> {
    // Build source materials for prompt
    let mut source_materials = String::new();
    for (i, chunk) in input.chunks.iter().enumerate() {
        source_materials.push_str(&format!("[{}] {}\n\n", i + 1, chunk.content));
    }

    let user_content = format!(
        "Write a wiki article about \"{}\".\n\nSOURCE MATERIALS:\n{}\nWrite the article now, citing sources with [N] notation.",
        input.tag_name, source_materials
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

    let user_content = format!(
        "CURRENT ARTICLE:\n{}\n\nEXISTING SOURCES (already cited as [1] through [{}]):\n{}\nNEW SOURCES TO INCORPORATE (cite as [{}] onwards):\n{}\nUpdate the article to incorporate the new information.",
        input.existing_article.content,
        input.existing_citations.len(),
        existing_sources,
        start_index,
        new_sources
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

    // Retry logic with exponential backoff
    let mut last_error = String::new();
    for attempt in 0..3 {
        if attempt > 0 {
            tokio::time::sleep(std::time::Duration::from_secs(1 << attempt)).await;
        }

        match provider.complete(&messages, &llm_config).await {
            Ok(response) => {
                let content = &response.content;
                if !content.is_empty() {
                    // Log the raw LLM output
                    eprintln!("=== WIKI GENERATION LLM OUTPUT ===");
                    eprintln!("{}", content);
                    eprintln!("==================================");

                    // Parse the wiki result from the content
                    let result: WikiGenerationResult = serde_json::from_str(content).map_err(
                        |e| format!("Failed to parse wiki result: {} - Content: {}", e, content),
                    )?;
                    return Ok(result);
                }
                return Err("No content in response".to_string());
            }
            Err(e) => {
                let err_str = e.to_string();
                if e.is_retryable() {
                    last_error = err_str;
                    continue;
                } else {
                    // Don't retry on non-retryable errors
                    last_error = err_str;
                    break;
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

/// Save a wiki article and its citations to the database
pub fn save_wiki_article(
    conn: &Connection,
    article: &WikiArticle,
    citations: &[WikiCitation],
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
    let tag_placeholders = all_tag_ids
        .iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(",");
    let count_query = format!(
        "SELECT COUNT(DISTINCT atom_id) FROM atom_tags WHERE tag_id IN ({})",
        tag_placeholders
    );

    let current_atom_count: i32 = conn
        .query_row(&count_query, params_from_iter(all_tag_ids.iter()), |row| {
            row.get(0)
        })
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

/// Load all wiki articles with tag names for list view
pub fn load_all_wiki_articles(conn: &Connection) -> Result<Vec<WikiArticleSummary>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT w.id, w.tag_id, t.name as tag_name, w.updated_at, w.atom_count
             FROM wiki_articles w
             JOIN tags t ON w.tag_id = t.id
             ORDER BY w.updated_at DESC",
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
            })
        })
        .map_err(|e| format!("Failed to query wiki articles: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect wiki articles: {}", e))?;

    Ok(articles)
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
        save_wiki_article(&conn, &article, &citations).unwrap();

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
