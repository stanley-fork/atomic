/**
 * Apple Notes import — orchestration.
 *
 * 1. Ask the Tauri backend for the contents of `NoteStore.sqlite`
 *    (`read_apple_notes` returns accounts, folders, and gunzipped protobuf
 *    blobs for each note).
 * 2. Decode each note's protobuf with `protobufjs` and convert attribute runs
 *    to markdown via `NoteConverter`.
 * 3. Build a hierarchical tag set mirroring Apple Notes folders (plus a
 *    top-level account tag when the user has multiple accounts).
 * 4. Send atoms in batches via `bulk_create_atoms`; the server dedupes on
 *    source URL.
 *
 * Attachments: the `ConverterContext.lookupAttachment` implementation here
 * does no DB queries — Atomic has no concept of vault-local attachments, and
 * the dominant path (file-backed drawings/media) is rendered as a placeholder
 * regardless of what we'd look up. URL cards, internal-note links, and tables
 * all ship their content inline inside the note's own protobuf, so no side
 * lookup is needed for those either. See convert-note.ts for the fallback
 * rendering.
 */

import { Root } from 'protobufjs';
import { descriptor } from './apple-notes/descriptor';
import { NoteConverter } from './apple-notes/convert-note';
import type { ANDocument, ANConverter, ANConverterType, ConverterContext } from './apple-notes/models';
import { ANFolderType } from './apple-notes/models';
import { getTransport } from './transport';
import type { ImportResult } from './api';
import {
  createTagCache,
  resolveTagIds,
  type HierarchicalTag,
  type TagCache,
} from './import-tags';

// ---------- Types ----------

export interface AppleNotesAccount {
  pk: number;
  name: string;
  uuid: string;
}

export interface AppleNotesFolder {
  pk: number;
  title: string;
  parentPk: number | null;
  accountPk: number | null;
  identifier: string;
  folderType: number;
}

export interface AppleNotesNote {
  pk: number;
  title: string;
  folderPk: number | null;
  creationDate: number | null;
  modificationDate: number | null;
  isPasswordProtected: boolean;
  protobufBase64: string | null;
}

export interface AppleNotesData {
  accounts: AppleNotesAccount[];
  folders: AppleNotesFolder[];
  notes: AppleNotesNote[];
}

export interface AppleNotesImportProgress {
  current: number;
  total: number;
  currentFile: string;
  status: 'importing' | 'skipped' | 'error';
}

export interface AppleNotesImportOptions {
  importTags?: boolean;
  importTrashed?: boolean;
  includeHandwriting?: boolean;
  omitFirstLine?: boolean;
  onProgress?: (progress: AppleNotesImportProgress) => void;
}

// Exposed for tests.
export interface AppleNotesDeps {
  readAppleNotes: (folderPath: string) => Promise<AppleNotesData>;
  bulkCreate: (atoms: BulkCreateAtomInput[]) => Promise<BulkCreateResult>;
  resolveTags: (folderTags: HierarchicalTag[], flat: string[], cache: TagCache) => Promise<string[]>;
}

interface BulkCreateAtomInput {
  content: string;
  sourceUrl: string;
  skipIfSourceExists: boolean;
  tagIds: string[];
}

interface BulkCreateResult {
  atoms: unknown[];
  count: number;
  skipped: number;
}

// ---------- Constants ----------

const BATCH_SIZE = 50;
const MIN_CONTENT_LENGTH = 10;

// ---------- Base64 → Uint8Array (browser-safe) ----------

function base64ToBytes(b64: string): Uint8Array {
  const binary = atob(b64);
  const out = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) out[i] = binary.charCodeAt(i);
  return out;
}

// ---------- Main entry point ----------

export async function importAppleNotes(
  folderPath: string,
  options: AppleNotesImportOptions = {},
): Promise<ImportResult> {
  return importAppleNotesWithDeps(folderPath, options, defaultDeps());
}

/**
 * Test-friendly overload: accepts injected deps so unit tests can run the
 * full orchestration against synthetic `AppleNotesData` without hitting
 * protobuf/Tauri/HTTP.
 */
export async function importAppleNotesWithDeps(
  folderPath: string,
  options: AppleNotesImportOptions,
  deps: AppleNotesDeps,
): Promise<ImportResult> {
  const {
    importTags = true,
    importTrashed = false,
    includeHandwriting = false,
    omitFirstLine = true,
    onProgress,
  } = options;

  const data = await deps.readAppleNotes(folderPath);

  const accountsByPk = new Map(data.accounts.map((a) => [a.pk, a]));
  const folderPks = new Set(data.folders.map((f) => f.pk));

  // Identify folders to skip (trash, smart) and find active root folders.
  const activeFolders = data.folders.filter(
    (f) => f.folderType !== ANFolderType.Smart && (importTrashed || f.folderType !== ANFolderType.Trash),
  );
  const activeFolderPks = new Set(activeFolders.map((f) => f.pk));

  const noteTitlesByPk = new Map(data.notes.map((n) => [n.pk, n.title]));
  const multiAccount = data.accounts.length > 1;

  const protobufRoot = Root.fromJSON(descriptor);

  const ctx: ConverterContext = {
    includeHandwriting,
    omitFirstLine,
    decodeData: <T extends ANConverter>(hexOrBytes: string | Uint8Array, converterType: ANConverterType<T>): T => {
      const bytes = typeof hexOrBytes === 'string' ? hexToBytes(hexOrBytes) : hexOrBytes;
      const type = protobufRoot.lookupType(converterType.protobufType);
      const decoded = type.decode(bytes);
      return new converterType(ctx, decoded);
    },
    lookupAttachment: async () => null,
    resolveInternalLinkTitle: async (identifier: string) => {
      // The identifier is a note UUID (uppercase). Apple Notes stores this as
      // `zidentifier` on a ICNote row — we don't return it from the backend,
      // so fall back to scanning by raw identifier. In practice most internal
      // links are to notes also being imported, and if the target title
      // happens to match we can wikilink. Otherwise return the raw id so the
      // user can find it.
      const lower = identifier.toLowerCase();
      for (const n of data.notes) {
        if (n.title && n.title.toLowerCase() === lower) return n.title;
      }
      return null;
    },
  };

  let imported = 0;
  let skipped = 0;
  let errors = 0;
  let tagsLinked = 0;

  const tagCache = createTagCache();

  // Prepare per-note payloads (markdown + tag IDs).
  const prepared: { atom: BulkCreateAtomInput; title: string }[] = [];
  const total = data.notes.length;
  let processed = 0;

  for (const note of data.notes) {
    processed++;

    if (note.folderPk === null || !folderPks.has(note.folderPk) || !activeFolderPks.has(note.folderPk)) {
      skipped++;
      continue;
    }

    if (note.isPasswordProtected) {
      onProgress?.({ current: processed, total, currentFile: note.title, status: 'skipped' });
      skipped++;
      continue;
    }

    if (!note.protobufBase64) {
      onProgress?.({ current: processed, total, currentFile: note.title, status: 'error' });
      errors++;
      continue;
    }

    onProgress?.({ current: processed, total, currentFile: note.title, status: 'importing' });

    try {
      const bytes = base64ToBytes(note.protobufBase64);
      const docType = protobufRoot.lookupType(NoteConverter.protobufType);
      const doc = docType.decode(bytes) as ANDocument;

      const converter = new NoteConverter(ctx, doc);
      const body = await converter.format(false);

      let content = body.trim();
      if (!content.startsWith('# ')) {
        content = `# ${note.title}\n\n${content}`;
      }

      if (content.length < MIN_CONTENT_LENGTH) {
        skipped++;
        continue;
      }

      // Resolve tags: account (if multi-account) + folder hierarchy.
      let tagIds: string[] = [];
      if (importTags) {
        const folderTags = buildFolderHierarchy(
          note.folderPk,
          data.folders,
          accountsByPk,
          multiAccount,
        );
        tagIds = await deps.resolveTags(folderTags, [], tagCache);
        tagsLinked += tagIds.length;
      }

      const account = note.folderPk !== null
        ? accountsByPk.get(data.folders.find((f) => f.pk === note.folderPk)?.accountPk ?? -1)
        : undefined;
      const sourceUrl = `applenotes://${account?.uuid ?? 'unknown'}/${note.pk}`;

      prepared.push({
        atom: { content, sourceUrl, skipIfSourceExists: true, tagIds },
        title: note.title,
      });
    } catch (err) {
      console.error(`Apple Notes import: failed to convert note "${note.title}":`, err);
      errors++;
    }
  }

  // Bulk create in batches.
  for (let i = 0; i < prepared.length; i += BATCH_SIZE) {
    const batch = prepared.slice(i, i + BATCH_SIZE);
    onProgress?.({
      current: Math.min(i + BATCH_SIZE, prepared.length),
      total: prepared.length,
      currentFile: batch[batch.length - 1].title,
      status: 'importing',
    });
    try {
      const result = await deps.bulkCreate(batch.map((b) => b.atom));
      imported += result.count;
      skipped += result.skipped;
    } catch (e) {
      errors += batch.length;
      console.error('Apple Notes import: bulk create failed:', e);
    }
  }

  // Silence the linter — we exported this for parity with the markdown importer,
  // but we don't need the reverse lookup because note titles are already in data.notes.
  void noteTitlesByPk;

  return {
    imported,
    skipped,
    errors,
    tags_created: tagCache.size,
    tags_linked: tagsLinked,
  };
}

// ---------- Helpers ----------

/**
 * Walk from a note's folder up to the root, producing a single
 * `HierarchicalTag` whose name is the innermost folder and whose parent path
 * is the chain of ancestor folder titles (plus the account name when
 * multi-account). Folders with `identifier` starting with `DefaultFolder` are
 * omitted — they're Apple Notes' implicit "Notes" folder that every account
 * has, and don't add useful structure.
 */
export function buildFolderHierarchy(
  folderPk: number,
  folders: AppleNotesFolder[],
  accountsByPk: Map<number, AppleNotesAccount>,
  multiAccount: boolean,
): HierarchicalTag[] {
  const byPk = new Map(folders.map((f) => [f.pk, f]));
  const chain: AppleNotesFolder[] = [];

  let cursor: AppleNotesFolder | undefined = byPk.get(folderPk);
  while (cursor) {
    chain.push(cursor);
    cursor = cursor.parentPk !== null ? byPk.get(cursor.parentPk) : undefined;
  }
  chain.reverse();

  // Strip the implicit default folder so notes that sit in the top-level
  // "Notes" bucket don't get an unnecessary tag.
  const meaningful = chain.filter((f) => !f.identifier.startsWith('DefaultFolder'));

  const titles = meaningful.map((f) => f.title).filter(Boolean);

  if (multiAccount) {
    const account = chain[0] && accountsByPk.get(chain[0].accountPk ?? -1);
    if (account?.name) titles.unshift(account.name);
  }

  if (titles.length === 0) return [];

  return [
    {
      name: titles[titles.length - 1],
      parentPath: titles.slice(0, -1),
    },
  ];
}

function hexToBytes(hex: string): Uint8Array {
  const bytes = new Uint8Array(hex.length / 2);
  for (let i = 0; i < bytes.length; i++) {
    bytes[i] = parseInt(hex.substr(i * 2, 2), 16);
  }
  return bytes;
}

function defaultDeps(): AppleNotesDeps {
  return {
    // `read_apple_notes` lives in the Tauri main process, not atomic-server,
    // so it must be invoked over Tauri IPC directly rather than through the
    // HTTP transport (which would 404).
    readAppleNotes: async (folderPath: string) => {
      const { invoke } = await import('@tauri-apps/api/core');
      return await invoke<AppleNotesData>('read_apple_notes', { folderPath });
    },
    bulkCreate: (atoms: BulkCreateAtomInput[]) =>
      getTransport().invoke<BulkCreateResult>('bulk_create_atoms', { atoms }),
    resolveTags: resolveTagIds,
  };
}
