# Atomic - Note-Taking Desktop Application

## Project Overview
Atomic is a Tauri v2 desktop application for note-taking with a React frontend. It features markdown editing, hierarchical tagging, AI-powered semantic search using embeddings, automatic tag extraction, wiki article synthesis using OpenRouter LLM, and an interactive canvas view for spatial atom visualization.

## Current Status: Phase 6 In Progress
Phase 6 (Conversational AI) in progress:
- Chat conversations with agentic RAG pipeline
- Multi-tag scoped conversations (editable at any time)
- Tool-calling agent with search_atoms and get_atom tools
- Conversations list view in right drawer
- Chat interface with markdown rendering and citations
- Chat entry points: header button and tag hover icon
- Streaming responses via Tauri events

Phase 5 (Canvas View) is complete with:
- Interactive, zoomable canvas view as the default view option
- Atoms spatially arranged using D3-force simulation based on semantic similarity (temporarily disabled due to performance issues)
- Connection lines drawn between atoms sharing tags
- Zoom/pan handled by react-zoom-pan-pinch library
- View toggle (Canvas | Grid | List) in main view header
- Canvas view persisted as default, preference saved to localStorage
- Tag filtering fades non-matching atoms to 20% opacity
- Semantic search fades non-matching atoms to 20% opacity
- Positions saved to database after simulation completes
- Subsequent canvas views load stored positions (no re-simulation)
- Incremental position calculation for new atoms

Phase 4 (Wiki Synthesis) features:
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
- **State Management**: Zustand 5 (with persist middleware for UI preferences)
- **Database**: SQLite with sqlite-vec extension (via rusqlite)
- **Embeddings**: OpenRouter-based embeddings (default model: openai/text-embedding-3-small)
- **LLM Provider**: OpenRouter API (configurable model, default: openai/gpt-4o-mini for tagging)
- **HTTP Client**: reqwest (Rust)
- **Markdown Editor**: CodeMirror 6 (`@uiw/react-codemirror`)
- **Markdown Rendering**: react-markdown with remark-gfm
- **Canvas Visualization**: d3-force (simulation), react-zoom-pan-pinch (zoom/pan)

## Project Structure
```
/scripts
  import-wikipedia.js  # Bulk import Wikipedia articles for stress testing
  README.md            # Documentation for scripts

/src-tauri
  /src
    main.rs           # Tauri entry point
    lib.rs            # App setup, command registration
    db.rs             # SQLite setup, migrations
    commands.rs       # All Tauri command implementations
    models.rs         # Rust structs for data
    chunking.rs       # Content chunking algorithm
    embedding.rs      # Embedding generation + tag extraction pipeline
    extraction.rs     # Tag extraction logic using provider abstraction
    wiki.rs           # Wiki article generation and update logic
    settings.rs       # Settings CRUD operations
    chat.rs           # Conversation CRUD and scope management
    agent.rs          # Agentic chat loop with tool calling and streaming
    clustering.rs     # Atom clustering for canvas visualization
    /providers        # Pluggable AI provider abstraction
      mod.rs          # Module exports
      types.rs        # Message, ToolCall, CompletionResponse, StreamDelta, etc.
      error.rs        # ProviderError enum with retry support
      traits.rs       # EmbeddingProvider, LlmProvider, StreamingLlmProvider traits
      registry.rs     # Provider factory (for future multi-provider support)
      /openrouter     # OpenRouter provider implementation
        mod.rs        # OpenRouterProvider combining embedding + LLM
        embedding.rs  # Embedding API calls
        llm.rs        # Chat completion + streaming
  Cargo.toml
  tauri.conf.json

/src
  /components
    /layout           # LeftPanel, MainView, RightDrawer, Layout
    /atoms            # AtomCard, AtomEditor, AtomViewer, AtomGrid, AtomList, RelatedAtoms
    /canvas           # CanvasView, CanvasContent, AtomNode, ConnectionLines, CanvasControls, useForceSimulation
    /tags             # TagTree, TagNode, TagChip, TagSelector
    /wiki             # WikiViewer, WikiArticleContent, WikiHeader, WikiEmptyState, WikiGenerating, CitationLink, CitationPopover
    /chat             # ChatViewer, ConversationsList, ConversationCard, ChatView, ChatHeader, ChatMessage, ChatInput, ScopeEditor
    /search           # SemanticSearch
    /settings         # SettingsModal, SettingsButton
    /ui               # Button, Input, Modal, FAB, ContextMenu
  /stores             # Zustand stores (atoms.ts, tags.ts, ui.ts, settings.ts, wiki.ts, chat.ts)
  /hooks              # Custom hooks (useClickOutside, useKeyboard, useEmbeddingEvents, useChatEvents)
  /lib                # Utilities (tauri.ts, markdown.ts, date.ts, similarity.ts)
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

### Utility Scripts
```bash
# Import Wikipedia articles for stress testing (requires app to be run once first)
npm run import:wikipedia        # Import 500 articles (default)
npm run import:wikipedia 1000   # Import custom number of articles
npm run import:wikipedia 500 --db /path/to/atomic.db  # Custom database path
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

-- Atom positions for canvas view
CREATE TABLE atom_positions (
  atom_id TEXT PRIMARY KEY REFERENCES atoms(id) ON DELETE CASCADE,
  x REAL NOT NULL,
  y REAL NOT NULL,
  updated_at TEXT NOT NULL
);

-- Chat conversations
CREATE TABLE conversations (
  id TEXT PRIMARY KEY,
  title TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  is_archived INTEGER DEFAULT 0
);

-- Many-to-many: conversation tag scope
CREATE TABLE conversation_tags (
  conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
  tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
  PRIMARY KEY (conversation_id, tag_id)
);

-- Chat messages
CREATE TABLE chat_messages (
  id TEXT PRIMARY KEY,
  conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
  role TEXT NOT NULL,  -- 'user', 'assistant', 'system', 'tool'
  content TEXT NOT NULL,
  created_at TEXT NOT NULL,
  message_index INTEGER NOT NULL
);

-- Tool calls for transparency
CREATE TABLE chat_tool_calls (
  id TEXT PRIMARY KEY,
  message_id TEXT NOT NULL REFERENCES chat_messages(id) ON DELETE CASCADE,
  tool_name TEXT NOT NULL,
  tool_input TEXT NOT NULL,
  tool_output TEXT,
  status TEXT NOT NULL DEFAULT 'pending',
  created_at TEXT NOT NULL,
  completed_at TEXT
);

-- Chat citations
CREATE TABLE chat_citations (
  id TEXT PRIMARY KEY,
  message_id TEXT NOT NULL REFERENCES chat_messages(id) ON DELETE CASCADE,
  citation_index INTEGER NOT NULL,
  atom_id TEXT NOT NULL REFERENCES atoms(id) ON DELETE CASCADE,
  chunk_index INTEGER,
  excerpt TEXT NOT NULL,
  relevance_score REAL
);

```

### Settings Keys
- `openrouter_api_key`: User's OpenRouter API key for LLM and embedding access
- `auto_tagging_enabled`: "true" or "false" (default: "true")
- `embedding_model`: Model for embeddings (default: "openai/text-embedding-3-small")
  - Supported: `openai/text-embedding-3-small` (1536 dim), `openai/text-embedding-3-large` (3072 dim)
  - Changing dimension requires re-embedding all atoms (handled automatically)
- `tagging_model`: Model for tag extraction (default: "openai/gpt-4o-mini")
  - Supported: `openai/gpt-4o-mini`, `openai/gpt-5-nano`, `openai/gpt-5-mini`, `anthropic/claude-sonnet-4.5`
- `wiki_model`: Model for wiki generation (default: "anthropic/claude-sonnet-4.5")
  - Supported: `anthropic/claude-sonnet-4.5`, `openai/gpt-4o-mini`, `openai/gpt-5-nano`
- `chat_model`: Model for chat (default: "anthropic/claude-sonnet-4.5")
  - Supported: `anthropic/claude-sonnet-4.5`, `openai/gpt-4o-mini`, `openai/gpt-5-nano`

Note: LLM models are restricted to those supporting structured outputs via OpenRouter.

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

### Canvas Operations
- `get_atom_positions()` → `Vec<AtomPosition>` (returns all stored canvas positions)
- `save_atom_positions(positions)` → `()` (bulk save/update positions after simulation)
- `get_atoms_with_embeddings()` → `Vec<AtomWithEmbedding>` (atoms with average embedding vectors)

### Chat Operations
- `create_conversation(tag_ids, title?)` → `ConversationWithTags` (creates new conversation with optional tag scope)
- `get_conversations(filter_tag_id?, limit, offset)` → `Vec<ConversationWithTags>` (list conversations)
- `get_conversation(id)` → `Option<ConversationWithMessages>` (single conversation with full message history)
- `update_conversation(id, title?, is_archived?)` → `Conversation` (update metadata)
- `delete_conversation(id)` → `()` (delete conversation and all messages)
- `set_conversation_scope(conversation_id, tag_ids)` → `ConversationWithTags` (replace all scope tags)
- `add_tag_to_scope(conversation_id, tag_id)` → `ConversationWithTags` (add single tag to scope)
- `remove_tag_from_scope(conversation_id, tag_id)` → `ConversationWithTags` (remove single tag from scope)
- `send_chat_message(conversation_id, content)` → `ChatMessageWithContext` (send message, triggers agent loop)

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

### Chat Events
Events emitted during chat agent loop:

**chat-stream-delta**: Streaming content from assistant
```typescript
{ conversation_id: string; content: string; }
```

**chat-tool-start**: Tool execution started
```typescript
{ conversation_id: string; tool_call_id: string; tool_name: string; tool_input: unknown; }
```

**chat-tool-complete**: Tool execution completed
```typescript
{ conversation_id: string; tool_call_id: string; results_count: number; }
```

**chat-complete**: Full message completed
```typescript
{ conversation_id: string; message: ChatMessageWithContext; }
```

**chat-error**: Error during chat
```typescript
{ conversation_id: string; error: string; }
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
3. Each content chunk is sent to OpenRouter using the configured tagging model (default: openai/gpt-4o-mini) with the existing tag hierarchy
4. The LLM identifies existing tags that apply and suggests new tags if needed
5. Results from all chunks are merged and deduplicated
6. Existing tags are linked to the atom; new tags are created with proper hierarchy
7. The `embedding-complete` event includes tag information for UI updates

### Configurable Model
The tagging model can be configured in Settings:
- Default: `openai/gpt-4o-mini` (cheaper/faster, good for bulk imports)
- Alternative: `anthropic/claude-sonnet-4.5` (higher quality, more expensive)
- Any OpenRouter model ID that supports structured outputs can be used

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

## Canvas View

### Architecture
The canvas view provides a spatial visualization of atoms using:
- **react-zoom-pan-pinch**: Handles zoom/pan interactions via TransformWrapper and TransformComponent
- **d3-force**: Calculates atom positions using force simulation (no D3 rendering)
- **React components**: Renders atom cards and SVG connection lines

### Force Simulation
The simulation uses multiple forces to position atoms:
- `forceManyBody()`: Repulsion between atoms (strength: -200)
- `forceCollide()`: Collision detection (radius: 100px)
- `forceLink()`: Attraction between atoms sharing tags
- `forceCenter()`: Centers graph at (2500, 2500) on 5000x5000 canvas
- Custom `similarityForce`: Attraction based on embedding cosine similarity (threshold: 0.7)

### Position Persistence
- Positions are saved to `atom_positions` table after simulation completes
- On subsequent loads, stored positions are used (no re-simulation)
- New atoms trigger incremental simulation with existing atoms fixed initially

### Visual Design
- **Atom nodes**: 160px wide compact cards with truncated content
- **Connection lines**: SVG lines between atoms sharing tags (opacity: 0.15)
- **Fading**: Non-matching atoms fade to 20% opacity when filtering by tag or search
- **Canvas controls**: Zoom in/out/reset buttons in bottom-right corner

### Components
- `CanvasView`: Main container, handles data loading and simulation orchestration
- `CanvasContent`: Inner content layer that gets transformed by zoom/pan
- `AtomNode`: Compact card component for individual atoms (memoized)
- `ConnectionLines`: SVG layer rendering lines between connected atoms
- `CanvasControls`: Zoom control buttons using react-zoom-pan-pinch hooks
- `useForceSimulation`: Custom hook managing D3 force simulation

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
- `d3-force` = "^3.0.0"
- `react-zoom-pan-pinch` = "^3.0.0"
- `@types/d3-force` = "^3.0.0" (dev)
- `better-sqlite3` = "^11.5.0" (dev, for import scripts)

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
- Right Drawer: 75vw width, slides from right as overlay

### Tag Display
- Tags are collapsed by default in AtomViewer and TagSelector
- Maximum 5 tags shown initially
- "+N more" button expands to show all tags
- "Show less" button collapses back to 5 tags

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
- `drawerState: { isOpen, mode, atomId, tagId, tagName, conversationId }` - Drawer state
- `viewMode: 'canvas' | 'grid' | 'list'` - Atom display mode (default: 'canvas', persisted to localStorage)
- `searchQuery: string` - Text search filter
- Drawer modes: `'editor' | 'viewer' | 'wiki' | 'chat'`
- Actions: `setSelectedTag`, `openDrawer`, `openWikiDrawer`, `openChatDrawer`, `closeDrawer`, `setViewMode`, `setSearchQuery`

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

### chat.ts
- `view: 'list' | 'conversation'` - Current chat view
- `currentConversation: ConversationWithTags | null` - Active conversation
- `messages: ChatMessageWithContext[]` - Messages in current conversation
- `conversations: ConversationWithTags[]` - List of all conversations
- `listFilterTagId: string | null` - Filter for conversations list
- `isLoading: boolean` - Loading state
- `isStreaming: boolean` - Streaming response in progress
- `streamingContent: string` - Content being streamed
- `retrievalSteps: RetrievalStep[]` - Tool calls for transparency
- `error: string | null` - Error message
- Actions: `showList`, `openConversation`, `goBack`, `fetchConversations`, `createConversation`, `deleteConversation`, `updateConversationTitle`, `setScope`, `addTagToScope`, `removeTagFromScope`, `sendMessage`, `cancelResponse`, `appendStreamContent`, `addRetrievalStep`, `completeMessage`, `setStreamingError`, `clearError`, `reset`

### Similarity Calculation
- sqlite-vec returns Euclidean distance (lower = more similar)
- For normalized vectors, convert to similarity: `1.0 - (distance / 2.0)`
- Default threshold: 0.7 for related atoms, 0.3 for semantic search, 0.3 for wiki chunk selection
