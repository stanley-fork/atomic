import { App, TFile, TAbstractFile, Notice, EventRef } from "obsidian";
import { AtomicClient } from "./atomic-client";
import type { AtomicSettings } from "./settings";
import { SyncState, hashContent, type SyncStateData } from "./sync-state";

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
    if (!this.isMarkdownFile(file) || this.shouldExclude(file.path)) return;

    const existing = this.pendingSync.get(file.path);
    if (existing) clearTimeout(existing);

    this.pendingSync.set(
      file.path,
      setTimeout(() => {
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
      const content = await this.app.vault.read(file);
      await this.client.updateAtom(info.atomId, {
        content,
        source_url: newSourceUrl,
      });
      await this.persistState();
    } catch (e) {
      console.error(`Atomic: Failed to update source URL for renamed ${file.path}:`, e);
    }
  }

  // --- Core sync logic ---

  async syncFile(file: TFile): Promise<void> {
    const content = await this.app.vault.read(file);
    const hash = await hashContent(content);

    // Skip if unchanged
    const existing = this.syncState.getFile(file.path);
    if (existing && existing.contentHash === hash) return;

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
          await this.client.updateAtom(serverAtom.atom.id, { content, source_url: sourceUrl });
          this.syncState.setFile(file.path, {
            atomId: serverAtom.atom.id,
            contentHash: hash,
            lastSynced: Date.now(),
          });
        } else {
          // Create new atom
          const created = await this.client.createAtom({ content, source_url: sourceUrl });
          this.syncState.setFile(file.path, {
            atomId: created.atom.id,
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

  async syncAll(): Promise<void> {
    const files = this.app.vault.getMarkdownFiles().filter((f) => !this.shouldExclude(f.path));
    let synced = 0;
    let skipped = 0;
    let errors = 0;

    new Notice(`Syncing ${files.length} files to Atomic...`);

    for (const file of files) {
      try {
        const content = await this.app.vault.read(file);
        const hash = await hashContent(content);
        const existing = this.syncState.getFile(file.path);

        if (existing && existing.contentHash === hash) {
          skipped++;
          continue;
        }

        await this.syncFile(file);
        synced++;
      } catch (e) {
        errors++;
        console.error(`Atomic: Failed to sync ${file.path}:`, e);
      }
    }

    new Notice(`Sync complete: ${synced} synced, ${skipped} unchanged, ${errors} errors`);
  }

  // --- Watch management ---

  startWatching(): void {
    if (this.watching) return;
    this.watching = true;

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
