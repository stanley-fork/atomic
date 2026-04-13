import { App, TFile, TAbstractFile, Notice, EventRef } from "obsidian";
import { AtomicClient } from "./atomic-client";
import type { AtomicSettings } from "./settings";
import { SyncState, hashContent, type SyncStateData } from "./sync-state";

export interface SyncProgress {
  phase: 'reading' | 'syncing' | 'complete';
  totalFiles: number;
  processed: number;
  created: number;
  updated: number;
  skipped: number;
  errors: number;
  /** IDs of atoms created or updated this run — each will have embedding/tagging queued. */
  atomIds: string[];
}

export type SyncProgressCallback = (progress: SyncProgress) => void;

export class SyncEngine {
  private app: App;
  private client: AtomicClient;
  private settings: AtomicSettings;
  private syncState: SyncState;
  private pendingSync = new Map<string, ReturnType<typeof setTimeout>>();
  private watching = false;
  private eventRefs: EventRef[] = [];
  private saveState: () => Promise<void>;

  constructor(
    app: App,
    client: AtomicClient,
    settings: AtomicSettings,
    syncStateData: SyncStateData | undefined,
    saveState: () => Promise<void>
  ) {
    this.app = app;
    this.client = client;
    this.settings = settings;
    this.syncState = syncStateData ? SyncState.fromJSON(syncStateData) : new SyncState();
    this.saveState = saveState;
  }

  getSyncStateData(): SyncStateData {
    return this.syncState.toJSON();
  }

  private getVaultName(): string {
    return this.settings.vaultName || this.app.vault.getName();
  }

  private generateSourceUrl(filePath: string): string {
    const encoded = filePath
      .split("/")
      .map((s) => encodeURIComponent(s))
      .join("/");
    return `obsidian://${this.getVaultName()}/${encoded}`;
  }

  private shouldExclude(path: string): boolean {
    for (const pattern of this.settings.excludePatterns) {
      // Simple glob matching: ** matches any path, * matches any segment
      const regex = pattern
        .replace(/\*\*/g, "<<GLOBSTAR>>")
        .replace(/\*/g, "[^/]*")
        .replace(/<<GLOBSTAR>>/g, ".*");
      if (new RegExp(`^${regex}$`).test(path)) return true;
    }
    return false;
  }

  private isMarkdownFile(file: TAbstractFile): file is TFile {
    return file instanceof TFile && file.extension === "md";
  }

  // --- File event handlers ---

  private onFileChange(file: TAbstractFile): void {
    if (!this.isMarkdownFile(file)) {
      console.debug(`Atomic: ignoring non-markdown file: ${file.path}`);
      return;
    }
    if (this.shouldExclude(file.path)) {
      console.debug(`Atomic: excluded by pattern: ${file.path}`);
      return;
    }

    console.debug(`Atomic: file changed, scheduling sync: ${file.path}`);

    const existing = this.pendingSync.get(file.path);
    if (existing) clearTimeout(existing);

    this.pendingSync.set(
      file.path,
      setTimeout(() => {
        console.debug(`Atomic: debounce fired, syncing: ${file.path}`);
        this.syncFile(file).catch((e) =>
          console.error(`Atomic: Failed to sync ${file.path}:`, e)
        );
        this.pendingSync.delete(file.path);
      }, this.settings.syncDebounceMs)
    );
  }

  private onFileCreate(file: TAbstractFile): void {
    if (!this.isMarkdownFile(file) || this.shouldExclude(file.path)) return;
    // Debounce same as modify — new files often get immediate edits
    this.onFileChange(file);
  }

  private async onFileDelete(file: TAbstractFile): Promise<void> {
    if (!this.isMarkdownFile(file) || this.shouldExclude(file.path)) return;

    // Cancel any pending sync for this file
    const pending = this.pendingSync.get(file.path);
    if (pending) {
      clearTimeout(pending);
      this.pendingSync.delete(file.path);
    }

    if (!this.settings.deleteOnRemove) {
      this.syncState.removeFile(file.path);
      await this.persistState();
      return;
    }

    const info = this.syncState.getFile(file.path);
    if (info) {
      try {
        await this.client.deleteAtom(info.atomId);
        this.syncState.removeFile(file.path);
        await this.persistState();
      } catch (e) {
        console.error(`Atomic: Failed to delete atom for ${file.path}:`, e);
        new Notice(`Atomic: couldn't delete remote atom for ${file.name ?? file.path} — ${e instanceof Error ? e.message : String(e)}`);
      }
    }
  }

  private async onFileRename(file: TAbstractFile, oldPath: string): Promise<void> {
    if (!this.isMarkdownFile(file)) return;

    const info = this.syncState.getFile(oldPath);
    if (!info) return;

    // Update sync state path mapping
    this.syncState.renameFile(oldPath, file.path);

    // Update the atom's source_url to reflect new path
    const newSourceUrl = this.generateSourceUrl(file.path);
    try {
      const raw = await this.app.vault.read(file);
      const content = this.stripFrontmatter(file, raw);
      await this.client.updateAtom(info.atomId, {
        content,
        source_url: newSourceUrl,
      });
      await this.persistState();
    } catch (e) {
      console.error(`Atomic: Failed to update source URL for renamed ${file.path}:`, e);
      new Notice(`Atomic: couldn't update renamed note — ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  /** Strip YAML frontmatter using Obsidian's parsed metadata cache */
  private stripFrontmatter(file: TFile, content: string): string {
    const cache = this.app.metadataCache.getFileCache(file);
    if (cache?.frontmatterPosition) {
      const end = cache.frontmatterPosition.end.offset;
      return content.slice(end).trimStart();
    }
    return content;
  }

  // --- Core sync logic ---

  async syncFile(file: TFile): Promise<void> {
    const raw = await this.app.vault.read(file);
    const content = this.stripFrontmatter(file, raw);
    const hash = await hashContent(content);

    // Skip if unchanged
    const existing = this.syncState.getFile(file.path);
    if (existing && existing.contentHash === hash) {
      console.debug(`Atomic: skipping unchanged file: ${file.path}`);
      return;
    }
    console.debug(`Atomic: syncing ${file.path} (existing atom: ${existing?.atomId ?? "none"})`);

    const sourceUrl = this.generateSourceUrl(file.path);

    try {
      if (existing) {
        // Update existing atom
        await this.client.updateAtom(existing.atomId, { content, source_url: sourceUrl });
        this.syncState.setFile(file.path, {
          atomId: existing.atomId,
          contentHash: hash,
          lastSynced: Date.now(),
        });
      } else {
        // Check server for existing atom (e.g., imported via batch before plugin was installed)
        const serverAtom = await this.client.getAtomBySourceUrl(sourceUrl);
        if (serverAtom) {
          await this.client.updateAtom(serverAtom.id, { content, source_url: sourceUrl });
          this.syncState.setFile(file.path, {
            atomId: serverAtom.id,
            contentHash: hash,
            lastSynced: Date.now(),
          });
        } else {
          // Create new atom
          const created = await this.client.createAtom({ content, source_url: sourceUrl });
          this.syncState.setFile(file.path, {
            atomId: created.id,
            contentHash: hash,
            lastSynced: Date.now(),
          });
        }
      }
      await this.persistState();
    } catch (e) {
      console.error(`Atomic: Sync failed for ${file.path}:`, e);
      throw e;
    }
  }

  async syncCurrentFile(): Promise<void> {
    const file = this.app.workspace.getActiveFile();
    if (!file) {
      new Notice("No active file to sync");
      return;
    }
    if (!this.isMarkdownFile(file)) {
      new Notice("Active file is not a markdown file");
      return;
    }
    try {
      await this.syncFile(file);
      new Notice(`Synced: ${file.basename}`);
    } catch (e) {
      new Notice(`Sync failed: ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  async syncAll(onProgress?: SyncProgressCallback): Promise<SyncProgress> {
    const files = this.app.vault.getMarkdownFiles().filter((f) => !this.shouldExclude(f.path));
    const progress: SyncProgress = {
      phase: 'reading',
      totalFiles: files.length,
      processed: 0,
      created: 0,
      updated: 0,
      skipped: 0,
      errors: 0,
      atomIds: [],
    };

    if (!onProgress) {
      new Notice(`Syncing ${files.length} files to Atomic...`);
    }
    onProgress?.(progress);

    // Separate files into new (bulk-create) vs existing (individual update)
    const newFiles: { file: TFile; content: string; hash: string; sourceUrl: string }[] = [];
    const updatedFiles: { file: TFile; content: string; hash: string; sourceUrl: string; atomId: string }[] = [];

    for (const file of files) {
      try {
        const raw = await this.app.vault.read(file);
        const content = this.stripFrontmatter(file, raw);
        const hash = await hashContent(content);
        const existing = this.syncState.getFile(file.path);

        if (existing && existing.contentHash === hash) {
          progress.skipped++;
          progress.processed++;
          continue;
        }

        const sourceUrl = this.generateSourceUrl(file.path);
        if (existing) {
          updatedFiles.push({ file, content, hash, sourceUrl, atomId: existing.atomId });
        } else {
          newFiles.push({ file, content, hash, sourceUrl });
        }
      } catch (e) {
        progress.errors++;
        progress.processed++;
        console.error(`Atomic: Failed to read ${file.path}:`, e);
      }
    }

    progress.phase = 'syncing';
    onProgress?.(progress);

    // Bulk-create new files in batches (cap at ~1.5MB per batch to stay under actix-web's 2MB default)
    const MAX_BATCH_BYTES = 1_500_000;
    const batches: typeof newFiles[] = [];
    let currentBatch: typeof newFiles = [];
    let currentBytes = 0;
    for (const entry of newFiles) {
      const entryBytes = new TextEncoder().encode(entry.content).length;
      if (currentBatch.length > 0 && currentBytes + entryBytes > MAX_BATCH_BYTES) {
        batches.push(currentBatch);
        currentBatch = [];
        currentBytes = 0;
      }
      currentBatch.push(entry);
      currentBytes += entryBytes;
    }
    if (currentBatch.length > 0) batches.push(currentBatch);

    for (const batch of batches) {
      try {
        // skip_if_source_exists prevents duplicates when re-syncing to a database
        // that already has atoms (e.g., reverting a database name change). The
        // server returns skipped entries' atom IDs nowhere in the response, so
        // we fall back to getAtomBySourceUrl below for any unmatched entries.
        const result = await this.client.bulkCreateAtoms(
          batch.map((f) => ({
            content: f.content,
            source_url: f.sourceUrl,
            skip_if_source_exists: true,
          }))
        );

        // Match returned atoms back to files via source_url
        const atomByUrl = new Map<string, string>();
        for (const atom of result.atoms) {
          if (atom.source_url) {
            atomByUrl.set(atom.source_url, atom.id);
          }
        }

        // Collect entries the bulk endpoint didn't return (skipped because the
        // atom already existed, or otherwise dropped). Look each one up so we
        // can register it in syncState — otherwise every subsequent sync would
        // re-attempt and re-fail forever.
        const unmatched = batch.filter((entry) => !atomByUrl.has(entry.sourceUrl));
        const fallbackResults = await Promise.all(
          unmatched.map(async (entry) => {
            try {
              const existing = await this.client.getAtomBySourceUrl(entry.sourceUrl);
              return { entry, atomId: existing?.id ?? null, error: null as Error | null };
            } catch (e) {
              return { entry, atomId: null, error: e as Error };
            }
          })
        );
        const recovered = new Map<string, string>();
        const lookupErrors = new Map<string, Error>();
        for (const r of fallbackResults) {
          if (r.atomId) recovered.set(r.entry.sourceUrl, r.atomId);
          else if (r.error) lookupErrors.set(r.entry.sourceUrl, r.error);
        }

        for (const entry of batch) {
          const newId = atomByUrl.get(entry.sourceUrl);
          if (newId) {
            this.syncState.setFile(entry.file.path, {
              atomId: newId,
              contentHash: entry.hash,
              lastSynced: Date.now(),
            });
            progress.atomIds.push(newId);
            progress.created++;
          } else {
            const existingId = recovered.get(entry.sourceUrl);
            if (existingId) {
              // Atom already existed on the server — adopt it. Treat as updated
              // for progress reporting since we didn't create anything new.
              this.syncState.setFile(entry.file.path, {
                atomId: existingId,
                contentHash: entry.hash,
                lastSynced: Date.now(),
              });
              progress.atomIds.push(existingId);
              progress.updated++;
            } else {
              progress.errors++;
              const lookupErr = lookupErrors.get(entry.sourceUrl);
              console.error(
                `Atomic: No atom returned for ${entry.file.path}${lookupErr ? ` (lookup also failed: ${lookupErr.message})` : ""}`
              );
            }
          }
          progress.processed++;
        }

        await this.persistState();
        onProgress?.(progress);
      } catch (e) {
        progress.errors += batch.length;
        progress.processed += batch.length;
        console.error(`Atomic: Bulk create failed (${batch.length} files):`, e);
        onProgress?.(progress);
      }
    }

    // Update existing files individually
    for (const entry of updatedFiles) {
      try {
        await this.client.updateAtom(entry.atomId, {
          content: entry.content,
          source_url: entry.sourceUrl,
        });
        this.syncState.setFile(entry.file.path, {
          atomId: entry.atomId,
          contentHash: entry.hash,
          lastSynced: Date.now(),
        });
        progress.atomIds.push(entry.atomId);
        progress.updated++;
      } catch (e) {
        progress.errors++;
        console.error(`Atomic: Failed to update ${entry.file.path}:`, e);
      }
      progress.processed++;
      onProgress?.(progress);
    }

    await this.persistState();
    progress.phase = 'complete';
    onProgress?.(progress);

    if (!onProgress) {
      new Notice(`Sync complete: ${progress.created} created, ${progress.updated} updated, ${progress.skipped} unchanged, ${progress.errors} errors`);
    }

    return progress;
  }

  async resetAndResync(onProgress?: SyncProgressCallback): Promise<SyncProgress> {
    this.syncState.clear();
    await this.persistState();
    return this.syncAll(onProgress);
  }

  // --- Watch management ---

  startWatching(): void {
    if (this.watching) return;
    this.watching = true;
    console.log("Atomic: auto-sync started, watching vault events");

    this.eventRefs.push(
      this.app.vault.on("modify", (file) => this.onFileChange(file)),
      this.app.vault.on("create", (file) => this.onFileCreate(file)),
      this.app.vault.on("delete", (file) => {
        this.onFileDelete(file).catch((e) =>
          console.error("Atomic: delete handler error:", e)
        );
      }),
      this.app.vault.on("rename", (file, oldPath) => {
        this.onFileRename(file, oldPath).catch((e) =>
          console.error("Atomic: rename handler error:", e)
        );
      })
    );
  }

  stopWatching(): void {
    if (!this.watching) return;
    this.watching = false;

    for (const ref of this.eventRefs) {
      this.app.vault.offref(ref);
    }
    this.eventRefs = [];

    // Clear pending syncs
    for (const timeout of this.pendingSync.values()) {
      clearTimeout(timeout);
    }
    this.pendingSync.clear();
  }

  toggleAutoSync(): void {
    if (this.watching) {
      this.stopWatching();
      new Notice("Auto-sync disabled");
    } else {
      this.startWatching();
      new Notice("Auto-sync enabled");
    }
  }

  private async persistState(): Promise<void> {
    await this.saveState();
  }
}
