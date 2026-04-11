// scripts/generate-changelog.js
//
// Generates a concise, human-friendly CHANGELOG entry for a release using the
// Claude Agent SDK. The SDK inherits Atomic's CLAUDE.md and project settings
// via `settingSources: ['project']`, so the summary follows the same context
// Claude Code sees locally.
//
// Auth: the SDK uses $ANTHROPIC_API_KEY if set, otherwise falls back to the
// credentials stored by the Claude Code CLI. If you're logged into Claude Code,
// no extra setup is required.

import { execSync } from 'child_process';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const PROJECT_ROOT = path.resolve(__dirname, '..');

function execCapture(cmd) {
  return execSync(cmd, { cwd: PROJECT_ROOT, encoding: 'utf8' }).trim();
}

/**
 * Most recent tag reachable from HEAD, or null if the repo has no tags yet.
 */
export function getPreviousTag() {
  try {
    return execCapture('git describe --tags --abbrev=0');
  } catch {
    return null;
  }
}

function collectCommitHistory(previousTag) {
  const range = previousTag ? `${previousTag}..HEAD` : 'HEAD';
  // %h short hash, %s subject, %b body — separated by a unique marker so the
  // model can tell commits apart even when bodies contain blank lines.
  const log = execCapture(
    `git log ${range} --no-merges --pretty=format:"--- %h %s%n%b"`
  );
  let stat = '';
  try {
    stat = execCapture(`git diff --stat ${range}`);
  } catch {
    // diff against a non-existent range on the first release is fine
  }
  return { range, log, stat };
}

// Read-only built-ins the changelog agent is allowed to use. The agent inherits
// the project's CLAUDE.md via settingSources, and uses these to inspect the
// actual diffs, read source, and grep the repo — not just the commit subjects.
// The custom submit_changelog tool is added alongside these at query time.
const CHANGELOG_TOOLS = ['Bash', 'Read', 'Grep', 'Glob'];
const SUBMIT_TOOL_NAME = 'mcp__changelog__submit_changelog';

// Hard timeout so a runaway release script can't hang the build forever.
const AGENT_TIMEOUT_MS = 10 * 60 * 1000;

function logAgentProgress(message) {
  if (message.type === 'assistant' && message.message?.content) {
    for (const block of message.message.content) {
      if (block.type === 'text' && block.text) {
        // Keep it compact — just the first line of any assistant narration.
        const firstLine = block.text.split('\n').find((l) => l.trim());
        if (firstLine) process.stdout.write(`  · ${firstLine.trim()}\n`);
      } else if (block.type === 'tool_use') {
        const name = block.name;
        const input = block.input ?? {};
        let detail = '';
        if (name === 'Bash' && input.command) {
          detail = String(input.command).split('\n')[0].slice(0, 120);
        } else if ((name === 'Read' || name === 'Glob') && input.file_path) {
          detail = String(input.file_path);
        } else if (name === 'Glob' && input.pattern) {
          detail = String(input.pattern);
        } else if (name === 'Grep' && input.pattern) {
          detail = String(input.pattern);
        }
        process.stdout.write(`  → ${name}${detail ? `: ${detail}` : ''}\n`);
      }
    }
  }
}

/**
 * Generate the markdown body (bullets only — no heading) for a release entry.
 *
 * The agent has read-only access to the repo via Bash/Read/Grep/Glob so it
 * can inspect actual diffs (`git show <hash>`, `git log -p`, reading source)
 * rather than relying on commit subjects alone. Because this runs from an
 * unattended build script, permissions are bypassed — the tool allowlist is
 * the real safety boundary, not interactive prompts.
 *
 * @param {string | null} previousTag
 * @param {string} newVersion
 * @returns {Promise<string>}
 */
export async function generateChangelogBody(previousTag, newVersion) {
  const { log, stat } = collectCommitHistory(previousTag);
  if (!log) {
    throw new Error(
      `No commits found since ${previousTag ?? 'the beginning of history'}. Nothing to release.`
    );
  }

  // Dynamic import so the SDK only loads when actually cutting a release.
  const { query, tool, createSdkMcpServer } = await import(
    '@anthropic-ai/claude-agent-sdk'
  );
  const { z } = await import('zod');

  // Capture the bullets the agent submits via the custom tool. Using a closure
  // here (rather than parsing the final text message) is what makes preamble
  // impossible by construction — we never read the model's free-form output.
  let submittedBullets = null;

  const submitChangelog = tool(
    'submit_changelog',
    'Submit the final CHANGELOG entry for this release. Call this exactly once, ' +
      'at the end of your investigation, with the finalized bullets. Do not include ' +
      'any surrounding prose — the `bullets` array is the entire changelog entry.',
    {
      bullets: z
        .array(z.string().min(1))
        .min(1)
        .max(6)
        .describe(
          'Markdown bullet lines without the leading "- ". Each entry is one ' +
            'line, present tense, user-facing ("Add…", "Fix…", "Improve…"). ' +
            '1 to 6 entries. Group related commits. If every commit is purely ' +
            'internal, still submit at least one bullet with the most ' +
            'user-relevant framing available.'
        ),
    },
    async (args) => {
      submittedBullets = args.bullets;
      return {
        content: [{ type: 'text', text: 'Changelog submitted.' }],
      };
    }
  );

  const changelogServer = createSdkMcpServer({
    name: 'changelog',
    version: '1.0.0',
    tools: [submitChangelog],
  });

  const range = previousTag ? `${previousTag}..HEAD` : 'HEAD';

  const prompt = `You are writing the CHANGELOG entry for Atomic v${newVersion}.

Atomic is a personal knowledge base desktop app (Tauri + React + Rust + SQLite)
with a headless server and an iOS client. Readers of this changelog are users
of the app, not contributors — focus on what they will notice.

The range for this release is \`${range}\`${previousTag ? ` (previous tag: ${previousTag})` : ''}.

Here's a quick seed to orient you — the commit subjects and file-change summary:

=== GIT LOG (subjects + bodies) ===
${log}

=== FILE DIFF SUMMARY ===
${stat || '(no diff stat available)'}

You have read-only access to the repo. **Don't stop at the commit subjects — dig in.**
Use your tools to understand what actually changed:

- \`git show <hash>\` or \`git log -p ${range} -- <path>\` to see real diffs for
  anything whose user impact isn't clear from the subject alone.
- \`Read\` / \`Grep\` / \`Glob\` to inspect source around interesting changes
  (e.g. a new config option, a reworked UI component, a changed default).
- Look at CLAUDE.md if you need context on the architecture.
- You can run any read-only Bash command. Don't modify anything on disk.

Your goal is a concise, accurate, user-facing changelog — even when that
requires more context than what the commit messages say.

When you're done investigating, submit the finalized entry by calling the
\`submit_changelog\` tool with a \`bullets\` array. That tool call IS the
changelog — do not also print the entry as text. Anything you write outside
of the tool call is ignored.

Content rules for the bullets:
- 1 to 6 entries. Each entry is one line, present tense, user-facing
  ("Add…", "Fix…", "Improve…"). No leading "- " — the array entries are
  already bullets.
- Group related commits into a single bullet where it makes sense.
- Prefer omitting purely internal refactors, dependency bumps, and CI changes.
  But if *every* commit in the range is internal, still submit at least one
  bullet with the most user-relevant framing available (e.g. "Improve internal
  release infrastructure") rather than refusing.
- Don't invent features that aren't actually in the diff.
- Call \`submit_changelog\` exactly once.`;

  const abortController = new AbortController();
  const timeout = setTimeout(() => abortController.abort(), AGENT_TIMEOUT_MS);

  let errorSubtype = null;
  let errors = [];

  try {
    for await (const message of query({
      prompt,
      options: {
        cwd: PROJECT_ROOT,
        settingSources: ['project'],
        tools: CHANGELOG_TOOLS,
        mcpServers: { changelog: changelogServer },
        allowedTools: [...CHANGELOG_TOOLS, SUBMIT_TOOL_NAME],
        // Unattended build script — auto-approve tool calls. The tool allowlist
        // above (read-only exploration + submit_changelog) is the real safety
        // boundary.
        permissionMode: 'bypassPermissions',
        abortController,
      },
    })) {
      logAgentProgress(message);
      if (message.type === 'result' && message.subtype !== 'success') {
        errorSubtype = message.subtype;
        errors = message.errors ?? [];
      }
    }
  } finally {
    clearTimeout(timeout);
  }

  if (errorSubtype) {
    const detail = errors.length ? `\n${errors.join('\n')}` : '';
    throw new Error(`Claude Agent SDK returned error subtype: ${errorSubtype}${detail}`);
  }
  if (!submittedBullets || submittedBullets.length === 0) {
    throw new Error(
      'Changelog agent finished without calling submit_changelog. ' +
        'Nothing to write to CHANGELOG.md.'
    );
  }

  return submittedBullets
    .map((line) => `- ${line.trim().replace(/^-+\s*/, '')}`)
    .join('\n');
}

/**
 * Prepend a new release entry to CHANGELOG.md, creating the file if needed.
 * Returns the absolute path of the file that was written.
 *
 * @param {string} newVersion
 * @param {string} body - markdown bullets (no heading)
 */
export function prependChangelog(newVersion, body) {
  const changelogPath = path.join(PROJECT_ROOT, 'CHANGELOG.md');
  const date = new Date().toISOString().slice(0, 10);
  const entry = `## v${newVersion} — ${date}\n\n${body.trim()}\n`;

  let existing = '';
  if (fs.existsSync(changelogPath)) {
    existing = fs.readFileSync(changelogPath, 'utf8');
  }

  let newContent;
  if (!existing.trim()) {
    newContent = `# Changelog\n\nAll notable changes to Atomic are documented here.\n\n${entry}`;
  } else {
    // Insert the new entry before the first existing `## ` section. If there
    // are no sections yet (only a top-level header), append after the preamble.
    const firstSectionIdx = existing.search(/^## /m);
    if (firstSectionIdx === -1) {
      newContent = `${existing.trimEnd()}\n\n${entry}`;
    } else {
      newContent = `${existing.slice(0, firstSectionIdx)}${entry}\n${existing.slice(firstSectionIdx)}`;
    }
  }

  fs.writeFileSync(changelogPath, newContent);
  return changelogPath;
}
