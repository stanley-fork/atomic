# Changelog

All notable changes to Atomic are documented here.

## v1.21.4 — 2026-04-12

- Add URL-based routing — views, tag filters, and open atoms/wikis are now reflected in the URL, enabling browser back/forward navigation and deep links
- Add local cache for tag tree and atom list so the app paints instantly on launch instead of waiting for the network
- Add PWA support for the web build (manifest, service worker, app icons) so the hosted server can be installed as a standalone app on mobile and desktop
- Improve reconnect behavior: transient disconnects are hidden for 4 seconds instead of flashing a banner, and resuming from background reconnects immediately
- Fix overlay back/forward chevrons navigating outside the current overlay session; they now stay scoped to reader/graph/wiki entries and disable at stack boundaries
- Fix WebSocket reconnect race where resuming the app during a pending connection could orphan an in-flight socket

## v1.21.3 — 2026-04-12

- Bundle the MCP bridge with the desktop app and auto-discover auth tokens, so local MCP setup requires no manual token configuration
- Split MCP onboarding and settings into local (stdio) and remote (HTTP + token) modes, with a one-click token provisioning flow for remote connections
- Fix desktop users connected to a remote server seeing the local sidecar URL instead of the active server URL in Mobile and MCP setup sections
- Fix stale MCP config showing after switching between local and remote server modes in settings
- Fix SSE stream handling for multi-line data events in the MCP bridge

## v1.21.2 — 2026-04-12

- Add resizable chat sidebar with drag handle, default width increased to 480px (adjustable 320–800px), persisted across sessions
- Add animated thinking indicator with live retrieval step display while the chat agent searches your knowledge base
- Persist active chat conversation so reopening the sidebar or refreshing restores where you left off

## v1.21.1 — 2026-04-12

- Improve canvas label readability by preventing overlapping atom and cluster labels — largest nodes are prioritized in dense regions

## v1.21.0 — 2026-04-11

- Add Dashboard view with AI daily briefing — a new home screen featuring a scheduled, LLM-generated summary of recently captured atoms with clickable inline citations and an embedded canvas preview
- Add briefing history navigation with prev/next controls to browse past daily briefings
- Consolidate Grid and List into a single Atoms view with a compact layout sub-toggle, simplifying the top-level navigation to four modes: Dashboard, Atoms, Canvas, and Wiki
- Migrate ~170 inline SVG icons to Lucide React, reducing frontend bundle size by ~4 kB gzipped
- Improve reliability of structured LLM outputs (wiki synthesis, tag extraction, briefing) with unified retry logic, tolerant JSON parsing, and a prompt-based fallback for providers that ignore response_format

## v1.20.2 — 2026-04-11

- Cache the global canvas payload in memory with automatic invalidation on atom, tag, and edge changes — eliminates redundant PCA recomputation and makes the canvas load significantly faster after the first request
- Warm the canvas cache at server startup so the first canvas open is instant instead of waiting for a full recompute
- Optimize canvas metadata query from two correlated subqueries per atom to a single JOIN + GROUP BY, improving canvas load time for large knowledge bases
- Serialize concurrent cold-cache canvas rebuilds so multiple simultaneous requests share a single computation instead of racing

## v1.20.1 — 2026-04-11

- Fix release notification formatting in the CI pipeline (no user-facing changes).

## v1.20.0 — 2026-04-11

- Add configurable auto-tag categories — choose which top-level tags the AI auto-tagger is allowed to extend (e.g. disable People/Locations if you don't need them, or add your own like "Projects" or "Books"), manageable during onboarding and in Settings → Tags
- Add Obsidian plugin onboarding wizard with a 4-step setup flow, database selection, size-based sync batching, YAML frontmatter stripping, and real-time sync progress reporting
- Fix mobile layout — sidebar, chat, and filter controls now work correctly on small screens with a slide-in sidebar, full-width chat overlay, filter bottom-sheet, and an overflow menu for reader actions
- Fix Obsidian plugin resync loop when the target database already contains atoms — re-syncing to a populated database now deduplicates server-side instead of retrying endlessly
- Skip the onboarding wizard when connecting to a server that is already configured with an AI provider
- Fix Obsidian plugin wiki view to preserve citation markers for notes outside the current vault instead of stripping them
