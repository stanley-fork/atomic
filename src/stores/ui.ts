import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import type { CanvasLevel } from '../lib/api';
import { getCanvasLevel } from '../lib/api';

export type DrawerMode = 'editor' | 'viewer' | 'wiki';
export type ViewMode = 'grid' | 'list' | 'canvas' | 'wiki';

interface DrawerState {
  isOpen: boolean;
  mode: DrawerMode;
  atomId: string | null;      // For editor/viewer modes
  tagId: string | null;       // For wiki mode
  tagName: string | null;     // For wiki mode (display purposes)
  highlightText: string | null;   // For viewer mode (text to highlight and scroll to)
}

interface LocalGraphState {
  isOpen: boolean;
  centerAtomId: string | null;
  depth: 1 | 2;
  navigationHistory: string[];  // For breadcrumb navigation
}

export interface LoadingOperation {
  id: string;
  message: string;
  timestamp: number;
}

interface CanvasNavState {
  currentLevel: CanvasLevel | null;
  isLoading: boolean;
}

interface ReaderState {
  atomId: string | null;
  highlightText: string | null;
  editing: boolean;
  saveStatus: 'idle' | 'saving' | 'saved' | 'error';
}

interface WikiReaderState {
  tagId: string | null;
  tagName: string | null;
}

type OverlayNavEntry =
  | { type: 'reader'; atomId: string; highlightText?: string | null }
  | { type: 'graph'; atomId: string }
  | { type: 'wiki'; tagId: string; tagName: string }

interface OverlayNav {
  stack: OverlayNavEntry[];
  index: number; // -1 = no overlay open
}

interface UIStore {
  selectedTagId: string | null;
  expandedTagIds: Record<string, boolean>;  // Tags that should be expanded in sidebar
  drawerState: DrawerState;
  readerState: ReaderState;
  wikiReaderState: WikiReaderState;
  overlayNav: OverlayNav;
  viewMode: ViewMode;
  searchQuery: string;
  loadingOperations: LoadingOperation[];
  // Panel state
  leftPanelOpen: boolean;
  leftPanelOpenBeforeReader: boolean;
  wikiSidebarOpen: boolean;
  // Chat sidebar state
  chatSidebarOpen: boolean;
  chatSidebarInitialTagId: string | null;
  chatSidebarInitialConversationId: string | null;
  // Server connection state
  serverConnected: boolean;
  // Local graph state
  localGraph: LocalGraphState;
  highlightedAtomId: string | null;
  // Command palette state
  commandPaletteOpen: boolean;
  commandPaletteInitialQuery: string;
  // Reader theme
  readerTheme: 'light' | 'dark';
  // Canvas navigation state
  canvasNav: CanvasNavState;
  // Actions
  setServerConnected: (connected: boolean) => void;
  setLeftPanelOpen: (open: boolean) => void;
  toggleLeftPanel: () => void;
  setWikiSidebarOpen: (open: boolean) => void;
  toggleWikiSidebar: () => void;
  setSelectedTag: (tagId: string | null) => void;
  expandTagPath: (tagIds: string[]) => void;  // Expand all tags in path
  toggleTagExpanded: (tagId: string) => void;
  openReader: (atomId: string, highlightText?: string) => void;
  openReaderEditing: (atomId: string) => void;
  setReaderEditState: (editing: boolean, saveStatus?: 'idle' | 'saving' | 'saved' | 'error') => void;
  closeReader: () => void;
  openWikiReader: (tagId: string, tagName: string) => void;
  overlayNavigate: (entry: OverlayNavEntry) => void;
  overlayBack: () => void;
  overlayForward: () => void;
  overlayDismiss: () => void;
  openDrawer: (mode: DrawerMode, atomId?: string, highlightText?: string) => void;
  openWikiDrawer: (tagId: string, tagName: string) => void;
  openWikiListDrawer: () => void;
  closeDrawer: () => void;
  // Chat sidebar actions
  toggleChatSidebar: () => void;
  setChatSidebarOpen: (open: boolean) => void;
  openChatSidebar: (tagId?: string, conversationId?: string) => void;
  clearChatSidebarInitial: () => void;
  setViewMode: (mode: ViewMode) => void;
  setSearchQuery: (query: string) => void;
  addLoadingOperation: (id: string, message: string) => void;
  removeLoadingOperation: (id: string) => void;
  // Local graph actions
  openLocalGraph: (atomId: string, depth?: 1 | 2) => void;
  navigateLocalGraph: (atomId: string) => void;
  goBackLocalGraph: () => void;
  closeLocalGraph: () => void;
  setLocalGraphDepth: (depth: 1 | 2) => void;
  setHighlightedAtom: (atomId: string | null) => void;
  // Command palette actions
  openCommandPalette: (initialQuery?: string) => void;
  closeCommandPalette: () => void;
  toggleCommandPalette: () => void;
  setReaderTheme: (theme: 'light' | 'dark') => void;
  toggleReaderTheme: () => void;
  // Canvas navigation actions
  navigateCanvas: (parentId: string | null, childrenHint?: string[]) => Promise<void>;
}

export const useUIStore = create<UIStore>()(
  persist(
    (set) => ({
      selectedTagId: null,
      expandedTagIds: {} as Record<string, boolean>,
      drawerState: {
        isOpen: false,
        mode: 'viewer',
        atomId: null,
        tagId: null,
        tagName: null,
        highlightText: null,
      },
      readerState: {
        atomId: null,
        highlightText: null,
        editing: false,
        saveStatus: 'idle' as const,
      },
      wikiReaderState: {
        tagId: null,
        tagName: null,
      },
      overlayNav: {
        stack: [],
        index: -1,
      },
      viewMode: 'grid',
      searchQuery: '',
      loadingOperations: [],
      localGraph: {
        isOpen: false,
        centerAtomId: null,
        depth: 1,
        navigationHistory: [],
      },
      highlightedAtomId: null,
      leftPanelOpen: true,
      leftPanelOpenBeforeReader: false,
      wikiSidebarOpen: true,
      chatSidebarOpen: false,
      chatSidebarInitialTagId: null,
      chatSidebarInitialConversationId: null,
      serverConnected: false,
      commandPaletteOpen: false,
      commandPaletteInitialQuery: '',
      readerTheme: 'dark' as 'light' | 'dark',
      canvasNav: {
        currentLevel: null,
        isLoading: false,
      },

      setLeftPanelOpen: (open: boolean) => set({ leftPanelOpen: open }),
      toggleLeftPanel: () => set((state) => ({ leftPanelOpen: !state.leftPanelOpen })),
      setWikiSidebarOpen: (open: boolean) => set({ wikiSidebarOpen: open }),
      toggleWikiSidebar: () => set((state) => ({ wikiSidebarOpen: !state.wikiSidebarOpen })),
      setServerConnected: (connected: boolean) => set({ serverConnected: connected }),

      setSelectedTag: (tagId: string | null) => set({ selectedTagId: tagId }),

      expandTagPath: (tagIds: string[]) =>
        set((state) => {
          const updated = { ...state.expandedTagIds };
          for (const id of tagIds) {
            updated[id] = true;
          }
          return { expandedTagIds: updated };
        }),

      toggleTagExpanded: (tagId: string) =>
        set((state) => ({
          expandedTagIds: {
            ...state.expandedTagIds,
            [tagId]: !state.expandedTagIds[tagId],
          },
        })),

      openReader: (atomId: string, highlightText?: string) => {
        const entry: OverlayNavEntry = { type: 'reader', atomId, highlightText };
        set((state) => {
          const stack = state.overlayNav.stack.slice(0, state.overlayNav.index + 1);
          const isFirstOpen = state.overlayNav.index === -1;
          stack.push(entry);
          return {
            readerState: { atomId, highlightText: highlightText || null, editing: false, saveStatus: 'idle' as const },
            wikiReaderState: { tagId: null, tagName: null },
            overlayNav: { stack, index: stack.length - 1 },
            localGraph: { ...state.localGraph, isOpen: false },
            ...(isFirstOpen && state.leftPanelOpen ? { leftPanelOpen: false, leftPanelOpenBeforeReader: true } : {}),
          };
        });
      },

      openReaderEditing: (atomId: string) => {
        const entry: OverlayNavEntry = { type: 'reader', atomId };
        set((state) => {
          const stack = state.overlayNav.stack.slice(0, state.overlayNav.index + 1);
          const isFirstOpen = state.overlayNav.index === -1;
          stack.push(entry);
          return {
            readerState: { atomId, highlightText: null, editing: true, saveStatus: 'idle' as const },
            wikiReaderState: { tagId: null, tagName: null },
            overlayNav: { stack, index: stack.length - 1 },
            localGraph: { ...state.localGraph, isOpen: false },
            ...(isFirstOpen && state.leftPanelOpen ? { leftPanelOpen: false, leftPanelOpenBeforeReader: true } : {}),
          };
        });
      },

      setReaderEditState: (editing: boolean, saveStatus?: 'idle' | 'saving' | 'saved' | 'error') =>
        set((state) => ({
          readerState: { ...state.readerState, editing, ...(saveStatus !== undefined ? { saveStatus } : {}) },
        })),

      closeReader: () =>
        set({
          readerState: { atomId: null, highlightText: null, editing: false, saveStatus: 'idle' as const },
          wikiReaderState: { tagId: null, tagName: null },
          overlayNav: { stack: [], index: -1 },
        }),

      openWikiReader: (tagId: string, tagName: string) => {
        const entry: OverlayNavEntry = { type: 'wiki', tagId, tagName };
        set((state) => {
          const stack = state.overlayNav.stack.slice(0, state.overlayNav.index + 1);
          const isFirstOpen = state.overlayNav.index === -1;
          stack.push(entry);
          return {
            wikiReaderState: { tagId, tagName },
            readerState: { atomId: null, highlightText: null, editing: false, saveStatus: 'idle' as const },
            overlayNav: { stack, index: stack.length - 1 },
            localGraph: { ...state.localGraph, isOpen: false },
            ...(isFirstOpen && state.leftPanelOpen ? { leftPanelOpen: false, leftPanelOpenBeforeReader: true } : {}),
          };
        });
      },

      overlayNavigate: (entry: OverlayNavEntry) =>
        set((state) => {
          const stack = state.overlayNav.stack.slice(0, state.overlayNav.index + 1);
          stack.push(entry);
          const index = stack.length - 1;
          // Sync readerState, wikiReaderState, and localGraph based on entry type
          if (entry.type === 'reader') {
            return {
              overlayNav: { stack, index },
              readerState: { atomId: entry.atomId, highlightText: entry.highlightText || null, editing: false, saveStatus: 'idle' as const },
              wikiReaderState: { tagId: null, tagName: null },
              localGraph: { ...state.localGraph, isOpen: false },
            };
          } else if (entry.type === 'wiki') {
            return {
              overlayNav: { stack, index },
              readerState: { atomId: null, highlightText: null, editing: false, saveStatus: 'idle' as const },
              wikiReaderState: { tagId: entry.tagId, tagName: entry.tagName },
              localGraph: { ...state.localGraph, isOpen: false },
            };
          } else {
            return {
              overlayNav: { stack, index },
              readerState: { atomId: null, highlightText: null, editing: false, saveStatus: 'idle' as const },
              wikiReaderState: { tagId: null, tagName: null },
              localGraph: { isOpen: true, centerAtomId: entry.atomId, depth: 1, navigationHistory: [entry.atomId] },
            };
          }
        }),

      overlayBack: () =>
        set((state) => {
          const newIndex = state.overlayNav.index - 1;
          if (newIndex < 0) {
            // Nothing to go back to — dismiss
            return {
              overlayNav: { stack: [], index: -1 },
              readerState: { atomId: null, highlightText: null, editing: false, saveStatus: 'idle' as const },
              wikiReaderState: { tagId: null, tagName: null },
              localGraph: { ...state.localGraph, isOpen: false },
              ...(state.leftPanelOpenBeforeReader ? { leftPanelOpen: true, leftPanelOpenBeforeReader: false } : {}),
            };
          }
          const entry = state.overlayNav.stack[newIndex];
          if (entry.type === 'reader') {
            return {
              overlayNav: { ...state.overlayNav, index: newIndex },
              readerState: { atomId: entry.atomId, highlightText: entry.highlightText || null, editing: false, saveStatus: 'idle' as const },
              wikiReaderState: { tagId: null, tagName: null },
              localGraph: { ...state.localGraph, isOpen: false },
            };
          } else if (entry.type === 'wiki') {
            return {
              overlayNav: { ...state.overlayNav, index: newIndex },
              readerState: { atomId: null, highlightText: null, editing: false, saveStatus: 'idle' as const },
              wikiReaderState: { tagId: entry.tagId, tagName: entry.tagName },
              localGraph: { ...state.localGraph, isOpen: false },
            };
          } else {
            return {
              overlayNav: { ...state.overlayNav, index: newIndex },
              readerState: { atomId: null, highlightText: null, editing: false, saveStatus: 'idle' as const },
              wikiReaderState: { tagId: null, tagName: null },
              localGraph: { isOpen: true, centerAtomId: entry.atomId, depth: 1, navigationHistory: [entry.atomId] },
            };
          }
        }),

      overlayForward: () =>
        set((state) => {
          const newIndex = state.overlayNav.index + 1;
          if (newIndex >= state.overlayNav.stack.length) return {};
          const entry = state.overlayNav.stack[newIndex];
          if (entry.type === 'reader') {
            return {
              overlayNav: { ...state.overlayNav, index: newIndex },
              readerState: { atomId: entry.atomId, highlightText: entry.highlightText || null, editing: false, saveStatus: 'idle' as const },
              wikiReaderState: { tagId: null, tagName: null },
              localGraph: { ...state.localGraph, isOpen: false },
            };
          } else if (entry.type === 'wiki') {
            return {
              overlayNav: { ...state.overlayNav, index: newIndex },
              readerState: { atomId: null, highlightText: null, editing: false, saveStatus: 'idle' as const },
              wikiReaderState: { tagId: entry.tagId, tagName: entry.tagName },
              localGraph: { ...state.localGraph, isOpen: false },
            };
          } else {
            return {
              overlayNav: { ...state.overlayNav, index: newIndex },
              readerState: { atomId: null, highlightText: null, editing: false, saveStatus: 'idle' as const },
              wikiReaderState: { tagId: null, tagName: null },
              localGraph: { isOpen: true, centerAtomId: entry.atomId, depth: 1, navigationHistory: [entry.atomId] },
            };
          }
        }),

      overlayDismiss: () =>
        set((state) => ({
          overlayNav: { stack: [], index: -1 },
          readerState: { atomId: null, highlightText: null, editing: false, saveStatus: 'idle' as const },
          wikiReaderState: { tagId: null, tagName: null },
          localGraph: { ...state.localGraph, isOpen: false },
          ...(state.leftPanelOpenBeforeReader ? { leftPanelOpen: true, leftPanelOpenBeforeReader: false } : {}),
        })),


      openDrawer: (mode: DrawerMode, atomId?: string, highlightText?: string) =>
        set({
          drawerState: {
            isOpen: true,
            mode,
            atomId: atomId || null,
            tagId: null,
            tagName: null,
            highlightText: highlightText || null,
          },
        }),

      openWikiDrawer: (tagId: string, tagName: string) =>
        set({
          drawerState: {
            isOpen: true,
            mode: 'wiki',
            atomId: null,
            tagId,
            tagName,
            highlightText: null,
          },
        }),

      openWikiListDrawer: () =>
        set({
          drawerState: {
            isOpen: true,
            mode: 'wiki',
            atomId: null,
            tagId: null,
            tagName: null,
            highlightText: null,
          },
        }),

      closeDrawer: () =>
        set((state) => ({
          drawerState: {
            ...state.drawerState,
            isOpen: false,
            highlightText: null,
          },
        })),

      // Chat sidebar actions
      toggleChatSidebar: () => set((state) => ({ chatSidebarOpen: !state.chatSidebarOpen })),
      setChatSidebarOpen: (open: boolean) => set({ chatSidebarOpen: open }),
      openChatSidebar: (tagId?: string, conversationId?: string) =>
        set({
          chatSidebarOpen: true,
          chatSidebarInitialTagId: tagId || null,
          chatSidebarInitialConversationId: conversationId || null,
        }),
      clearChatSidebarInitial: () =>
        set({ chatSidebarInitialTagId: null, chatSidebarInitialConversationId: null }),

      setViewMode: (mode: ViewMode) => set({
        viewMode: mode,
      }),

      setSearchQuery: (query: string) => set({ searchQuery: query }),

      addLoadingOperation: (id: string, message: string) =>
        set((state) => ({
          loadingOperations: [
            ...state.loadingOperations,
            { id, message, timestamp: Date.now() },
          ],
        })),

      removeLoadingOperation: (id: string) =>
        set((state) => ({
          loadingOperations: state.loadingOperations.filter((op) => op.id !== id),
        })),

      // Local graph actions
      openLocalGraph: (atomId: string, depth: 1 | 2 = 1) =>
        set({
          localGraph: {
            isOpen: true,
            centerAtomId: atomId,
            depth,
            navigationHistory: [atomId],
          },
        }),

      navigateLocalGraph: (atomId: string) =>
        set((state) => ({
          localGraph: {
            ...state.localGraph,
            centerAtomId: atomId,
            navigationHistory: [...state.localGraph.navigationHistory, atomId],
          },
        })),

      goBackLocalGraph: () =>
        set((state) => {
          const history = [...state.localGraph.navigationHistory];
          history.pop(); // Remove current
          const previousAtomId = history[history.length - 1] || null;
          return {
            localGraph: {
              ...state.localGraph,
              centerAtomId: previousAtomId,
              navigationHistory: history,
              isOpen: history.length > 0,
            },
          };
        }),

      closeLocalGraph: () =>
        set({
          localGraph: {
            isOpen: false,
            centerAtomId: null,
            depth: 1,
            navigationHistory: [],
          },
        }),

      setLocalGraphDepth: (depth: 1 | 2) =>
        set((state) => ({
          localGraph: {
            ...state.localGraph,
            depth,
          },
        })),

      setHighlightedAtom: (atomId: string | null) =>
        set({ highlightedAtomId: atomId }),

      // Command palette actions
      openCommandPalette: (initialQuery?: string) => set({
        commandPaletteOpen: true,
        commandPaletteInitialQuery: initialQuery || ''
      }),
      closeCommandPalette: () => set({
        commandPaletteOpen: false,
        commandPaletteInitialQuery: ''
      }),
      toggleCommandPalette: () =>
        set((state) => ({
          commandPaletteOpen: !state.commandPaletteOpen,
          commandPaletteInitialQuery: state.commandPaletteOpen ? '' : state.commandPaletteInitialQuery
        })),

      setReaderTheme: (theme: 'light' | 'dark') => set({ readerTheme: theme }),
      toggleReaderTheme: () => set((state) => ({ readerTheme: state.readerTheme === 'dark' ? 'light' : 'dark' })),

      navigateCanvas: async (parentId: string | null, childrenHint?: string[]) => {
        set({ canvasNav: { currentLevel: null, isLoading: true } });
        try {
          const level = await getCanvasLevel(parentId, childrenHint);
          set({ canvasNav: { currentLevel: level, isLoading: false } });
        } catch (err) {
          console.error('Failed to load canvas level:', err);
          set({ canvasNav: { currentLevel: null, isLoading: false } });
        }
      },
    }),
    {
      name: 'atomic-ui-storage',
      partialize: (state) => ({ viewMode: state.viewMode, readerTheme: state.readerTheme, chatSidebarOpen: state.chatSidebarOpen }),
    }
  )
);

