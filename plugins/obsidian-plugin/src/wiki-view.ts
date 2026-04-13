import { ItemView, Notice, WorkspaceLeaf, MarkdownRenderer, setIcon } from "obsidian";
import { AtomicClient, type TagWithCount, type WikiArticleWithCitations, type WikiCitation } from "./atomic-client";

export const WIKI_VIEW_TYPE = "atomic-wiki";

export class WikiView extends ItemView {
  private client: AtomicClient;
  private getVaultName: () => string;
  private tags: TagWithCount[] = [];
  private selectedTagId: string | null = null;
  private article: WikiArticleWithCitations | null = null;
  private loading = false;
  private generatingAtomCount: number | null = null;
  private loadError: string | null = null;

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
      this.loadError = null;
    } catch (e) {
      console.error("Atomic: Failed to load tags:", e);
      this.loadError = `Couldn't load tags: ${e instanceof Error ? e.message : String(e)}`;
    }
  }

  private flattenTags(tags: TagWithCount[], depth = 0): Array<{ tag: TagWithCount; depth: number }> {
    const result: Array<{ tag: TagWithCount; depth: number }> = [];
    for (const tag of tags) {
      // Autotag category roots (Topics, People, Locations, …) aren't meaningful
      // wiki subjects on their own — they're buckets for extracted tags. Skip
      // the bucket itself but surface its children at the current depth so the
      // real tags remain selectable.
      if (tag.is_autotag_target) {
        if (tag.children.length > 0) {
          result.push(...this.flattenTags(tag.children, depth));
        }
        continue;
      }
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
      this.loadError = null;
    } catch (e) {
      console.error("Atomic: Failed to load wiki article:", e);
      this.article = null;
      this.loadError = `Couldn't load article: ${e instanceof Error ? e.message : String(e)}`;
    }
    this.loading = false;
  }

  private renderContent(wrapper: HTMLElement): void {
    // Remove previous content (keep the select)
    const existing = wrapper.querySelector(".atomic-wiki-content, .atomic-wiki-empty, .atomic-wiki-actions");
    if (existing) existing.remove();

    // Also remove any additional content/action elements
    wrapper.querySelectorAll(".atomic-wiki-content, .atomic-wiki-empty, .atomic-wiki-actions").forEach((el) => el.remove());

    if (this.loadError) {
      wrapper.createDiv({ cls: "atomic-wiki-empty atomic-canvas-status-error", text: this.loadError });
      return;
    }

    if (this.loading) {
      const name = this.findTagName(this.selectedTagId ?? "") ?? "";
      this.renderGenerating(wrapper, name, this.generatingAtomCount);
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
        this.generatingAtomCount = this.findTagAtomCount(this.selectedTagId);
        this.loading = true;
        this.renderContent(wrapper);
        try {
          this.article = await this.client.generateWikiArticle(this.selectedTagId, tagName);
        } catch (e) {
          console.error("Atomic: Failed to generate wiki:", e);
          new Notice(`Couldn't generate article: ${e instanceof Error ? e.message : String(e)}`);
          this.article = null;
        } finally {
          this.loading = false;
          this.generatingAtomCount = null;
          this.renderContent(wrapper);
        }
      });
      return;
    }

    const contentEl = wrapper.createDiv({ cls: "atomic-wiki-content" });
    const rewritten = this.rewriteCitations(
      this.article.article.content,
      this.article.citations
    );
    const sourcePath = this.app.workspace.getActiveFile()?.path ?? "";
    MarkdownRenderer.render(this.app, rewritten, contentEl, sourcePath, this);

    // Obsidian's global click handler for .internal-link is scoped to
    // markdown file views, not custom ItemViews — wire it up manually.
    contentEl.addEventListener("click", async (evt) => {
      const target = evt.target as HTMLElement | null;
      const link = target?.closest("a") as HTMLAnchorElement | null;
      if (!link) return;
      evt.preventDefault();
      evt.stopPropagation();
      const href = link.getAttribute("data-href") ?? link.getAttribute("href");
      if (!href) return;

      // Cross-wiki references: the LLM emits `[[Other Topic]]` when another
      // wiki article exists for that tag. These look identical to Obsidian
      // wikilinks, so we check the tag list first and switch the view when
      // the target matches a known tag (case-insensitive, same as the core).
      const tagMatch = this.findTagByName(href);
      if (tagMatch) {
        this.selectedTagId = tagMatch.id;
        const select = wrapper.querySelector<HTMLSelectElement>(".atomic-wiki-tag-select");
        if (select) select.value = tagMatch.id;
        await this.loadArticle(tagMatch.id);
        this.renderContent(wrapper);
        return;
      }

      const newLeaf = evt.ctrlKey || evt.metaKey;
      this.app.workspace.openLinkText(href, sourcePath, newLeaf);
    }, true);
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
      // Use a wikilink alias so the citation renders as `[N]` inline
      // instead of the full note name, which was swamping the prose.
      // Obsidian resolves `[[target|alias]]` to the target file on click.
      return `[[${target}|${index}]]`;
    });
  }

  private renderGenerating(wrapper: HTMLElement, tagName: string, atomCount: number | null): void {
    const el = wrapper.createDiv({ cls: "atomic-wiki-generating" });

    const spinner = el.createDiv({ cls: "atomic-wiki-spinner" });
    setIcon(spinner, "loader-2");

    const title = tagName
      ? `Synthesizing article about "${tagName}"…`
      : "Loading…";
    el.createEl("h3", { cls: "atomic-wiki-generating-title", text: title });

    if (atomCount !== null && atomCount > 0) {
      el.createEl("p", {
        cls: "atomic-wiki-generating-sub",
        text: `Processing ${atomCount} source${atomCount === 1 ? "" : "s"}`,
      });
    }
    el.createEl("p", {
      cls: "atomic-wiki-generating-hint",
      text: "This may take a moment",
    });
  }

  private findTagByName(name: string): TagWithCount | null {
    const needle = name.trim().toLowerCase();
    const search = (nodes: TagWithCount[]): TagWithCount | null => {
      for (const node of nodes) {
        if (node.name.toLowerCase() === needle) return node;
        if (node.children.length > 0) {
          const found = search(node.children);
          if (found) return found;
        }
      }
      return null;
    };
    return search(this.tags);
  }

  private findTagAtomCount(tagId: string): number | null {
    const search = (nodes: TagWithCount[]): number | null => {
      for (const node of nodes) {
        if (node.id === tagId) return node.atom_count;
        if (node.children.length > 0) {
          const found = search(node.children);
          if (found !== null) return found;
        }
      }
      return null;
    };
    return search(this.tags);
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
