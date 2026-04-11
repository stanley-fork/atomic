import { create } from 'zustand';
import { getTransport } from '../lib/transport';

export interface Tag {
  id: string;
  name: string;
  parent_id: string | null;
  created_at: string;
  is_autotag_target: boolean;
}

export interface TagWithCount extends Tag {
  atom_count: number;
  children_total: number;
  children: TagWithCount[];
}

interface PaginatedTagChildren {
  children: TagWithCount[];
  total: number;
}

export interface CompactionResult {
  tags_moved: number;
  tags_merged: number;
  atoms_retagged: number;
}

const TAG_CHILDREN_PAGE_SIZE = 100;

interface TagsStore {
  tags: TagWithCount[];
  isLoading: boolean;
  isCompacting: boolean;
  error: string | null;
  fetchTags: () => Promise<void>;
  fetchTagChildren: (parentId: string) => Promise<void>;
  fetchMoreTagChildren: (parentId: string) => Promise<void>;
  createTag: (name: string, parentId?: string) => Promise<Tag>;
  updateTag: (id: string, name: string, parentId?: string) => Promise<Tag>;
  deleteTag: (id: string, recursive?: boolean) => Promise<void>;
  setTagAutotagTarget: (id: string, value: boolean) => Promise<void>;
  configureAutotagTargets: (keepDefaults: string[], addCustom: string[]) => Promise<Tag[]>;
  compactTags: () => Promise<CompactionResult>;
  clearError: () => void;
  reset: () => void;
}

function replaceChildrenInTree(
  nodes: TagWithCount[],
  parentId: string,
  newChildren: TagWithCount[],
  total: number,
): TagWithCount[] {
  return nodes.map((node) => {
    if (node.id === parentId) {
      return { ...node, children: newChildren, children_total: total };
    }
    if (node.children.length > 0) {
      return { ...node, children: replaceChildrenInTree(node.children, parentId, newChildren, total) };
    }
    return node;
  });
}

function appendChildrenInTree(
  nodes: TagWithCount[],
  parentId: string,
  moreChildren: TagWithCount[],
): TagWithCount[] {
  return nodes.map((node) => {
    if (node.id === parentId) {
      return { ...node, children: [...node.children, ...moreChildren] };
    }
    if (node.children.length > 0) {
      return { ...node, children: appendChildrenInTree(node.children, parentId, moreChildren) };
    }
    return node;
  });
}

function findTagInTree(nodes: TagWithCount[], tagId: string): TagWithCount | null {
  for (const node of nodes) {
    if (node.id === tagId) return node;
    if (node.children.length > 0) {
      const found = findTagInTree(node.children, tagId);
      if (found) return found;
    }
  }
  return null;
}

export const useTagsStore = create<TagsStore>((set, get) => ({
  tags: [],
  isLoading: false,
  isCompacting: false,
  error: null,

  fetchTags: async () => {
    set({ isLoading: true, error: null });
    try {
      const tags = await getTransport().invoke<TagWithCount[]>('get_all_tags');
      set({ tags, isLoading: false });
    } catch (error) {
      set({ error: String(error), isLoading: false });
    }
  },

  fetchTagChildren: async (parentId: string) => {
    try {
      const result = await getTransport().invoke<PaginatedTagChildren>('get_tag_children', {
        parentId,
        minCount: 0,
        limit: TAG_CHILDREN_PAGE_SIZE,
        offset: 0,
      });
      set((state) => ({
        tags: replaceChildrenInTree(state.tags, parentId, result.children, result.total),
      }));
    } catch (error) {
      set({ error: String(error) });
    }
  },

  fetchMoreTagChildren: async (parentId: string) => {
    try {
      const parent = findTagInTree(get().tags, parentId);
      if (!parent) return;
      const offset = parent.children.length;
      if (offset >= parent.children_total) return;

      const result = await getTransport().invoke<PaginatedTagChildren>('get_tag_children', {
        parentId,
        minCount: 0,
        limit: TAG_CHILDREN_PAGE_SIZE,
        offset,
      });
      set((state) => ({
        tags: appendChildrenInTree(state.tags, parentId, result.children),
      }));
    } catch (error) {
      set({ error: String(error) });
    }
  },

  createTag: async (name: string, parentId?: string) => {
    set({ error: null });
    try {
      const tag = await getTransport().invoke<Tag>('create_tag', {
        name,
        parentId: parentId || null,
      });
      // Refetch tags to get updated tree structure
      const tags = await getTransport().invoke<TagWithCount[]>('get_all_tags');
      set({ tags });
      return tag;
    } catch (error) {
      set({ error: String(error) });
      throw error;
    }
  },

  updateTag: async (id: string, name: string, parentId?: string) => {
    set({ error: null });
    try {
      const tag = await getTransport().invoke<Tag>('update_tag', {
        id,
        name,
        parentId: parentId || null,
      });
      // Refetch tags to get updated tree structure
      const tags = await getTransport().invoke<TagWithCount[]>('get_all_tags');
      set({ tags });
      return tag;
    } catch (error) {
      set({ error: String(error) });
      throw error;
    }
  },

  deleteTag: async (id: string, recursive?: boolean) => {
    set({ error: null });
    try {
      await getTransport().invoke('delete_tag', { id, recursive: recursive ?? false });
      // Refetch tags to get updated tree structure
      const tags = await getTransport().invoke<TagWithCount[]>('get_all_tags');
      set({ tags });
    } catch (error) {
      set({ error: String(error) });
      throw error;
    }
  },

  setTagAutotagTarget: async (id: string, value: boolean) => {
    set({ error: null });
    try {
      await getTransport().invoke('set_tag_autotag_target', { id, value });
      const tags = await getTransport().invoke<TagWithCount[]>('get_all_tags');
      set({ tags });
    } catch (error) {
      set({ error: String(error) });
      throw error;
    }
  },

  configureAutotagTargets: async (keepDefaults: string[], addCustom: string[]) => {
    set({ error: null });
    try {
      const created = await getTransport().invoke<Tag[]>('configure_autotag_targets', {
        keepDefaults,
        addCustom,
      });
      const tags = await getTransport().invoke<TagWithCount[]>('get_all_tags');
      set({ tags });
      return created;
    } catch (error) {
      set({ error: String(error) });
      throw error;
    }
  },

  compactTags: async () => {
    set({ isCompacting: true, error: null });
    try {
      const result = await getTransport().invoke<CompactionResult>('compact_tags');
      // Refetch tags to get updated tree structure
      const tags = await getTransport().invoke<TagWithCount[]>('get_all_tags');
      set({ tags, isCompacting: false });
      return result;
    } catch (error) {
      set({ error: String(error), isCompacting: false });
      throw error;
    }
  },

  clearError: () => set({ error: null }),

  reset: () => set({
    tags: [],
    isLoading: false,
    isCompacting: false,
    error: null,
  }),
}));
