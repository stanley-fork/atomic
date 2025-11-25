# Atomic - Note-Taking Desktop Application

## Project Overview
Atomic is a Tauri v2 desktop application for note-taking with a React frontend. It features markdown editing, hierarchical tagging, AI-powered semantic search using local embeddings, automatic tag extraction, and wiki article synthesis using OpenRouter LLM.

## Current Status: Phase 4 Complete
Phase 4 (Wiki Synthesis) is complete with:
- Wiki article generation for tags using OpenRouter LLM
- Inline citations with [N] notation linking to source atoms
- Citation popovers showing excerpt text with "View full atom" links
- Incremental article updates when new atoms are added
- Article regeneration with confirmation dialog
- New atoms available banner with update button
- Wiki viewer in right drawer with empty, generating, and ready states

Phase 3 (Automatic Tag Extraction) features:
- OpenRouter API integration for LLM-powered tag extraction
- Settings UI for API key configuration and auto-tagging toggle
- Automatic tag extraction during the embedding pipeline
- New tags created with proper hierarchy (under category tags like "Locations", "People", "Topics", etc.)
- Tag tree auto-refresh when new tags are created
- Atoms list auto-refresh when tags are extracted

Phase 2.1 (Real sqlite-lembed Integration) features:
- Real 384-dimensional embeddings using sqlite-lembed and all-MiniLM-L6-v2 model
- sqlite-lembed extension loaded at runtime for each database connection
- Model registered in temp.lembed_models for embedding generation
- Semantic search uses real embeddings for query and content matching

Phase 2 (Embedding Pipeline) features:
- Async embedding generation when atoms are created or updated
- Content chunking algorithm for optimal embedding
- Semantic search using sqlite-vec for vector similarity
- Related atoms discovery based on content similarity
- Embedding status indicators on atom cards
- Real-time status updates via Tauri events

Phase 1 (Foundation + Data Layer) features:
- Full UI layout with left panel, main view, and right drawer
- SQLite database with sqlite-vec extension
- Complete CRUD operations for atoms and tags
- Markdown editing with CodeMirror and rendering with react-markdown
- Hierarchical tag navigation with context menus
- Grid and list view modes for atoms
- Dark theme (Obsidian-inspired)

## Tech Stack
- **Desktop Framework**: Tauri v2 (Rust backend)
- **Frontend**: React 18+ with TypeScript
- **Build Tool**: Vite 6
- **Styling**: Tailwind CSS v4 (using `@tailwindcss/vite` plugin)
- **State Management**: Zustand 5
- **Database**: SQLite with sqlite-vec and sqlite-lembed extensions (via rusqlite)
- **Embeddings**: Real 384-dimensional vectors via sqlite-lembed + all-MiniLM-L6-v2 GGUF model
- **LLM Provider**: OpenRouter API (anthropic/claude-sonnet-4.5 with structured outputs)
- **HTTP Client**: reqwest (Rust)
- **Markdown Editor**: CodeMirror 6 (`@uiw/react-codemirror`)
- **Markdown Rendering**: react-markdown with remark-gfm

## Project Structure
```
/src-tauri
  /src
    main.rs           # Tauri entry point
    lib.rs            # App setup, command registration, resource path resolution
    db.rs             # SQLite setup, migrations, sqlite-lembed loading, model registration
    commands.rs       # All Tauri command implementations
    models.rs         # Rust structs for data
    chunking.rs       # Content chunking algorithm
    embedding.rs      # Embedding generation + tag extraction pipeline
    extraction.rs     # OpenRouter API integration, tag extraction logic
    wiki.rs           # Wiki article generation and update logic
    settings.rs       # Settings CRUD operations
  /resources
    all-MiniLM-L6-v2.q8_0.gguf  # Bundled embedding model (~24MB, Q8_0 quantization)
    lembed0.so                   # sqlite-lembed extension (Linux x86_64)
    lembed0-aarch64.dylib        # sqlite-lembed extension (macOS Apple Silicon)
    lembed0-x86_64.dylib         # sqlite-lembed extension (macOS Intel)
  Cargo.toml
  tauri.conf.json

/src
  /components
    /layout           # LeftPanel, MainView, RightDrawer, Layout
    /atoms            # AtomCard, AtomEditor, AtomViewer, AtomGrid, AtomList, RelatedAtoms
    /tags             # TagTree, TagNode, TagChip, TagSelector
    /wiki             # WikiViewer, WikiArticleContent, WikiHeader, WikiEmptyState, WikiGenerating, CitationLink, CitationPopover
    /search           # SemanticSearch
    /settings         # SettingsModal, SettingsButton
    /ui               # Button, Input, Modal, FAB, ContextMenu
  /stores             # Zustand stores (atoms.ts, tags.ts, ui.ts, settings.ts, wiki.ts)
  /hooks              # Custom hooks (useClickOutside, useKeyboard, useEmbeddingEvents)
  /lib                # Utilities (tauri.ts, markdown.ts, date.ts)
  App.tsx
  main.tsx
  index.css           # Tailwind imports + custom animations

/index.html
/vite.config.ts
/package.json
```

## Common Commands

### Development
```bash
# Install dependencies
npm install

# Run development server (frontend only)
npm run dev

# Run development server (frontend + Tauri)
npm run tauri dev

# Build for production
npm run tauri build

# Type check
npm run build
```

### Rust Backend
```bash
# Check Rust code
cd src-tauri && cargo check

# Build Rust code
cd src-tauri && cargo build

# Run tests (including chunking tests)
cd src-tauri && cargo test
```

## Database

### Location
The SQLite database is stored in the Tauri app data directory:
- macOS: `~/Library/Application Support/com.atomic.app/atomic.db`
- Linux: `~/.local/share/com.atomic.app/atomic.db`
- Windows: `%APPDATA%/com.atomic.app/atomic.db`

### Schema
```sql
-- Core content units
CREATE TABLE atoms (
  id TEXT PRIMARY KEY,  -- UUID
  content TEXT NOT NULL,
  source_url TEXT,
  created_at TEXT NOT NULL,  -- ISO 8601
  updated_at TEXT NOT NULL,
  embedding_status TEXT DEFAULT 'pending'  -- 'pending', 'processing', 'complete', 'failed'
);

-- Hierarchical tags
CREATE TABLE tags (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  parent_id TEXT REFERENCES tags(id) ON DELETE SET NULL,
  created_at TEXT NOT NULL
);

-- Many-to-many relationship
CREATE TABLE atom_tags (
  atom_id TEXT REFERENCES atoms(id) ON DELETE CASCADE,
  tag_id TEXT REFERENCES tags(id) ON DELETE CASCADE,
  PRIMARY KEY (atom_id, tag_id)
);

-- Chunked content with embeddings
CREATE TABLE atom_chunks (
  id TEXT PRIMARY KEY,
  atom_id TEXT REFERENCES atoms(id) ON DELETE CASCADE,
  chunk_index INTEGER NOT NULL,
  content TEXT NOT NULL,
  embedding BLOB  -- 384-dimensional float vector from sqlite-lembed
);

-- Vector similarity search (sqlite-vec virtual table)
CREATE VIRTUAL TABLE vec_chunks USING vec0(
  chunk_id TEXT PRIMARY KEY,
  embedding float[384]
);

-- App settings (key-value store)
CREATE TABLE settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

-- Wiki articles for tags
CREATE TABLE wiki_articles (
  id TEXT PRIMARY KEY,
  tag_id TEXT UNIQUE REFERENCES tags(id) ON DELETE CASCADE,
  content TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  atom_count INTEGER NOT NULL
);

-- Citations linking article content to source atoms/chunks
CREATE TABLE wiki_citations (
  id TEXT PRIMARY KEY,
  wiki_article_id TEXT REFERENCES wiki_articles(id) ON DELETE CASCADE,
  citation_index INTEGER NOT NULL,
  atom_id TEXT REFERENCES atoms(id) ON DELETE CASCADE,
  chunk_index INTEGER,
  excerpt TEXT NOT NULL
);

-- Temporary table for sqlite-lembed model registration (per-connection)
-- temp.lembed_models(name TEXT, model BLOB)
```

### Settings Keys
- `openrouter_api_key`: User's OpenRouter API key for LLM access
- `auto_tagging_enabled`: "true" or "false" (default: "true")

## Tauri Commands (API)

### Atom Operations
- `get_all_atoms()` → `Vec<AtomWithTags>`
- `get_atom(id)` → `AtomWithTags`
- `create_atom(content, source_url?, tag_ids)` → `AtomWithTags` (triggers async embedding + tag extraction)
- `update_atom(id, content, source_url?, tag_ids)` → `AtomWithTags` (triggers async embedding + tag extraction)
- `delete_atom(id)` → `()`
- `get_atoms_by_tag(tag_id)` → `Vec<AtomWithTags>`

### Tag Operations
- `get_all_tags()` → `Vec<TagWithCount>` (hierarchical tree)
- `create_tag(name, parent_id?)` → `Tag`
- `update_tag(id, name, parent_id?)` → `Tag`
- `delete_tag(id)` → `()`

### Embedding Operations
- `find_similar_atoms(atom_id, limit, threshold)` → `Vec<SimilarAtomResult>`
- `search_atoms_semantic(query, limit, threshold)` → `Vec<SemanticSearchResult>`
- `retry_embedding(atom_id)` → `()` (retriggers embedding for failed atoms)
- `process_pending_embeddings()` → `i32` (processes all pending atoms, returns count)
- `get_embedding_status(atom_id)` → `String`

### Wiki Operations
- `get_wiki_article(tag_id)` → `Option<WikiArticleWithCitations>` (returns article with citations if exists)
- `get_wiki_article_status(tag_id)` → `WikiArticleStatus` (quick check: has_article, atom counts, updated_at)
- `generate_wiki_article(tag_id, tag_name)` → `WikiArticleWithCitations` (generates new article from scratch)
- `update_wiki_article(tag_id, tag_name)` → `WikiArticleWithCitations` (incrementally updates with new atoms)
- `delete_wiki_article(tag_id)` → `()` (deletes article and citations)

### Settings Operations
- `get_settings()` → `HashMap<String, String>` (all settings)
- `set_setting(key, value)` → `()` (upsert a setting)
- `test_openrouter_connection(apiKey)` → `Result<bool, String>` (validates API key)

### Utility
- `check_sqlite_vec()` → `String` (version check)

## Tauri Events

### embedding-complete
Emitted when an atom's embedding generation completes (success or failure).

Payload:
```typescript
{
  atom_id: string;
  status: 'complete' | 'failed';
  error?: string;
  tags_extracted: string[];      // IDs of all tags applied
  new_tags_created: string[];    // IDs of newly created tags
}
```

## Wiki Synthesis

### How It Works
1. User clicks the article icon next to a tag in the left panel
2. Right drawer opens in wiki mode for that tag
3. If no article exists, shows empty state with "Generate Article" button
4. Generation fetches relevant chunks from atoms with that tag
5. Chunks are ranked by embedding similarity to tag name
6. Top chunks are sent to OpenRouter LLM with generation prompt
7. LLM returns markdown article with [N] citations
8. Citations are extracted and mapped to source atoms/chunks
9. Article and citations are saved to database

### Incremental Updates
When new atoms are added after article generation:
1. Status check shows "X new atoms available" banner
2. Clicking "Update Article" fetches only new atoms' chunks
3. Existing article and new sources are sent to LLM with update prompt
4. LLM integrates new information, continuing citation numbering
5. Updated article replaces existing content

### Citation Interaction
- Citations appear as clickable [N] links inline in text
- Clicking opens a popover positioned near the citation
- Popover shows excerpt text (~300 chars max)
- "View full atom →" link opens atom in viewer mode
- Popover closes on click outside or Escape key

### Structured Outputs
Wiki generation uses OpenRouter's structured outputs:
- `response_format.type`: `"json_schema"` with strict validation
- Schema: `article_content` (string) and `citations_used` (array of integers)
- Temperature: 0.3 for consistent output
- Max tokens: 4000 for longer articles

## Automatic Tag Extraction

### How It Works
1. When an atom is created/updated, the embedding pipeline runs
2. If auto-tagging is enabled and API key is set, tag extraction runs in parallel with embedding
3. Each content chunk is sent to OpenRouter (Claude Sonnet 4.5) with the existing tag hierarchy
4. The LLM identifies existing tags that apply and suggests new tags if needed
5. Results from all chunks are merged and deduplicated
6. Existing tags are linked to the atom; new tags are created with proper hierarchy
7. The `embedding-complete` event includes tag information for UI updates

### Structured Outputs
The tag extraction uses OpenRouter's structured outputs feature to guarantee valid JSON responses:
- `response_format.type`: `"json_schema"` with strict schema validation
- `provider.require_parameters`: `true` to ensure the provider supports structured outputs
- Schema enforces the exact structure: `existing_tag_ids` (array of strings) and `new_tags` (array of objects with name, parent_id, suggested_category)

### Tag Categories
New tags are automatically placed under category tags:
- **Locations**: Geographic places
- **People**: Named individuals
- **Organizations**: Companies, institutions, groups
- **Topics**: Subject matter, concepts
- **Events**: Historical or current events
- **Other**: Miscellaneous

### Error Handling
- API errors are retried up to 3 times with exponential backoff
- Extraction failures don't break the embedding pipeline
- Missing API key or disabled auto-tagging gracefully skips extraction

## Chunking Algorithm

Content is chunked for optimal embedding generation:
1. Split by double newlines (paragraphs)
2. For paragraphs > 1500 chars, split by sentence boundaries (`. `, `! `, `? `)
3. Merge chunks < 100 chars with previous chunk
4. Skip final chunks < 50 chars
5. Cap chunks at 2000 chars max

## Key Dependencies

### Rust (Cargo.toml)
- `tauri` = "2"
- `tauri-plugin-opener` = "2"
- `rusqlite` = { version = "0.32", features = ["bundled", "load_extension"] }
- `sqlite-vec` = "0.1.6"
- `serde` = { version = "1", features = ["derive"] }
- `serde_json` = "1"
- `uuid` = { version = "1", features = ["v4"] }
- `chrono` = { version = "0.4", features = ["serde"] }
- `zerocopy` = { version = "0.8", features = ["derive"] }
- `tokio` = { version = "1", features = ["full"] }
- `reqwest` = { version = "0.12", features = ["json"] }
- `regex` = "1"

### Frontend (package.json)
- `@tauri-apps/api` = "^2.0.0"
- `react` = "^18.3.1"
- `zustand` = "^5.0.0"
- `@uiw/react-codemirror` = "^4.25.3"
- `@codemirror/lang-markdown` = "^6.5.0"
- `@codemirror/theme-one-dark` = "^6.1.3"
- `react-markdown` = "^10.1.0"
- `remark-gfm` = "^4.0.1"
- `tailwindcss` = "^4.0.0"
- `@tailwindcss/vite` = "^4.0.0"
- `@tailwindcss/typography` = "^0.5.19"

## Design System (Dark Theme - Obsidian-inspired)

### Colors
- Background: `#1e1e1e` (main), `#252525` (panels), `#2d2d2d` (cards/elevated)
- Text: `#dcddde` (primary), `#888888` (secondary/muted), `#666666` (tertiary)
- Borders: `#3d3d3d`
- Accent: `#7c3aed` (purple), `#a78bfa` (light purple for tags)
- Status: `amber-500` (pending/processing), `red-500` (failed), `green-500` (success)

### Layout
- Left Panel: 250px fixed width
- Main View: Flexible, fills remaining space
- Right Drawer: 500px max or 40% of screen, slides from right as overlay

### Animations
- Drawer slide: 200ms ease-out
- Modal fade/zoom: 200ms
- Hover transitions: 150ms
- Embedding status pulse: CSS `animate-pulse`

## State Management (Zustand Stores)

### atoms.ts
- `atoms: AtomWithTags[]` - All loaded atoms
- `isLoading: boolean` - Loading state
- `error: string | null` - Error message
- `semanticSearchQuery: string` - Current semantic search query
- `semanticSearchResults: SemanticSearchResult[] | null` - Search results (null = not searching)
- `isSearching: boolean` - Semantic search loading state
- Actions: `fetchAtoms`, `fetchAtomsByTag`, `createAtom`, `updateAtom`, `deleteAtom`, `updateAtomStatus`, `searchSemantic`, `clearSemanticSearch`, `retryEmbedding`

### tags.ts
- `tags: TagWithCount[]` - Hierarchical tag tree
- `isLoading: boolean`
- `error: string | null`
- Actions: `fetchTags`, `createTag`, `updateTag`, `deleteTag`

### ui.ts
- `selectedTagId: string | null` - Currently selected tag filter
- `drawerState: { isOpen, mode, atomId, tagId, tagName }` - Drawer state
- `viewMode: 'grid' | 'list'` - Atom display mode
- `searchQuery: string` - Text search filter
- Actions: `setSelectedTag`, `openDrawer`, `openWikiDrawer`, `closeDrawer`, `setViewMode`, `setSearchQuery`

### settings.ts
- `settings: Record<string, string>` - All settings as key-value pairs
- `isLoading: boolean`
- `error: string | null`
- Actions: `fetchSettings`, `setSetting`, `testOpenRouterConnection`

### wiki.ts
- `currentArticle: WikiArticleWithCitations | null` - Current wiki article
- `articleStatus: WikiArticleStatus | null` - Article status info
- `isLoading: boolean` - Loading state
- `isGenerating: boolean` - Generation in progress
- `isUpdating: boolean` - Update in progress
- `error: string | null` - Error message
- Actions: `fetchArticle`, `fetchArticleStatus`, `generateArticle`, `updateArticle`, `deleteArticle`, `clearArticle`, `clearError`

## sqlite-lembed Integration

### How It Works
1. **Extension Loading**: On database initialization, sqlite-lembed is loaded via `conn.load_extension()` with the `load_extension` feature enabled in rusqlite
2. **Model Registration**: The all-MiniLM-L6-v2 GGUF model is registered in `temp.lembed_models` for each connection
3. **Embedding Generation**: Content chunks are embedded using `SELECT lembed('all-MiniLM-L6-v2', ?1)`
4. **Query Embedding**: Search queries are embedded the same way for semantic matching

### Resource Files
- `all-MiniLM-L6-v2.q8_0.gguf` - Embedding model (Q8_0 quantization, ~24MB)
- `lembed0.so` - sqlite-lembed extension binary (Linux x86_64, v0.0.1-alpha.8)
- `lembed0-aarch64.dylib` - sqlite-lembed extension binary (macOS Apple Silicon, v0.0.1-alpha.8)
- `lembed0-x86_64.dylib` - sqlite-lembed extension binary (macOS Intel, v0.0.1-alpha.8)

### Platform Support
The application supports the following platforms with bundled sqlite-lembed extensions:
- **Linux x86_64**: Uses `lembed0.so`
- **macOS Apple Silicon (aarch64)**: Uses `lembed0-aarch64.dylib`
- **macOS Intel (x86_64)**: Uses `lembed0-x86_64.dylib`
- **Windows**: Not yet supported (no pre-built binaries available)

The `get_lembed_extension_filename()` function in `db.rs` automatically selects the correct extension file based on the target OS and architecture at compile time.

### Similarity Calculation
- sqlite-vec returns Euclidean distance (lower = more similar)
- For normalized vectors, convert to similarity: `1.0 - (distance / 2.0)`
- Default threshold: 0.7 for related atoms, 0.3 for semantic search, 0.3 for wiki chunk selection

