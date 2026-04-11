import { ItemView, WorkspaceLeaf, MarkdownRenderer } from "obsidian";
import { AtomicClient, type TagWithCount, type WikiArticleWithCitations, type WikiCitation } from "./atomic-client";

export const WIKI_VIEW_TYPE = "atomic-wiki";

export class WikiView extends ItemView {
  private client: AtomicClient;
  private getVaultName: () => string;
  private tags: TagWithCount[] = [];
  private selectedTagId: string | null = null;
  private article: WikiArticleWithCitations | null = null;
  private loading = false;

  constructor(leaf: WorkspaceLeaf, client: AtomicClient, getVaultName: () => string) {
    super(leaf);
    this.client = client;
    this.getVaultName = getVaultName;
  }

  getViewType(): string {
    return WIKI_VIEW_TYPE;
  }

  getDisplayText(): string {
    return "Atomic Wiki";
  }

  getIcon(): string {
    return "book-open";
  }

  async onOpen(): Promise<void> {
    await this.loadTags();
    this.render();
  }

  private async loadTags(): Promise<void> {
    try {
      this.tags = await this.client.getTags(1);
    } catch (e) {
      console.error("Atomic: Failed to load tags:", e);
    }
  }

  private flattenTags(tags: TagWithCount[], depth = 0): Array<{ tag: TagWithCount; depth: number }> {
    const result: Array<{ tag: TagWithCount; depth: number }> = [];
    for (const tag of tags) {
      result.push({ tag, depth });
      if (tag.children.length > 0) {
        result.push(...this.flattenTags(tag.children, depth + 1));
      }
    }
    return result;
  }

  private render(): void {
    const container = this.containerEl.children[1];
    container.empty();

    const wrapper = container.createDiv({ cls: "atomic-wiki-container" });

    // Tag selector
    const select = wrapper.createEl("select", { cls: "atomic-wiki-tag-select" });
    select.createEl("option", { text: "Select a tag...", value: "" });

    const flatTags = this.flattenTags(this.tags);
    for (const { tag, depth } of flatTags) {
      const prefix = "\u00A0\u00A0".repeat(depth);
      select.createEl("option", {
        text: `${prefix}${tag.name} (${tag.atom_count})`,
        value: tag.id,
      });
    }

    if (this.selectedTagId) {
      select.value = this.selectedTagId;
    }

    select.addEventListener("change", async () => {
      this.selectedTagId = select.value || null;
      if (this.selectedTagId) {
        await this.loadArticle(this.selectedTagId);
      } else {
        this.article = null;
      }
      this.renderContent(wrapper);
    });

    // Content area
    this.renderContent(wrapper);
  }

  private async loadArticle(tagId: string): Promise<void> {
    this.loading = true;
    try {
      this.article = await this.client.getWikiArticle(tagId);
    } catch (e) {
      console.error("Atomic: Failed to load wiki article:", e);
      this.article = null;
    }
    this.loading = false;
  }

  private renderContent(wrapper: HTMLElement): void {
    // Remove previous content (keep the select)
    const existing = wrapper.querySelector(".atomic-wiki-content, .atomic-wiki-empty, .atomic-wiki-actions");
    if (existing) existing.remove();

    // Also remove any additional content/action elements
    wrapper.querySelectorAll(".atomic-wiki-content, .atomic-wiki-empty, .atomic-wiki-actions").forEach((el) => el.remove());

    if (this.loading) {
      wrapper.createDiv({ cls: "atomic-wiki-empty", text: "Loading..." });
      return;
    }

    if (!this.selectedTagId) {
      wrapper.createDiv({
        cls: "atomic-wiki-empty",
        text: "Select a tag to view its wiki article.",
      });
      return;
    }

    if (!this.article) {
      const empty = wrapper.createDiv({ cls: "atomic-wiki-empty" });
      empty.setText("No article for this tag yet.");

      const actions = wrapper.createDiv({ cls: "atomic-wiki-actions" });
      const genBtn = actions.createEl("button", { text: "Generate Article" });
      genBtn.addEventListener("click", async () => {
        if (!this.selectedTagId) return;
        const tagName = this.findTagName(this.selectedTagId);
        if (!tagName) {
          genBtn.setText("Tag not found");
          return;
        }
        genBtn.disabled = true;
        genBtn.setText("Generating...");
        try {
          this.article = await this.client.generateWikiArticle(this.selectedTagId, tagName);
          this.renderContent(wrapper);
        } catch (e) {
          genBtn.setText("Failed - Retry");
          genBtn.disabled = false;
          console.error("Atomic: Failed to generate wiki:", e);
        }
      });
      return;
    }

    const contentEl = wrapper.createDiv({ cls: "atomic-wiki-content" });
    const rewritten = this.rewriteCitations(
      this.article.article.content,
      this.article.citations
    );
    MarkdownRenderer.render(this.app, rewritten, contentEl, "", this);
  }

  /**
   * Replace `[N]` citation markers in wiki content with Obsidian wikilinks
   * for citations that point to atoms synced from this vault, and strip
   * markers that point to non-Obsidian atoms.
   *
   * Caveat: this is a regex pass and does not respect markdown code fences.
   * Wiki content from the LLM rarely contains code blocks with `[N]`-shaped
   * strings, so the trade-off is acceptable for now.
   */
  private rewriteCitations(content: string, citations: WikiCitation[]): string {
    const byIndex = new Map<number, WikiCitation>();
    for (const c of citations) byIndex.set(c.citation_index, c);

    const vaultPrefix = `obsidian://${this.getVaultName()}/`;

    return content.replace(/\[(\d+)\]/g, (match, indexStr) => {
      const index = parseInt(indexStr, 10);
      const citation = byIndex.get(index);
      // Unknown citation index — leave the marker untouched rather than silently dropping it.
      if (!citation) return match;

      const url = citation.source_url;
      if (!url || !url.startsWith(vaultPrefix)) {
        // Non-Obsidian (or different-vault) citation — leave the marker as-is
        // so prose like "…Smith [2] and Jones [3]…" doesn't end up with a
        // dangling gap that looks like a typo. The reader can still see the
        // citation existed even though we can't link it.
        return match;
      }

      // Decode obsidian://VaultName/encoded/path.md → vault-relative path,
      // then drop the .md extension for the wikilink target. Obsidian resolves
      // both `[[Note]]` and `[[folder/Note]]`; using the path is unambiguous
      // when basenames collide.
      const encodedPath = url.slice(vaultPrefix.length);
      let path: string;
      try {
        path = encodedPath
          .split("/")
          .map((seg) => decodeURIComponent(seg))
          .join("/");
      } catch {
        return ""; // malformed encoding — strip rather than render garbage
      }
      const target = path.replace(/\.md$/i, "");
      return `[[${target}]]`;
    });
  }

  private findTagName(tagId: string): string | null {
    const search = (nodes: TagWithCount[]): string | null => {
      for (const node of nodes) {
        if (node.id === tagId) return node.name;
        if (node.children.length > 0) {
          const found = search(node.children);
          if (found) return found;
        }
      }
      return null;
    };
    return search(this.tags);
  }
}
