import { ItemView, WorkspaceLeaf, TFile } from "obsidian";
import { AtomicClient, type SearchResult } from "./atomic-client";
import type { SyncState } from "./sync-state";

export const SIMILAR_VIEW_TYPE = "atomic-similar";

export class SimilarView extends ItemView {
  private client: AtomicClient;
  private getSyncState: () => SyncState;
  private results: SearchResult[] = [];
  private currentFile: string | null = null;

  constructor(
    leaf: WorkspaceLeaf,
    client: AtomicClient,
    getSyncState: () => SyncState
  ) {
    super(leaf);
    this.client = client;
    this.getSyncState = getSyncState;
  }

  getViewType(): string {
    return SIMILAR_VIEW_TYPE;
  }

  getDisplayText(): string {
    return "Similar Notes";
  }

  getIcon(): string {
    return "git-compare";
  }

  async onOpen(): Promise<void> {
    this.registerEvent(
      this.app.workspace.on("active-leaf-change", () => this.onActiveFileChange())
    );
    await this.onActiveFileChange();
  }

  private async onActiveFileChange(): Promise<void> {
    const file = this.app.workspace.getActiveFile();
    if (!file || file.extension !== "md") {
      this.results = [];
      this.render();
      return;
    }

    if (file.path === this.currentFile) return;
    this.currentFile = file.path;

    const syncState = this.getSyncState();
    const info = syncState.getFile(file.path);

    if (!info) {
      this.results = [];
      this.render("Note not synced to Atomic yet. Use 'Sync Current Note' first.");
      return;
    }

    try {
      this.results = await this.client.findSimilar(info.atomId, 10);
      this.render();
    } catch (e) {
      console.error("Atomic: Failed to fetch similar notes:", e);
      this.render("Failed to load similar notes.");
    }
  }

  private render(message?: string): void {
    const container = this.containerEl.children[1];
    container.empty();

    const wrapper = container.createDiv({ cls: "atomic-similar-container" });

    if (message) {
      wrapper.createDiv({ cls: "atomic-wiki-empty", text: message });
      return;
    }

    if (this.results.length === 0) {
      wrapper.createDiv({
        cls: "atomic-wiki-empty",
        text: this.currentFile ? "No similar notes found." : "Open a note to see similar content.",
      });
      return;
    }

    for (const result of this.results) {
      const item = wrapper.createDiv({ cls: "atomic-similar-item" });

      item.createDiv({
        cls: "atomic-similar-title",
        text: result.title || "Untitled",
      });

      const score = Math.round(result.similarity_score * 100);
      item.createDiv({
        cls: "atomic-similar-score",
        text: `${score}% similar`,
      });

      if (result.matching_chunk_content) {
        item.createDiv({
          cls: "atomic-similar-chunk",
          text: result.matching_chunk_content.slice(0, 200),
        });
      }

      item.addEventListener("click", () => {
        if (result.source_url?.startsWith("obsidian://")) {
          const match = result.source_url.match(/^obsidian:\/\/[^/]+\/(.+)$/);
          if (match) {
            const filePath = decodeURIComponent(match[1]);
            const file = this.app.vault.getAbstractFileByPath(filePath);
            if (file instanceof TFile) {
              this.app.workspace.getLeaf(false).openFile(file);
              return;
            }
          }
        }
      });
    }
  }
}
