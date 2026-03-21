// scripts/stress-test-wikipedia.js
// Stress test: fetch Wikipedia articles → import via HTTP API → monitor pipeline via WebSocket
import TurndownService from 'turndown';

// --- CLI argument parsing ---

function parseArgs() {
  const args = process.argv.slice(2);
  const opts = {
    server: process.env.ATOMIC_SERVER || 'http://127.0.0.1:8080',
    token: process.env.ATOMIC_TOKEN || null,
    count: 100,
    batchSize: 100,
    mode: 'crawl',
    dbName: `stress-test-${Math.floor(Date.now() / 1000)}`,
    skipCreateDb: false,
    skipMonitor: false,
    timeout: 600,
    jsonOutput: false,
  };

  for (let i = 0; i < args.length; i++) {
    switch (args[i]) {
      case '--server':
        opts.server = args[++i];
        break;
      case '--token':
        opts.token = args[++i];
        break;
      case '--count':
        opts.count = parseInt(args[++i], 10);
        break;
      case '--batch-size':
        opts.batchSize = Math.min(parseInt(args[++i], 10), 1000);
        break;
      case '--mode':
        opts.mode = args[++i];
        break;
      case '--db-name':
        opts.dbName = args[++i];
        break;
      case '--skip-create-db':
        opts.skipCreateDb = true;
        break;
      case '--skip-monitor':
        opts.skipMonitor = true;
        break;
      case '--timeout':
        opts.timeout = parseInt(args[++i], 10);
        break;
      case '--json-output':
        opts.jsonOutput = true;
        break;
      case '--help':
        console.log(`Usage: node scripts/stress-test-wikipedia.js [options]

Options:
  --server <url>       Server URL (default: http://127.0.0.1:8080, or ATOMIC_SERVER env)
  --token <token>      API token (required, or ATOMIC_TOKEN env)
  --count <n>          Articles to import (default: 100)
  --batch-size <n>     Atoms per bulk API call (default: 100, max 1000)
  --mode <mode>        "random" or "crawl" (default: crawl)
  --db-name <name>     Database name (default: "stress-test-{timestamp}")
  --skip-create-db     Use active database instead of creating one
  --skip-monitor       Exit after import without waiting for pipeline
  --timeout <seconds>  Max wait for pipeline completion (default: 600)
  --json-output        Output report as JSON`);
        process.exit(0);
    }
  }

  if (!opts.token) {
    console.error('Error: --token is required (or set ATOMIC_TOKEN env)');
    process.exit(1);
  }

  if (!['random', 'crawl'].includes(opts.mode)) {
    console.error('Error: --mode must be "random" or "crawl"');
    process.exit(1);
  }

  return opts;
}

// --- HTTP helper ---

function createClient(serverUrl, token, dbId) {
  const base = serverUrl.replace(/\/$/, '');

  return async function request(method, path, body) {
    const headers = {
      Authorization: `Bearer ${token}`,
      'Content-Type': 'application/json',
    };
    if (dbId) {
      headers['X-Atomic-Database'] = dbId;
    }

    const res = await fetch(`${base}${path}`, {
      method,
      headers,
      body: body != null ? JSON.stringify(body) : undefined,
    });

    if (!res.ok) {
      const text = await res.text().catch(() => '');
      throw new Error(`${method} ${path} → ${res.status}: ${text}`);
    }

    return res.json();
  };
}

// --- Wikipedia fetching ---

const SEEDS = {
  computing: [
    'History_of_computing', 'Computer', 'Alan_Turing', 'Programming_language',
    'Artificial_intelligence', 'Internet', 'World_Wide_Web', 'Operating_system',
    'Algorithm', 'Data_structure', 'Software_engineering', 'Computer_science',
    'Machine_learning', 'Database', 'Computer_network',
  ],
  philosophy: [
    'Philosophy', 'Epistemology', 'Ethics', 'Metaphysics', 'Logic',
    'Plato', 'Aristotle', 'Immanuel_Kant', 'Friedrich_Nietzsche', 'Existentialism',
    'Stoicism', 'Utilitarianism', 'Philosophy_of_mind', 'Political_philosophy', 'Aesthetics',
  ],
  history: [
    'European_Union', 'History_of_Europe', 'Ancient_Greece', 'Roman_Empire',
    'Renaissance', 'World_War_I', 'World_War_II', 'Cold_War', 'French_Revolution',
    'Industrial_Revolution', 'Byzantine_Empire', 'Holy_Roman_Empire',
    'Napoleonic_Wars', 'Ancient_Rome', 'Medieval_Europe',
  ],
};

const turndown = new TurndownService({
  headingStyle: 'atx',
  codeBlockStyle: 'fenced',
  bulletListMarker: '-',
});
turndown.remove(['script', 'style', 'nav', 'footer', 'aside', 'sup', 'figure']);

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function fetchArticleHtml(title) {
  try {
    const url = `https://en.wikipedia.org/api/rest_v1/page/html/${encodeURIComponent(title)}`;
    const res = await fetch(url, {
      headers: { 'Accept-Language': 'en', 'User-Agent': 'AtomicStressTest/1.0' },
    });
    if (!res.ok) return null;

    const html = await res.text();
    const markdown = turndown.turndown(html).trim();

    // Skip very short articles (disambiguation, stubs)
    if (markdown.length < 200) return null;

    return {
      title: title.replace(/_/g, ' '),
      content: `# ${title.replace(/_/g, ' ')}\n\n${markdown}`,
      url: `https://en.wikipedia.org/wiki/${encodeURIComponent(title)}`,
    };
  } catch (err) {
    console.error(`  Failed to fetch ${title}: ${err.message}`);
    return null;
  }
}

async function fetchLinksFromArticle(title, limit = 10) {
  try {
    const res = await fetch(
      `https://en.wikipedia.org/w/api.php?action=query&titles=${encodeURIComponent(title)}&prop=links&pllimit=${limit}&plnamespace=0&format=json&origin=*`
    );
    if (!res.ok) return [];

    const data = await res.json();
    const pages = data.query?.pages;
    if (!pages) return [];

    const pageId = Object.keys(pages)[0];
    const links = pages[pageId]?.links || [];
    return links.map((link) => link.title.replace(/ /g, '_'));
  } catch (err) {
    return [];
  }
}

async function fetchRandomTitle() {
  try {
    const res = await fetch('https://en.wikipedia.org/api/rest_v1/page/random/summary');
    if (!res.ok) return null;
    const data = await res.json();
    if (data.type === 'disambiguation') return null;
    return data.title?.replace(/ /g, '_') || null;
  } catch {
    return null;
  }
}

async function fetchArticles(count, mode) {
  const articles = [];
  const seen = new Set();
  const startTime = Date.now();

  if (mode === 'crawl') {
    // BFS crawl from seed articles
    const queue = [];
    for (const seeds of Object.values(SEEDS)) {
      for (const seed of seeds) {
        queue.push(seed);
      }
    }

    while (articles.length < count && queue.length > 0) {
      const title = queue.shift();
      if (seen.has(title)) continue;
      seen.add(title);

      const article = await fetchArticleHtml(title);
      if (article) {
        articles.push(article);
        if (!process.env.ATOMIC_QUIET) {
          process.stdout.write(`\r  Fetched ${articles.length}/${count} articles`);
        }

        // Fetch outgoing links and add to queue
        if (articles.length < count) {
          const links = await fetchLinksFromArticle(title, 10);
          for (const link of links) {
            if (!seen.has(link)) queue.push(link);
          }
        }
      }

      await sleep(100); // Rate limit
    }
  } else {
    // Random mode
    while (articles.length < count) {
      const title = await fetchRandomTitle();
      if (title && !seen.has(title)) {
        seen.add(title);
        const article = await fetchArticleHtml(title);
        if (article) {
          articles.push(article);
          if (!process.env.ATOMIC_QUIET) {
            process.stdout.write(`\r  Fetched ${articles.length}/${count} articles`);
          }
        }
      }
      await sleep(100);
    }
  }

  if (!process.env.ATOMIC_QUIET) process.stdout.write('\n');

  const elapsed = (Date.now() - startTime) / 1000;
  return { articles, fetchTime: elapsed };
}

// --- Pipeline tracker ---

class PipelineTracker {
  constructor(atomIds) {
    this.atoms = new Map(); // atomId → { embedded: bool, tagged: bool, embeddingOk: bool, taggingOk: bool }
    for (const id of atomIds) {
      this.atoms.set(id, { embedded: false, tagged: false, embeddingOk: false, taggingOk: false });
    }
    this.embeddedOk = 0;
    this.embeddedFail = 0;
    this.taggedOk = 0;
    this.taggedFail = 0;
    this.taggedSkip = 0;
    this._resolve = null;
    this._progressInterval = null;
  }

  handleEvent(event) {
    const { type, atom_id } = event;
    if (!atom_id || !this.atoms.has(atom_id)) return;

    const state = this.atoms.get(atom_id);

    switch (type) {
      case 'EmbeddingComplete':
        if (!state.embedded) {
          state.embedded = true;
          state.embeddingOk = true;
          this.embeddedOk++;
        }
        break;
      case 'EmbeddingFailed':
        if (!state.embedded) {
          state.embedded = true;
          this.embeddedFail++;
        }
        break;
      case 'TaggingComplete':
        if (!state.tagged) {
          state.tagged = true;
          state.taggingOk = true;
          this.taggedOk++;
        }
        break;
      case 'TaggingFailed':
        if (!state.tagged) {
          state.tagged = true;
          this.taggedFail++;
        }
        break;
      case 'TaggingSkipped':
        if (!state.tagged) {
          state.tagged = true;
          this.taggedSkip++;
        }
        break;
    }

    // Check if all atoms are done (both embedded and tagged)
    if (this._resolve && this._allDone()) {
      this._resolve();
    }
  }

  _allDone() {
    for (const state of this.atoms.values()) {
      if (!state.embedded || !state.tagged) return false;
    }
    return true;
  }

  _printProgress() {
    const total = this.atoms.size;
    const embedded = this.embeddedOk + this.embeddedFail;
    const tagged = this.taggedOk + this.taggedFail + this.taggedSkip;
    process.stdout.write(
      `\r  Pipeline: ${embedded}/${total} embedded, ${tagged}/${total} tagged`
    );
  }

  waitForCompletion(timeoutMs) {
    if (this._allDone()) return Promise.resolve();

    return new Promise((resolve, reject) => {
      this._resolve = resolve;

      this._progressInterval = setInterval(() => this._printProgress(), 5000);

      setTimeout(() => {
        clearInterval(this._progressInterval);
        this._printProgress();
        process.stdout.write('\n');
        if (!this._allDone()) {
          resolve(); // Resolve anyway on timeout, report will show incomplete
        }
      }, timeoutMs);
    });
  }

  cleanup() {
    if (this._progressInterval) clearInterval(this._progressInterval);
  }
}

// --- Main ---

async function main() {
  const opts = parseArgs();
  const log = opts.jsonOutput ? () => {} : console.log;

  log(`Atomic Wikipedia Stress Test`);
  log(`  Server:  ${opts.server}`);
  log(`  Count:   ${opts.count}`);
  log(`  Mode:    ${opts.mode}`);
  log(`  Batch:   ${opts.batchSize}`);
  log('');

  // 1. Verify server connection
  const api = createClient(opts.server, opts.token, null);
  let dbList;
  try {
    dbList = await api('GET', '/api/databases');
    log(`Connected to server (${dbList.databases.length} databases)`);
  } catch (err) {
    console.error(`Failed to connect to server: ${err.message}`);
    process.exit(1);
  }

  // 2. Create or use existing database
  let dbId = null;
  let dbName = opts.dbName;

  if (opts.skipCreateDb) {
    dbId = dbList.active_id;
    const active = dbList.databases.find((d) => d.id === dbId);
    dbName = active?.name || 'active';
    log(`Using active database: ${dbName} (${dbId})`);
  } else {
    try {
      const db = await api('POST', '/api/databases', { name: opts.dbName });
      dbId = db.id;
      log(`Created database: ${dbName} (${dbId})`);
    } catch (err) {
      console.error(`Failed to create database: ${err.message}`);
      process.exit(1);
    }
  }

  // Create DB-scoped client
  const dbApi = createClient(opts.server, opts.token, dbId);

  // 3. Connect WebSocket
  let tracker = null;
  let ws = null;

  if (!opts.skipMonitor) {
    const wsUrl = opts.server.replace(/^http/, 'ws') + `/ws?token=${opts.token}`;
    ws = new WebSocket(wsUrl);

    await new Promise((resolve, reject) => {
      ws.onopen = resolve;
      ws.onerror = (e) => reject(new Error('WebSocket connection failed'));
      setTimeout(() => reject(new Error('WebSocket connection timeout')), 10000);
    });
    log(`WebSocket connected`);

    // We'll attach the message handler after we have atom IDs
  }

  // 4. Fetch Wikipedia articles
  log(`\nFetching ${opts.count} Wikipedia articles (${opts.mode} mode)...`);
  const { articles, fetchTime } = await fetchArticles(opts.count, opts.mode);
  log(`Fetched ${articles.length} articles in ${fetchTime.toFixed(1)}s (${(articles.length / fetchTime).toFixed(1)} articles/sec)`);

  if (articles.length === 0) {
    console.error('No articles fetched, aborting.');
    ws?.close();
    process.exit(1);
  }

  // 5. Import via bulk API with adaptive batch sizing
  log(`\nImporting ${articles.length} atoms...`);
  const importStart = Date.now();
  const allAtomIds = [];
  let skipped = 0;
  let batchNum = 0;

  // Send a batch, splitting in half on 413 (payload too large)
  async function sendBatch(items) {
    const payload = items.map((a) => ({ content: a.content, source_url: a.url }));
    const payloadSize = JSON.stringify(payload).length;

    // Pre-split if obviously too large (>1.8MB to leave headroom)
    if (payloadSize > 1_800_000 && items.length > 1) {
      const mid = Math.ceil(items.length / 2);
      await sendBatch(items.slice(0, mid));
      await sendBatch(items.slice(mid));
      return;
    }

    try {
      const result = await dbApi('POST', '/api/atoms/bulk', payload);
      for (const atom of result.atoms) {
        allAtomIds.push(atom.id);
      }
      skipped += result.skipped || 0;
      batchNum++;
      log(`  Batch ${batchNum}: ${result.count} imported, ${result.skipped || 0} skipped (${items.length} atoms, ${(payloadSize / 1024).toFixed(0)}KB)`);
    } catch (err) {
      if (err.message.includes('413') && items.length > 1) {
        // Payload too large — split in half and retry
        const mid = Math.ceil(items.length / 2);
        await sendBatch(items.slice(0, mid));
        await sendBatch(items.slice(mid));
      } else {
        batchNum++;
        console.error(`  Batch ${batchNum} failed: ${err.message}`);
      }
    }
  }

  for (let i = 0; i < articles.length; i += opts.batchSize) {
    await sendBatch(articles.slice(i, i + opts.batchSize));
  }

  const importTime = (Date.now() - importStart) / 1000;
  log(`Imported ${allAtomIds.length} atoms in ${importTime.toFixed(1)}s (${(allAtomIds.length / importTime).toFixed(1)} atoms/sec)`);

  // 6. Monitor pipeline
  let pipelineTime = 0;

  if (!opts.skipMonitor && ws && allAtomIds.length > 0) {
    log(`\nMonitoring pipeline (timeout: ${opts.timeout}s)...`);
    tracker = new PipelineTracker(allAtomIds);

    ws.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data);
        tracker.handleEvent(data);
      } catch {}
    };

    const pipelineStart = Date.now();
    await tracker.waitForCompletion(opts.timeout * 1000);
    pipelineTime = (Date.now() - pipelineStart) / 1000;
    tracker.cleanup();
    ws.close();

    log('');
  } else {
    ws?.close();
  }

  // 7. Report
  const totalTime = fetchTime + importTime + pipelineTime;

  if (opts.jsonOutput) {
    const report = {
      database: { name: dbName, id: dbId },
      articles: { fetched: articles.length, imported: allAtomIds.length, skipped },
      timing: {
        fetch_seconds: +fetchTime.toFixed(1),
        import_seconds: +importTime.toFixed(1),
        pipeline_seconds: +pipelineTime.toFixed(1),
        total_seconds: +totalTime.toFixed(1),
      },
      throughput: {
        fetch_per_sec: +(articles.length / fetchTime).toFixed(1),
        import_per_sec: +(allAtomIds.length / importTime).toFixed(1),
        pipeline_per_sec: pipelineTime > 0 ? +(allAtomIds.length / pipelineTime).toFixed(1) : null,
      },
      pipeline: tracker
        ? {
            embedded_ok: tracker.embeddedOk,
            embedded_fail: tracker.embeddedFail,
            tagged_ok: tracker.taggedOk,
            tagged_fail: tracker.taggedFail,
            tagged_skip: tracker.taggedSkip,
          }
        : null,
    };
    console.log(JSON.stringify(report, null, 2));
  } else {
    console.log(`\n=== Stress Test Complete ===`);
    console.log(`Database:     ${dbName} (id: ${dbId})`);
    console.log(`Articles:     ${articles.length} fetched, ${allAtomIds.length} imported, ${skipped} skipped`);
    console.log('');
    console.log('Timing:');
    console.log(`  Fetch:      ${fetchTime.toFixed(1)}s  (${(articles.length / fetchTime).toFixed(1)} articles/sec)`);
    console.log(`  Import:     ${importTime.toFixed(1)}s   (${(allAtomIds.length / importTime).toFixed(1)} atoms/sec)`);
    if (tracker) {
      console.log(`  Pipeline:   ${pipelineTime.toFixed(1)}s (${(allAtomIds.length / pipelineTime).toFixed(1)} atoms/sec)`);
    }
    console.log(`  Total:      ${totalTime.toFixed(1)}s`);

    if (tracker) {
      console.log('');
      console.log('Pipeline:');
      console.log(`  Embedded:   ${tracker.embeddedOk} ok, ${tracker.embeddedFail} failed`);
      console.log(`  Tagged:     ${tracker.taggedOk} ok, ${tracker.taggedFail} failed, ${tracker.taggedSkip} skipped`);
    }
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
