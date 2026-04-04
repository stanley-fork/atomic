import { ItemView, WorkspaceLeaf, MarkdownRenderer } from "obsidian";
import { AtomicClient, type TagWithCount, type WikiArticle } from "./atomic-client";

export const WIKI_VIEW_TYPE = "atomic-wiki";

export class WikiView extends ItemView {
  private client: AtomicClient;
  private tags: TagWithCount[] = [];
  private selectedTagId: string | null = null;
  private article: WikiArticle | null = null;
  private loading = false;

  constructor(leaf: WorkspaceLeaf, client: AtomicClient) {
    super(leaf);
    this.client = client;
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
        genBtn.disabled = true;
        genBtn.setText("Generating...");
        try {
          this.article = await this.client.generateWikiArticle(this.selectedTagId);
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
    MarkdownRenderer.render(
      this.app,
      this.article.content,
      contentEl,
      "",
      this
    );
  }
}
