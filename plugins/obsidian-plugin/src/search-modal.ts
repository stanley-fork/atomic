import { App, SuggestModal, TFile, Notice } from "obsidian";
import { AtomicClient, type SearchResult } from "./atomic-client";

export class SearchModal extends SuggestModal<SearchResult> {
  private client: AtomicClient;
  private results: SearchResult[] = [];
  private debounceTimer: ReturnType<typeof setTimeout> | null = null;

  constructor(app: App, client: AtomicClient) {
    super(app);
    this.client = client;
    this.setPlaceholder("Search your knowledge base semantically...");
    this.setInstructions([{ command: "↑↓", purpose: "navigate" }, { command: "↵", purpose: "open" }, { command: "esc", purpose: "dismiss" }]);
  }

  getSuggestions(query: string): SearchResult[] | Promise<SearchResult[]> {
    if (query.length < 2) return [];

    return new Promise((resolve) => {
      if (this.debounceTimer) clearTimeout(this.debounceTimer);

      this.debounceTimer = setTimeout(async () => {
        try {
          this.results = await this.client.search(query, "hybrid", 20);
          resolve(this.results);
        } catch (e) {
          console.error("Atomic search failed:", e);
          new Notice(`Search failed: ${e instanceof Error ? e.message : String(e)}`);
          resolve([]);
        }
      }, 300);
    });
  }

  renderSuggestion(result: SearchResult, el: HTMLElement): void {
    const container = el.createDiv({ cls: "atomic-search-result" });

    const titleLine = container.createDiv({ cls: "atomic-search-title" });
    titleLine.setText(result.title || "Untitled");

    const snippet = result.matching_chunk_content || result.snippet || "";
    if (snippet) {
      container.createDiv({
        cls: "atomic-search-snippet",
        text: snippet.slice(0, 150),
      });
    }

    const score = Math.round(result.similarity_score * 100);
    container.createDiv({
      cls: "atomic-search-score",
      text: `${score}% match${result.tags.length > 0 ? " · " + result.tags.map((t) => t.name).join(", ") : ""}`,
    });
  }

  onChooseSuggestion(result: SearchResult): void {
    // Try to open the corresponding Obsidian file via source_url
    if (result.source_url && result.source_url.startsWith("obsidian://")) {
      const filePath = this.extractFilePath(result.source_url);
      if (filePath) {
        const file = this.app.vault.getAbstractFileByPath(filePath);
        if (file instanceof TFile) {
          this.app.workspace.getLeaf(false).openFile(file);
          return;
        }
      }
    }

    // Fallback: show content in a new note (read-only)
    new Notice(`Found: ${result.title}\n${result.snippet?.slice(0, 100) || ""}`);
  }

  private extractFilePath(sourceUrl: string): string | null {
    // Format: obsidian://VaultName/encoded/path.md
    const match = sourceUrl.match(/^obsidian:\/\/[^/]+\/(.+)$/);
    if (!match) return null;
    return decodeURIComponent(match[1]);
  }
}
