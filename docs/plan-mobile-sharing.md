# Mobile Sharing & Server-Side URL Scraping

## Problem

Atomic's browser extension captures web pages in reader mode (Readability.js + Turndown.js) and sends cleaned markdown to the server. There's no equivalent workflow for mobile devices — users can't easily send content to Atomic from their phone.

## Goals

1. Let mobile users share URLs to Atomic as easily as the browser extension does on desktop
2. Extract article content server-side so any client can save a URL without needing DOM access
3. Keep the architecture simple — no native mobile app required initially

## Approach

### Server-Side URL Scraping

Add a `POST /api/atoms/from-url` endpoint to `atomic-server` that accepts a URL, fetches the page, extracts the main article content, converts it to markdown, and creates an atom with the `source_url` set.

**Library: `dom_smoothie`**

- Native Rust port of Mozilla's Readability.js
- Built-in markdown output (`TextMode::Markdown`) — handles extraction and conversion in one step
- MIT licensed, actively maintained (15 releases in 2025, zero open issues)
- Best extraction quality among Rust readability crates ([independent comparison](https://emschwartz.me/comparing-13-rust-crates-for-extracting-text-from-html/))
- Pure Rust, minimal deps (built on `dom-query`)

Pipeline:
```
POST /api/atoms/from-url { url: "https://..." }
  → reqwest::get(url)
  → dom_smoothie extract (TextMode::Markdown)
  → create atom { content: markdown, source_url: url }
  → return atom (embedding/tagging pipeline fires in background as usual)
```

**Fallback option:** If `dom_smoothie`'s markdown quality is insufficient, swap in `htmd` (Turndown.js-equivalent, passes all turndown test cases) for the HTML-to-markdown step and use `dom_smoothie` in HTML output mode.

**Limitation:** Server-side fetching uses a bare HTTP client, not the user's browser session. Paywalled or login-gated content won't be accessible. For those cases users would need to copy-paste content manually.

### Mobile Client: PWA Share Target

Make the web frontend installable as a PWA with share target support. When a user shares a URL from any mobile app (browser, Twitter, Reddit, etc.), the OS share sheet routes it to Atomic, which calls the `from-url` endpoint.

Requirements:
- `manifest.json` with `share_target` configured to accept URLs
- Service worker for PWA installability
- A lightweight share handler page that receives the shared URL, calls the endpoint, shows confirmation

This works because the Docker deployment already serves the web frontend on the same origin as the API — no CORS issues, and the share target can hit the API directly.

### Integration with Existing Browser Extension

The `from-url` endpoint is useful beyond mobile. The browser extension could use it as a fallback when content script extraction fails (e.g., CSP-restricted pages, PDFs). The extension would still prefer client-side extraction (has DOM access + user session for paywalled content) but fall back to server-side when needed.

## Implementation Order

1. **`dom_smoothie` integration in `atomic-core`** — Add URL scraping as a core capability so it's available to all wrappers
2. **`POST /api/atoms/from-url` endpoint** — Expose it via `atomic-server` (and Tauri command for desktop)
3. **PWA manifest + service worker** — Make the web frontend installable
4. **Share target handler** — Wire up the share target to call the endpoint
5. **Browser extension fallback** — Update extension to use the endpoint when client-side extraction fails

## Open Questions

- Should the endpoint accept optional tag IDs for pre-tagging the atom on creation?
- Should there be a bulk variant (`POST /api/atoms/from-urls`) for importing bookmark lists?
- Worth adding a user-agent string that identifies as a browser to improve compatibility with sites that block bot-like requests?
