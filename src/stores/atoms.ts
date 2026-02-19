import { create } from 'zustand';
import { getTransport } from '../lib/transport';

export interface Atom {
  id: string;
  content: string;
  title: string;
  snippet: string;
  source_url: string | null;
  created_at: string;
  updated_at: string;
  embedding_status: 'pending' | 'processing' | 'complete' | 'failed';
  tagging_status: 'pending' | 'processing' | 'complete' | 'failed' | 'skipped';
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

export interface AtomSummary {
  id: string;
  title: string;
  snippet: string;
  source_url: string | null;
  created_at: string;
  updated_at: string;
  embedding_status: 'pending' | 'processing' | 'complete' | 'failed';
  tagging_status: 'pending' | 'processing' | 'complete' | 'failed' | 'skipped';
  tags: Tag[];
}

export interface PaginatedAtoms {
  atoms: AtomSummary[];
  total_count: number;
  limit: number;
  offset: number;
  next_cursor?: string;
  next_cursor_id?: string;
}

export interface SemanticSearchResult {
  id: string;
  content: string;
  title: string;
  snippet: string;
  source_url: string | null;
  created_at: string;
  updated_at: string;
  embedding_status: 'pending' | 'processing' | 'complete' | 'failed';
  tagging_status: 'pending' | 'processing' | 'complete' | 'failed' | 'skipped';
  tags: Tag[];
  similarity_score: number;
  matching_chunk_content: string;
  matching_chunk_index: number;
}

export interface SimilarAtomResult {
  id: string;
  content: string;
  title: string;
  snippet: string;
  source_url: string | null;
  created_at: string;
  updated_at: string;
  embedding_status: 'pending' | 'processing' | 'complete' | 'failed';
  tagging_status: 'pending' | 'processing' | 'complete' | 'failed' | 'skipped';
  tags: Tag[];
  similarity_score: number;
  matching_chunk_content: string;
  matching_chunk_index: number;
}

export type SearchMode = 'keyword' | 'semantic' | 'hybrid';

// Union type for atoms displayed in grid/list — either summary or search result
export type DisplayAtom = AtomSummary | SemanticSearchResult;

const PAGE_SIZE = 50;

interface AtomsStore {
  atoms: AtomSummary[];
  totalCount: number;
  currentOffset: number;
  hasMore: boolean;
  currentTagFilter: string | null;
  isLoadingInitial: boolean;
  isLoadingMore: boolean;
  error: string | null;
  nextCursor: string | null;
  nextCursorId: string | null;

  // Search state
  searchMode: SearchMode;
  semanticSearchQuery: string;
  semanticSearchResults: SemanticSearchResult[] | null;  // null = not searching
  isSearching: boolean;

  // Existing methods
  fetchAtoms: () => Promise<void>;
  fetchAtomsByTag: (tagId: string) => Promise<void>;
  fetchNextPage: () => Promise<void>;
  createAtom: (content: string, sourceUrl?: string, tagIds?: string[]) => Promise<AtomWithTags>;
  updateAtom: (id: string, content: string, sourceUrl?: string, tagIds?: string[]) => Promise<AtomWithTags>;
  deleteAtom: (id: string) => Promise<void>;
  clearError: () => void;

  // New methods
  updateAtomStatus: (atomId: string, status: string) => void;
  batchUpdateAtomStatuses: (updates: Array<{atomId: string, status: string}>) => void;
  addAtom: (atom: AtomWithTags) => void;
  search: (query: string) => Promise<void>;
  clearSemanticSearch: () => void;
  setSemanticSearchQuery: (query: string) => void;
  setSearchMode: (mode: SearchMode) => void;
  retryEmbedding: (atomId: string) => Promise<void>;
}

/** Convert an AtomWithTags (full content) to AtomSummary shape for the store */
function toSummary(atom: AtomWithTags): AtomSummary {
  return {
    id: atom.id,
    title: atom.title,
    snippet: atom.snippet,
    source_url: atom.source_url,
    created_at: atom.created_at,
    updated_at: atom.updated_at,
    embedding_status: atom.embedding_status,
    tagging_status: atom.tagging_status,
    tags: atom.tags,
  };
}

export const useAtomsStore = create<AtomsStore>((set, get) => ({
  atoms: [],
  totalCount: 0,
  currentOffset: 0,
  hasMore: true,
  currentTagFilter: null,
  isLoadingInitial: false,
  isLoadingMore: false,
  error: null,
  nextCursor: null,
  nextCursorId: null,

  // Search state
  searchMode: 'hybrid' as SearchMode,
  semanticSearchQuery: '',
  semanticSearchResults: null,
  isSearching: false,

  fetchAtoms: async () => {
    set({ atoms: [], isLoadingInitial: true, error: null, currentTagFilter: null, currentOffset: 0, nextCursor: null, nextCursorId: null });
    try {
      const result = await getTransport().invoke<PaginatedAtoms>('list_atoms', {
        limit: PAGE_SIZE,
        offset: 0,
      });
      set({
        atoms: result.atoms,
        totalCount: result.total_count,
        currentOffset: result.atoms.length,
        hasMore: result.atoms.length < result.total_count,
        isLoadingInitial: false,
        nextCursor: result.next_cursor ?? null,
        nextCursorId: result.next_cursor_id ?? null,
      });
    } catch (error) {
      set({ error: String(error), isLoadingInitial: false });
    }
  },

  fetchAtomsByTag: async (tagId: string) => {
    set({ atoms: [], isLoadingInitial: true, error: null, currentTagFilter: tagId, currentOffset: 0, nextCursor: null, nextCursorId: null });
    try {
      const result = await getTransport().invoke<PaginatedAtoms>('list_atoms', {
        tagId,
        limit: PAGE_SIZE,
        offset: 0,
      });
      set({
        atoms: result.atoms,
        totalCount: result.total_count,
        currentOffset: result.atoms.length,
        hasMore: result.atoms.length < result.total_count,
        isLoadingInitial: false,
        nextCursor: result.next_cursor ?? null,
        nextCursorId: result.next_cursor_id ?? null,
      });
    } catch (error) {
      set({ error: String(error), isLoadingInitial: false });
    }
  },

  fetchNextPage: async () => {
    const { hasMore, isLoadingMore, currentTagFilter, nextCursor, nextCursorId } = get();
    if (!hasMore || isLoadingMore) return;

    set({ isLoadingMore: true });
    try {
      const args: Record<string, unknown> = {
        limit: PAGE_SIZE,
        offset: 0,
      };
      if (currentTagFilter) args.tagId = currentTagFilter;
      if (nextCursor && nextCursorId) {
        args.cursor = nextCursor;
        args.cursorId = nextCursorId;
      }

      const result = await getTransport().invoke<PaginatedAtoms>('list_atoms', args);
      set((state) => {
        const newAtoms = [...state.atoms, ...result.atoms];
        return {
          atoms: newAtoms,
          totalCount: result.total_count,
          currentOffset: newAtoms.length,
          hasMore: newAtoms.length < result.total_count,
          isLoadingMore: false,
          nextCursor: result.next_cursor ?? null,
          nextCursorId: result.next_cursor_id ?? null,
        };
      });
    } catch (error) {
      set({ error: String(error), isLoadingMore: false });
    }
  },

  createAtom: async (content: string, sourceUrl?: string, tagIds?: string[]) => {
    set({ error: null });
    try {
      const atom = await getTransport().invoke<AtomWithTags>('create_atom', {
        content,
        sourceUrl: sourceUrl || null,
        tagIds: tagIds || [],
      });
      // Prepend summary to list and bump total count
      set((state) => ({
        atoms: [toSummary(atom), ...state.atoms],
        totalCount: state.totalCount + 1,
      }));
      return atom;
    } catch (error) {
      set({ error: String(error) });
      throw error;
    }
  },

  updateAtom: async (id: string, content: string, sourceUrl?: string, tagIds?: string[]) => {
    set({ error: null });
    try {
      const atom = await getTransport().invoke<AtomWithTags>('update_atom', {
        id,
        content,
        sourceUrl: sourceUrl || null,
        tagIds: tagIds || [],
      });
      const summary = toSummary(atom);
      set((state) => ({
        atoms: state.atoms.map((a) => (a.id === id ? summary : a)),
      }));
      return atom;
    } catch (error) {
      set({ error: String(error) });
      throw error;
    }
  },

  deleteAtom: async (id: string) => {
    set({ error: null });
    try {
      await getTransport().invoke('delete_atom', { id });
      set((state) => ({
        atoms: state.atoms.filter((a) => a.id !== id),
        totalCount: Math.max(0, state.totalCount - 1),
      }));
    } catch (error) {
      set({ error: String(error) });
      throw error;
    }
  },

  clearError: () => set({ error: null }),

  // New methods
  updateAtomStatus: (atomId: string, status: string) => {
    set((state) => ({
      atoms: state.atoms.map((a) =>
        a.id === atomId
          ? { ...a, embedding_status: status as AtomSummary['embedding_status'] }
          : a
      ),
    }));
  },

  batchUpdateAtomStatuses: (updates: Array<{atomId: string, status: string}>) => {
    if (updates.length === 0) return;
    const updateMap = new Map(updates.map(u => [u.atomId, u.status]));
    set((state) => ({
      atoms: state.atoms.map((a) => {
        const newStatus = updateMap.get(a.id);
        return newStatus
          ? { ...a, embedding_status: newStatus as AtomSummary['embedding_status'] }
          : a;
      }),
    }));
  },

  addAtom: (atom: AtomWithTags) => {
    set((state) => {
      // Skip if atom already exists (e.g., same-session create already added it)
      if (state.atoms.some(a => a.id === atom.id)) return state;
      return {
        atoms: [toSummary(atom), ...state.atoms],
        totalCount: state.totalCount + 1,
      };
    });
  },

  search: async (query: string) => {
    const { searchMode } = get();
    set({ isSearching: true, error: null, semanticSearchQuery: query });
    try {
      let results: SemanticSearchResult[];

      switch (searchMode) {
        case 'keyword':
          results = await getTransport().invoke<SemanticSearchResult[]>('search_atoms_keyword', {
            query,
            limit: 20,
          });
          break;
        case 'semantic':
          results = await getTransport().invoke<SemanticSearchResult[]>('search_atoms_semantic', {
            query,
            limit: 20,
            threshold: 0.4,
          });
          break;
        case 'hybrid':
        default:
          results = await getTransport().invoke<SemanticSearchResult[]>('search_atoms_hybrid', {
            query,
            limit: 20,
            threshold: 0.4,
          });
          break;
      }

      set({ semanticSearchResults: results, isSearching: false });
    } catch (error) {
      set({ error: String(error), isSearching: false });
    }
  },

  clearSemanticSearch: () => {
    set({
      semanticSearchResults: null,
      semanticSearchQuery: '',
    });
  },

  setSemanticSearchQuery: (query: string) => {
    set({ semanticSearchQuery: query });
  },

  setSearchMode: (mode: SearchMode) => {
    set({ searchMode: mode });
  },

  retryEmbedding: async (atomId: string) => {
    set({ error: null });
    try {
      await getTransport().invoke('retry_embedding', { atomId });
      // Update the atom status to 'pending' optimistically
      set((state) => ({
        atoms: state.atoms.map((a) =>
          a.id === atomId ? { ...a, embedding_status: 'pending' as const } : a
        ),
      }));
    } catch (error) {
      set({ error: String(error) });
      throw error;
    }
  },
}));
