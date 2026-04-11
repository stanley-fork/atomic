import { requestUrl, RequestUrlParam } from "obsidian";
import type { AtomicSettings } from "./settings";

// Response types matching atomic-server API

export interface Atom {
  id: string;
  content: string;
  title: string;
  snippet: string;
  source_url: string | null;
  source: string | null;
  published_at: string | null;
  created_at: string;
  updated_at: string;
  embedding_status: string;
  tagging_status: string;
}

export interface Tag {
  id: string;
  name: string;
  parent_id: string | null;
  created_at: string;
}

export interface AtomWithTags extends Atom {
  tags: Tag[];
}

export interface TagWithCount {
  id: string;
  name: string;
  parent_id: string | null;
  created_at: string;
  atom_count: number;
  children_total: number;
  children: TagWithCount[];
}

export interface SearchResult {
  id: string;
  content: string;
  title: string;
  snippet: string;
  source_url: string | null;
  source: string | null;
  created_at: string;
  updated_at: string;
  tags: Tag[];
  similarity_score: number;
  matching_chunk_content: string | null;
  matching_chunk_index: number | null;
}

export interface BulkCreateResult {
  atoms: AtomWithTags[];
  count: number;
  skipped: number;
}

export interface WikiArticle {
  tag_id: string;
  tag_name: string;
  content: string;
  atom_count: number;
  created_at: string;
  updated_at: string;
}

export interface WikiCitation {
  id: string;
  citation_index: number;
  atom_id: string;
  chunk_index: number | null;
  excerpt: string;
  /** Source URL of the cited atom (e.g. `obsidian://VaultName/path.md`), or null. */
  source_url: string | null;
}

export interface WikiArticleWithCitations {
  article: WikiArticle;
  citations: WikiCitation[];
}

export interface CreateAtomRequest {
  content: string;
  source_url?: string | null;
  published_at?: string | null;
  tag_ids?: string[];
  /** When true, the server skips creation if an atom with the same source_url already exists. */
  skip_if_source_exists?: boolean;
}

export interface UpdateAtomRequest {
  content: string;
  source_url?: string | null;
  published_at?: string | null;
  tag_ids?: string[] | null;
}

export class AtomicClient {
  private settings: AtomicSettings;

  constructor(settings: AtomicSettings) {
    this.settings = settings;
  }

  private get baseUrl(): string {
    return this.settings.serverUrl.replace(/\/+$/, "");
  }

  private get headers(): Record<string, string> {
    const headers: Record<string, string> = {
      Authorization: `Bearer ${this.settings.authToken}`,
      "Content-Type": "application/json",
    };
    if (this.settings.databaseName) {
      headers["X-Atomic-Database"] = this.settings.databaseName;
    }
    return headers;
  }

  private async request<T>(params: RequestUrlParam): Promise<T> {
    params.headers = { ...this.headers, ...params.headers };
    const response = await requestUrl(params);
    if (response.status >= 400) {
      const error = response.json?.error || `HTTP ${response.status}`;
      throw new Error(error);
    }
    return response.json as T;
  }

  async testConnection(): Promise<void> {
    await this.request({
      url: `${this.baseUrl}/api/settings`,
      method: "GET",
    });
  }

  // Atom CRUD

  async createAtom(request: CreateAtomRequest): Promise<AtomWithTags> {
    return this.request({
      url: `${this.baseUrl}/api/atoms`,
      method: "POST",
      body: JSON.stringify(request),
    });
  }

  async updateAtom(id: string, request: UpdateAtomRequest): Promise<AtomWithTags> {
    return this.request({
      url: `${this.baseUrl}/api/atoms/${id}`,
      method: "PUT",
      body: JSON.stringify(request),
    });
  }

  async deleteAtom(id: string): Promise<void> {
    await this.request({
      url: `${this.baseUrl}/api/atoms/${id}`,
      method: "DELETE",
    });
  }

  async getAtomBySourceUrl(url: string): Promise<AtomWithTags | null> {
    try {
      return await this.request({
        url: `${this.baseUrl}/api/atoms/by-source-url?url=${encodeURIComponent(url)}`,
        method: "GET",
      });
    } catch (e) {
      if (e instanceof Error && e.message.includes("404")) return null;
      // requestUrl throws on 4xx — check for "No atom found"
      if (e instanceof Error && e.message.includes("No atom found")) return null;
      throw e;
    }
  }

  async bulkCreateAtoms(atoms: CreateAtomRequest[]): Promise<BulkCreateResult> {
    return this.request({
      url: `${this.baseUrl}/api/atoms/bulk`,
      method: "POST",
      body: JSON.stringify(atoms),
    });
  }

  // Search

  async search(
    query: string,
    mode: "keyword" | "semantic" | "hybrid" = "hybrid",
    limit = 20
  ): Promise<SearchResult[]> {
    return this.request({
      url: `${this.baseUrl}/api/search`,
      method: "POST",
      body: JSON.stringify({ query, mode, limit }),
    });
  }

  async findSimilar(atomId: string, limit = 10): Promise<SearchResult[]> {
    return this.request({
      url: `${this.baseUrl}/api/atoms/${atomId}/similar?limit=${limit}`,
      method: "GET",
    });
  }

  // Tags

  async getTags(minCount = 0): Promise<TagWithCount[]> {
    return this.request({
      url: `${this.baseUrl}/api/tags?min_count=${minCount}`,
      method: "GET",
    });
  }

  async createTag(name: string, parentId?: string): Promise<Tag> {
    return this.request({
      url: `${this.baseUrl}/api/tags`,
      method: "POST",
      body: JSON.stringify({ name, parent_id: parentId ?? null }),
    });
  }

  // Wiki

  async getWikiArticle(tagId: string): Promise<WikiArticleWithCitations | null> {
    try {
      const result = await this.request<WikiArticleWithCitations | null>({
        url: `${this.baseUrl}/api/wiki/${tagId}`,
        method: "GET",
      });
      return result ?? null;
    } catch {
      return null;
    }
  }

  async generateWikiArticle(tagId: string, tagName: string): Promise<WikiArticleWithCitations> {
    return this.request({
      url: `${this.baseUrl}/api/wiki/${tagId}/generate`,
      method: "POST",
      body: JSON.stringify({ tag_name: tagName }),
    });
  }

  async getWikiSuggestions(query: string): Promise<TagWithCount[]> {
    return this.request({
      url: `${this.baseUrl}/api/wiki/suggestions?q=${encodeURIComponent(query)}`,
      method: "GET",
    });
  }
}
