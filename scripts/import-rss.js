// scripts/import-rss.js
import Database from 'better-sqlite3';
import { randomUUID } from 'crypto';
import Parser from 'rss-parser';
import TurndownService from 'turndown';
import { JSDOM } from 'jsdom';

// Database path - same as Wikipedia import script
// For macOS: ~/Library/Application Support/com.atomic.app/atomic.db
// For Linux: ~/.local/share/com.atomic.app/atomic.db
// For Windows: %APPDATA%/com.atomic.app/atomic.db
function getDefaultDbPath() {
  const platform = process.platform;
  const home = process.env.HOME || process.env.USERPROFILE;

  if (platform === 'darwin') {
    return `${home}/Library/Application Support/com.atomic.app/atomic.db`;
  } else if (platform === 'linux') {
    return `${home}/.local/share/com.atomic.app/atomic.db`;
  } else if (platform === 'win32') {
    return `${process.env.APPDATA}/com.atomic.app/atomic.db`;
  }
  throw new Error(`Unsupported platform: ${platform}`);
}

// Initialize Turndown service for HTML to Markdown conversion
const turndownService = new TurndownService({
  headingStyle: 'atx',
  codeBlockStyle: 'fenced',
  bulletListMarker: '-',
});

// Remove unwanted elements before conversion
turndownService.remove(['script', 'style', 'nav', 'footer', 'aside', 'header']);

// Extract main content from HTML using common selectors
function extractMainContent(html, url) {
  try {
    const dom = new JSDOM(html, { url });
    const document = dom.window.document;

    // Try multiple selectors to find main content
    const selectors = [
      'article',
      '[role="main"]',
      '.post-content',
      '.entry-content',
      '.article-content',
      '.content',
      'main',
      'body',
    ];

    for (const selector of selectors) {
      const element = document.querySelector(selector);
      if (element) {
        // Remove unwanted nested elements
        const unwanted = element.querySelectorAll('script, style, nav, footer, aside, header, .comments, .related-posts, .sidebar');
        unwanted.forEach((el) => el.remove());

        return element.innerHTML;
      }
    }

    // Fallback to body if nothing found
    return document.body.innerHTML;
  } catch (error) {
    console.error(`  Error extracting content: ${error.message}`);
    return null;
  }
}

// Convert HTML to Markdown
function convertToMarkdown(html) {
  try {
    const markdown = turndownService.turndown(html);
    return markdown.trim();
  } catch (error) {
    console.error(`  Error converting to markdown: ${error.message}`);
    return null;
  }
}

// Fetch full article HTML and extract content
async function fetchArticleContent(url) {
  try {
    const response = await fetch(url, {
      headers: {
        'User-Agent': 'Mozilla/5.0 (compatible; AtomicRSSImporter/1.0)',
      },
    });

    if (!response.ok) {
      return null;
    }

    const contentType = response.headers.get('content-type') || '';
    if (!contentType.includes('text/html')) {
      console.log(`  Skipping non-HTML content: ${contentType}`);
      return null;
    }

    const html = await response.text();
    const mainContent = extractMainContent(html, url);

    if (!mainContent) {
      return null;
    }

    const markdown = convertToMarkdown(mainContent);
    return markdown;
  } catch (error) {
    console.error(`  Error fetching ${url}: ${error.message}`);
    return null;
  }
}

// Parse RSS feed and return items
async function parseRssFeed(feedUrl) {
  try {
    const parser = new Parser({
      timeout: 10000,
      headers: {
        'User-Agent': 'Mozilla/5.0 (compatible; AtomicRSSImporter/1.0)',
      },
    });

    const feed = await parser.parseURL(feedUrl);
    return {
      title: feed.title || 'Unknown Feed',
      description: feed.description || '',
      items: feed.items || [],
    };
  } catch (error) {
    throw new Error(`Failed to parse RSS feed: ${error.message}`);
  }
}

// Normalize URL for deduplication (handle http/https variants)
function normalizeUrl(url) {
  try {
    const urlObj = new URL(url);
    // Convert to https for consistency
    urlObj.protocol = 'https:';
    // Remove trailing slash
    return urlObj.href.replace(/\/$/, '');
  } catch {
    return url;
  }
}

// Import RSS items into the database
async function importRssItems(db, feed, maxItems = null) {
  const { title, description, items } = feed;

  console.log(`\nFeed: "${title}"`);
  if (description) {
    console.log(`Description: "${description}"`);
  }
  console.log(`Total items in feed: ${items.length}`);

  // Load existing URLs for deduplication
  console.log('\nLoading existing URLs for deduplication...');
  const existingUrlsRaw = db
    .prepare('SELECT source_url FROM atoms WHERE source_url IS NOT NULL')
    .all()
    .map((row) => normalizeUrl(row.source_url));

  const existingUrls = new Set(existingUrlsRaw);
  console.log(`Found ${existingUrls.size} existing URLs in database`);

  // Filter out duplicates and apply max items limit
  let filteredItems = items.filter((item) => {
    if (!item.link) return false;
    return !existingUrls.has(normalizeUrl(item.link));
  });

  if (maxItems !== null && filteredItems.length > maxItems) {
    filteredItems = filteredItems.slice(0, maxItems);
  }

  const duplicateCount = items.length - filteredItems.length;
  console.log(`New items to import: ${filteredItems.length}`);
  if (duplicateCount > 0) {
    console.log(`Skipped ${duplicateCount} duplicates`);
  }

  if (filteredItems.length === 0) {
    console.log('\nNo new items to import!');
    return { imported: 0, skipped: 0, errors: 0 };
  }

  // Prepare insert statement
  const insertAtom = db.prepare(`
    INSERT INTO atoms (id, content, source_url, created_at, updated_at, embedding_status)
    VALUES (?, ?, ?, ?, ?, 'pending')
  `);

  // Track import statistics
  let imported = 0;
  let skippedErrors = 0;
  const processed = new Set(); // Track URLs processed in this session

  console.log('\nImporting articles...\n');

  for (let i = 0; i < filteredItems.length; i++) {
    const item = filteredItems[i];
    const itemNumber = i + 1;
    const totalItems = filteredItems.length;

    // Skip if missing required fields
    if (!item.link || !item.title) {
      console.log(`[${itemNumber}/${totalItems}] Skipped: Missing title or link`);
      skippedErrors++;
      continue;
    }

    // Check for duplicates within this session
    const normalizedUrl = normalizeUrl(item.link);
    if (processed.has(normalizedUrl)) {
      console.log(`[${itemNumber}/${totalItems}] Skipped: Duplicate in feed - ${item.title}`);
      continue;
    }

    // Fetch full article content
    let content = await fetchArticleContent(item.link);

    // Fallback to RSS description if article fetch fails
    if (!content || content.length < 20) {
      if (item.contentSnippet || item.content) {
        content = item.contentSnippet || item.content;
        console.log(`[${itemNumber}/${totalItems}] Using RSS description: ${item.title}`);
      } else {
        console.log(`[${itemNumber}/${totalItems}] Skipped: No content available - ${item.title}`);
        skippedErrors++;
        await new Promise((resolve) => setTimeout(resolve, 100));
        continue;
      }
    }

    // Format content with title as h1
    const formattedContent = `# ${item.title}\n\n${content}`;

    // Insert into database
    const now = new Date().toISOString();
    const id = randomUUID();

    try {
      insertAtom.run(id, formattedContent, item.link, now, now);
      processed.add(normalizedUrl);
      imported++;
      console.log(`[${itemNumber}/${totalItems}] Imported: ${item.title}`);
    } catch (error) {
      console.error(`[${itemNumber}/${totalItems}] Failed to insert ${item.title}: ${error.message}`);
      skippedErrors++;
    }

    // Rate limiting - be respectful to source websites
    await new Promise((resolve) => setTimeout(resolve, 100));
  }

  return { imported, skipped: duplicateCount, errors: skippedErrors };
}

async function main() {
  const args = process.argv.slice(2);
  let feedUrl = null;
  let maxItems = null;
  let dbPath = null;

  // Parse arguments
  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--db' && args[i + 1]) {
      dbPath = args[i + 1];
      i++;
    } else if (args[i] === '--max-items' && args[i + 1]) {
      maxItems = parseInt(args[i + 1]);
      if (isNaN(maxItems) || maxItems <= 0) {
        console.error('\nError: --max-items must be a positive number');
        process.exit(1);
      }
      i++;
    } else if (!feedUrl) {
      feedUrl = args[i];
    }
  }

  // Validate feed URL
  if (!feedUrl) {
    console.error('\nUsage: node scripts/import-rss.js <feed_url> [--max-items N] [--db path]');
    console.error('\nExamples:');
    console.error('  node scripts/import-rss.js https://blog.example.com/feed');
    console.error('  node scripts/import-rss.js https://blog.example.com/feed --max-items 20');
    console.error('  node scripts/import-rss.js https://blog.example.com/feed --db /path/to/atomic.db');
    process.exit(1);
  }

  // Validate URL format
  try {
    new URL(feedUrl);
  } catch {
    console.error(`\nError: Invalid URL: ${feedUrl}`);
    process.exit(1);
  }

  // Use default database path if not specified
  if (!dbPath) {
    dbPath = getDefaultDbPath();
  }

  console.log(`Opening database at ${dbPath}`);

  // Check if database exists
  const fs = await import('fs');
  if (!fs.existsSync(dbPath)) {
    console.error(`\nError: Database not found at ${dbPath}`);
    console.error('\nThe database is created when you first run the Atomic app.');
    console.error('Please run the app at least once before using this import script.');
    console.error('\nAlternatively, specify a custom database path with --db <path>');
    process.exit(1);
  }

  const db = new Database(dbPath);

  console.log(`Fetching RSS feed from ${feedUrl}...`);

  try {
    const feed = await parseRssFeed(feedUrl);
    const stats = await importRssItems(db, feed, maxItems);

    // Print summary
    console.log('\n' + '='.repeat(60));
    console.log(`Imported ${stats.imported} articles successfully.`);
    if (stats.errors > 0) {
      console.log(`Skipped ${stats.errors} articles (fetch errors or missing content).`);
    }
    if (stats.skipped > 0) {
      console.log(`Skipped ${stats.skipped} duplicates (already in database).`);
    }

    if (stats.imported > 0) {
      console.log('\nNext steps:');
      console.log('1. Start the Atomic app');
      console.log('2. Embeddings will process automatically in the background');
      console.log('3. Watch atoms update with tags as processing completes');
      console.log('\nThis may take 10-30 minutes for large batches depending on API rate limits.');
    }

    db.close();
  } catch (error) {
    console.error(`\nError: ${error.message}`);
    db.close();
    process.exit(1);
  }
}

main().catch(console.error);
