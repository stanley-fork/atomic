import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import type { CanvasLevel } from '../lib/api';
import { getCanvasLevel } from '../lib/api';
import { navigateTo, navigateBack } from '../router/navigate-ref';
import { viewPath, atomReaderPath, wikiReaderPath, atomGraphPath } from '../router/routes';

export type DrawerMode = 'editor' | 'viewer' | 'wiki';
export type ViewMode = 'dashboard' | 'atoms' | 'canvas' | 'wiki';
export type AtomsLayout = 'grid' | 'list';

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

export type OverlayNavEntry =
  | { type: 'reader'; atomId: string; highlightText?: string | null }
  | { type: 'graph'; atomId: string }
  | { type: 'wiki'; tagId: string; tagName: string }

export interface OverlayNav {
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
  atomsLayout: AtomsLayout;
  searchQuery: string;
  loadingOperations: LoadingOperation[];
  // Panel state
  leftPanelOpen: boolean;
  leftPanelOpenBeforeReader: boolean;
  wikiSidebarOpen: boolean;
  // Chat sidebar state
  chatSidebarOpen: boolean;
  chatSidebarWidth: number;
  chatSidebarConversationId: string | null;
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
  setChatSidebarWidth: (width: number) => void;
  setChatSidebarConversationId: (id: string | null) => void;
  openChatSidebar: (tagId?: string, conversationId?: string) => void;
  clearChatSidebarInitial: () => void;
  setViewMode: (mode: ViewMode) => void;
  setAtomsLayout: (layout: AtomsLayout) => void;
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
    (set, get) => ({
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
      viewMode: 'atoms',
      atomsLayout: 'grid',
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
      chatSidebarWidth: 480,
      chatSidebarConversationId: null,
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

      setSelectedTag: (tagId: string | null) => {
        set({ selectedTagId: tagId });
        // Preserve the current route shape — if an overlay is open the tag
        // scope lives in its URL; otherwise it attaches to the current view.
        const state = get();
        if (state.readerState.atomId) {
          navigateTo(atomReaderPath(state.readerState.atomId, tagId), { replace: true });
        } else if (state.wikiReaderState.tagId) {
          // wiki reader's URL is already keyed on its own tagId; selectedTagId
          // as a scope-only concept doesn't apply here. Skip.
        } else {
          navigateTo(viewPath(state.viewMode, tagId), { replace: true });
        }
      },

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
        navigateTo(atomReaderPath(atomId, get().selectedTagId));
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
        navigateTo(atomReaderPath(atomId, get().selectedTagId));
      },

      setReaderEditState: (editing: boolean, saveStatus?: 'idle' | 'saving' | 'saved' | 'error') =>
        set((state) => ({
          readerState: { ...state.readerState, editing, ...(saveStatus !== undefined ? { saveStatus } : {}) },
        })),

      closeReader: () => {
        set({
          readerState: { atomId: null, highlightText: null, editing: false, saveStatus: 'idle' as const },
          wikiReaderState: { tagId: null, tagName: null },
          overlayNav: { stack: [], index: -1 },
        });
        const state = get();
        navigateBack(viewPath(state.viewMode, state.selectedTagId));
      },

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
        navigateTo(wikiReaderPath(tagId, tagName));
      },

      overlayNavigate: (entry: OverlayNavEntry) => {
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
        });
        const tagId = get().selectedTagId;
        if (entry.type === 'reader') {
          navigateTo(atomReaderPath(entry.atomId, tagId));
        } else if (entry.type === 'wiki') {
          navigateTo(wikiReaderPath(entry.tagId, entry.tagName));
        } else {
          navigateTo(atomGraphPath(entry.atomId, tagId));
        }
      },

      // Overlay back/forward are scoped to the *overlay session* (the
      // stack of reader/graph/wiki entries the user has navigated through
      // since opening the first overlay). They are NOT the same as the
      // browser's back button — the X button plays that role, closing the
      // whole overlay and returning to the parent view. Disable at stack
      // boundaries.
      //
      // `replace: true` is deliberate: chevron navigation within an open
      // overlay shouldn't pile up duplicate browser-history entries every
      // time you peek at a previous atom.
      overlayBack: () => {
        const state = get();
        const newIndex = state.overlayNav.index - 1;
        if (newIndex < 0) return;
        const entry = state.overlayNav.stack[newIndex];
        set({ overlayNav: { ...state.overlayNav, index: newIndex } });
        const tagId = state.selectedTagId;
        if (entry.type === 'reader') {
          navigateTo(atomReaderPath(entry.atomId, tagId), { replace: true });
        } else if (entry.type === 'wiki') {
          navigateTo(wikiReaderPath(entry.tagId, entry.tagName), { replace: true });
        } else {
          navigateTo(atomGraphPath(entry.atomId, tagId), { replace: true });
        }
      },

      overlayForward: () => {
        const state = get();
        const newIndex = state.overlayNav.index + 1;
        if (newIndex >= state.overlayNav.stack.length) return;
        const entry = state.overlayNav.stack[newIndex];
        set({ overlayNav: { ...state.overlayNav, index: newIndex } });
        const tagId = state.selectedTagId;
        if (entry.type === 'reader') {
          navigateTo(atomReaderPath(entry.atomId, tagId), { replace: true });
        } else if (entry.type === 'wiki') {
          navigateTo(wikiReaderPath(entry.tagId, entry.tagName), { replace: true });
        } else {
          navigateTo(atomGraphPath(entry.atomId, tagId), { replace: true });
        }
      },

      overlayDismiss: () => {
        // X closes the overlay outright — regardless of how deep the
        // in-overlay stack is. Use a direct navigate (not history.back)
        // so one tap always exits, even if the user has navigated through
        // several related atoms since opening the overlay.
        const state = get();
        navigateTo(viewPath(state.viewMode, state.selectedTagId));
      },


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
      setChatSidebarWidth: (width: number) => set({ chatSidebarWidth: Math.min(Math.max(width, 320), 800) }),
      setChatSidebarConversationId: (id: string | null) => set({ chatSidebarConversationId: id }),
      openChatSidebar: (tagId?: string, conversationId?: string) =>
        set({
          chatSidebarOpen: true,
          chatSidebarInitialTagId: tagId || null,
          chatSidebarInitialConversationId: conversationId || null,
        }),
      clearChatSidebarInitial: () =>
        set({ chatSidebarInitialTagId: null, chatSidebarInitialConversationId: null }),

      setViewMode: (mode: ViewMode) => {
        set({ viewMode: mode });
        navigateTo(viewPath(mode, get().selectedTagId));
      },

      setAtomsLayout: (layout: AtomsLayout) => set({ atomsLayout: layout }),

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
      openLocalGraph: (atomId: string, depth: 1 | 2 = 1) => {
        // Push a graph entry onto the overlay stack so it counts as
        // in-overlay navigation — chevron back from here returns to the
        // previous reader/wiki entry.
        set((state) => {
          const stack = state.overlayNav.stack.slice(0, state.overlayNav.index + 1);
          const isFirstOpen = state.overlayNav.index === -1;
          stack.push({ type: 'graph', atomId });
          return {
            localGraph: {
              isOpen: true,
              centerAtomId: atomId,
              depth,
              navigationHistory: [atomId],
            },
            readerState: { atomId: null, highlightText: null, editing: false, saveStatus: 'idle' as const },
            wikiReaderState: { tagId: null, tagName: null },
            overlayNav: { stack, index: stack.length - 1 },
            ...(isFirstOpen && state.leftPanelOpen ? { leftPanelOpen: false, leftPanelOpenBeforeReader: true } : {}),
          };
        });
        navigateTo(atomGraphPath(atomId, get().selectedTagId));
      },

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
      version: 1,
      partialize: (state) => ({
        viewMode: state.viewMode,
        atomsLayout: state.atomsLayout,
        readerTheme: state.readerTheme,
        chatSidebarOpen: state.chatSidebarOpen,
        chatSidebarWidth: state.chatSidebarWidth,
        chatSidebarConversationId: state.chatSidebarConversationId,
      }),
      // v0 → v1: 'grid' and 'list' were top-level ViewMode values. They're now
      // collapsed into a single 'atoms' view with a separate atomsLayout field.
      migrate: (persistedState: unknown, _version: number) => {
        const state = (persistedState ?? {}) as Record<string, unknown>;
        if (state.viewMode === 'grid' || state.viewMode === 'list') {
          state.atomsLayout = state.viewMode;
          state.viewMode = 'atoms';
        }
        return state;
      },
    }
  )
);

