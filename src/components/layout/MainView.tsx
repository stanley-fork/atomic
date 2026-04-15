import { useMemo, useCallback, useEffect, useRef, useState } from 'react';
import { useShallow } from 'zustand/react/shallow';
import {
  PanelLeft,
  X,
  Undo2,
  Redo2,
  ChevronLeft,
  ChevronRight,
  Sun,
  Moon,
  Check,
  Pencil,
  Trash2,
  MoreHorizontal,
  MessageCircle,
  LayoutDashboard,
  LayoutGrid,
  List as ListIcon,
  Library,
  Network,
  BookOpen,
  Search,
  Filter,
} from 'lucide-react';
import { AtomGrid } from '../atoms/AtomGrid';
import { AtomList } from '../atoms/AtomList';
import { AtomReader } from '../atoms/AtomReader';
import { FilterBar } from '../atoms/FilterBar';
import { FilterSheet } from '../atoms/FilterSheet';
import { SigmaCanvas } from '../canvas/SigmaCanvas';
import { LocalGraphView } from '../canvas/LocalGraphView';
import { DashboardView } from '../dashboard/DashboardView';
import { FAB } from '../ui/FAB';
import { Modal } from '../ui/Modal';
import { EmbeddingProgressBanner } from '../ui/EmbeddingProgressBanner';
import { WikiFullView } from '../wiki/WikiFullView';
import { WikiReader } from '../wiki/WikiReader';
import { ChatViewer } from '../chat/ChatViewer';
import { useAtomsStore } from '../../stores/atoms';
import { useTagsStore } from '../../stores/tags';
import { useUIStore } from '../../stores/ui';
import { isTauri } from '../../lib/platform';
import { useIsMobile } from '../../hooks';
import { readerEditorActions } from '../../lib/reader-editor-bridge';

export function MainView() {
  const atoms = useAtomsStore(s => s.atoms);
  const totalCount = useAtomsStore(s => s.totalCount);
  const hasMore = useAtomsStore(s => s.hasMore);
  const isLoadingInitial = useAtomsStore(s => s.isLoadingInitial);
  const isLoadingMore = useAtomsStore(s => s.isLoadingMore);
  const fetchNextPage = useAtomsStore(s => s.fetchNextPage);
  const semanticSearchResults = useAtomsStore(s => s.semanticSearchResults);
  const semanticSearchQuery = useAtomsStore(s => s.semanticSearchQuery);
  const retryEmbedding = useAtomsStore(s => s.retryEmbedding);
  const retryTagging = useAtomsStore(s => s.retryTagging);
  const sourceFilter = useAtomsStore(s => s.sourceFilter);
  const sourceValue = useAtomsStore(s => s.sourceValue);
  const sortBy = useAtomsStore(s => s.sortBy);
  const sortOrder = useAtomsStore(s => s.sortOrder);
  const search = useAtomsStore(s => s.search);
  const clearSemanticSearch = useAtomsStore(s => s.clearSemanticSearch);

  const { viewMode, atomsLayout, searchQuery } = useUIStore(
    useShallow(s => ({
      viewMode: s.viewMode,
      atomsLayout: s.atomsLayout,
      searchQuery: s.searchQuery,
    }))
  );
  const leftPanelOpen = useUIStore(s => s.leftPanelOpen);
  const toggleLeftPanel = useUIStore(s => s.toggleLeftPanel);
  const setViewMode = useUIStore(s => s.setViewMode);
  const setAtomsLayout = useUIStore(s => s.setAtomsLayout);
  const openReader = useUIStore(s => s.openReader);
  const readerState = useUIStore(s => s.readerState);
  const wikiReaderState = useUIStore(s => s.wikiReaderState);
  const localGraph = useUIStore(s => s.localGraph);
  const overlayNav = useUIStore(s => s.overlayNav);
  const overlayBack = useUIStore(s => s.overlayBack);
  const overlayForward = useUIStore(s => s.overlayForward);
  const overlayDismiss = useUIStore(s => s.overlayDismiss);
  const readerTheme = useUIStore(s => s.readerTheme);
  const toggleReaderTheme = useUIStore(s => s.toggleReaderTheme);
  const deleteAtom = useAtomsStore(s => s.deleteAtom);
  const fetchTags = useTagsStore(s => s.fetchTags);

  const openCommandPalette = useUIStore(s => s.openCommandPalette);

  const chatSidebarOpen = useUIStore(s => s.chatSidebarOpen);
  const chatSidebarWidth = useUIStore(s => s.chatSidebarWidth);
  const setChatSidebarWidth = useUIStore(s => s.setChatSidebarWidth);
  const toggleChatSidebar = useUIStore(s => s.toggleChatSidebar);
  const [isResizingChat, setIsResizingChat] = useState(false);

  const [filterBarOpen, setFilterBarOpen] = useState(false);
  const [showDeleteModal, setShowDeleteModal] = useState(false);
  const [isDeleting, setIsDeleting] = useState(false);
  const [readerMenuOpen, setReaderMenuOpen] = useState(false);
  const readerMenuRef = useRef<HTMLDivElement>(null);
  const isMobile = useIsMobile();
  const hasActiveFilter = sourceFilter !== 'all' || !!sourceValue || sortBy !== 'updated' || sortOrder !== 'desc';

  // Close reader overflow menu on outside click / escape
  useEffect(() => {
    if (!readerMenuOpen) return;
    const onClick = (e: MouseEvent) => {
      if (readerMenuRef.current && !readerMenuRef.current.contains(e.target as Node)) {
        setReaderMenuOpen(false);
      }
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setReaderMenuOpen(false);
    };
    document.addEventListener('mousedown', onClick);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('mousedown', onClick);
      document.removeEventListener('keydown', onKey);
    };
  }, [readerMenuOpen]);

  // Auto-close the menu when the reader closes
  useEffect(() => {
    if (!readerState.atomId) setReaderMenuOpen(false);
  }, [readerState.atomId]);

  // Debounced server-side search when searchQuery changes
  const searchTimerRef = useRef<ReturnType<typeof setTimeout>>();
  useEffect(() => {
    if (searchTimerRef.current) clearTimeout(searchTimerRef.current);

    const query = searchQuery.trim();
    if (!query) {
      // Clear search results when query is empty
      if (semanticSearchResults !== null) {
        clearSemanticSearch();
      }
      return;
    }

    // Debounce 300ms before triggering API search
    searchTimerRef.current = setTimeout(() => {
      search(query);
    }, 300);

    return () => {
      if (searchTimerRef.current) clearTimeout(searchTimerRef.current);
    };
  }, [searchQuery]);

  // Determine what to display
  const displayAtoms = useMemo(() => {
    // If semantic search is active, use those results
    if (semanticSearchResults !== null) {
      return semanticSearchResults;
    }
    return atoms;
  }, [atoms, semanticSearchResults]);

  // Check if we're showing semantic search results
  const isSemanticSearch = semanticSearchResults !== null;

  // Build lookup map for matching chunk content (avoids .find() per atom)
  const matchingChunkMap = useMemo(() => {
    if (!isSemanticSearch) return null;
    const map = new Map<string, string>();
    for (const r of semanticSearchResults) {
      if (r.matching_chunk_content) {
        map.set(r.id, r.matching_chunk_content);
      }
    }
    return map;
  }, [isSemanticSearch, semanticSearchResults]);

  const getMatchingChunkContent = useCallback((atomId: string): string | undefined => {
    return matchingChunkMap?.get(atomId);
  }, [matchingChunkMap]);

  const handleAtomClick = useCallback((atomId: string) => {
    // Pass highlight text based on search mode:
    // - Keyword: highlight the search query terms
    // - Semantic: highlight the matching chunk content
    // - Hybrid: highlight the search query (prioritize keywords over chunk)
    const isSearch = useAtomsStore.getState().semanticSearchResults !== null;
    if (!isSearch) {
      openReader(atomId);
      return;
    }
    const mode = useAtomsStore.getState().searchMode;
    const query = useAtomsStore.getState().semanticSearchQuery;
    let highlightText: string | undefined;
    if (mode === 'keyword' || mode === 'hybrid') {
      highlightText = query;
    } else {
      highlightText = matchingChunkMap?.get(atomId);
    }
    openReader(atomId, highlightText);
  }, [openReader, matchingChunkMap]);

  const createAtom = useAtomsStore(s => s.createAtom);
  const openReaderEditing = useUIStore(s => s.openReaderEditing);

  const handleNewAtom = useCallback(async () => {
    try {
      const newAtom = await createAtom('');
      openReaderEditing(newAtom.id);
    } catch (error) {
      console.error('Failed to create atom:', error);
    }
  }, [createAtom, openReaderEditing]);

  const handleRetryEmbedding = useCallback(async (atomId: string) => {
    try {
      await retryEmbedding(atomId);
    } catch (error) {
      console.error('Failed to retry embedding:', error);
    }
  }, [retryEmbedding]);

  const handleRetryTagging = useCallback(async (atomId: string) => {
    try {
      await retryTagging(atomId);
    } catch (error) {
      console.error('Failed to retry tagging:', error);
    }
  }, [retryTagging]);

  const handleOpenChat = useCallback(() => {
    toggleChatSidebar();
  }, [toggleChatSidebar]);

  const handleChatResizeStart = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    const startX = e.clientX;
    const startWidth = useUIStore.getState().chatSidebarWidth;
    setIsResizingChat(true);

    const onMouseMove = (e: MouseEvent) => {
      const delta = startX - e.clientX;
      setChatSidebarWidth(startWidth + delta);
    };

    const onMouseUp = () => {
      document.removeEventListener('mousemove', onMouseMove);
      document.removeEventListener('mouseup', onMouseUp);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
      setIsResizingChat(false);
    };

    document.addEventListener('mousemove', onMouseMove);
    document.addEventListener('mouseup', onMouseUp);
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
  }, [setChatSidebarWidth]);

  const handleOpenSearch = useCallback(() => {
    openCommandPalette('/');
  }, [openCommandPalette]);

  const handleLoadMore = useCallback(() => {
    if (!isSemanticSearch && hasMore) {
      fetchNextPage();
    }
  }, [isSemanticSearch, hasMore, fetchNextPage]);

  // Display count: totalCount from server when not searching, results length when searching
  const displayCount = isSemanticSearch ? displayAtoms.length : totalCount;

  return (
    <>
    <main className="relative flex-1 flex flex-col h-full bg-[var(--color-bg-main)] overflow-hidden">
      {/* Titlebar row */}
      <div className={`h-[52px] flex items-center gap-3 px-4 flex-shrink-0 ${!leftPanelOpen && isTauri() ? 'pl-[78px]' : ''}`}>
        {/* Left sidebar toggle — always visible */}
        <button
          onClick={toggleLeftPanel}
          className={`p-1.5 rounded-md transition-colors ${
            leftPanelOpen
              ? 'text-[var(--color-text-primary)] hover:bg-[var(--color-bg-hover)]'
              : 'text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] hover:bg-[var(--color-bg-hover)]'
          }`}
          title={leftPanelOpen ? "Hide sidebar" : "Show sidebar"}
        >
          <PanelLeft className="w-4 h-4" strokeWidth={2} />
        </button>

        {readerState.atomId || wikiReaderState.tagId || (localGraph.isOpen && localGraph.centerAtomId) ? (
          /* Reader/Graph/Wiki titlebar */
          <>
            <div className="flex items-center gap-1">
              <button
                onClick={overlayDismiss}
                className="p-1.5 rounded-md text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] hover:bg-[var(--color-bg-hover)] transition-colors"
                title="Close"
              >
                <X className="w-4 h-4" strokeWidth={2} />
              </button>
              {/* In edit mode: undo/redo. In view mode: back/forward */}
              {readerState.atomId && readerState.editing ? (
                <>
                  <button
                    onClick={() => readerEditorActions.current?.undo()}
                    className="p-1.5 rounded-md text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] hover:bg-[var(--color-bg-hover)] transition-colors"
                    title="Undo (Cmd+Z)"
                  >
                    <Undo2 className="w-4 h-4" strokeWidth={2} />
                  </button>
                  <button
                    onClick={() => readerEditorActions.current?.redo()}
                    className="p-1.5 rounded-md text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] hover:bg-[var(--color-bg-hover)] transition-colors"
                    title="Redo (Cmd+Shift+Z)"
                  >
                    <Redo2 className="w-4 h-4" strokeWidth={2} />
                  </button>
                </>
              ) : (
                <>
                  <button
                    onClick={overlayBack}
                    disabled={overlayNav.index <= 0}
                    className={`p-1.5 rounded-md transition-colors ${overlayNav.index > 0 ? 'text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] hover:bg-[var(--color-bg-hover)]' : 'text-[var(--color-text-tertiary)] cursor-default'}`}
                    title="Back"
                  >
                    <ChevronLeft className="w-4 h-4" strokeWidth={2} />
                  </button>
                  <button
                    onClick={overlayForward}
                    disabled={overlayNav.index >= overlayNav.stack.length - 1}
                    className={`p-1.5 rounded-md transition-colors ${overlayNav.index < overlayNav.stack.length - 1 ? 'text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] hover:bg-[var(--color-bg-hover)]' : 'text-[var(--color-text-tertiary)] cursor-default'}`}
                    title="Forward"
                  >
                    <ChevronRight className="w-4 h-4" strokeWidth={2} />
                  </button>
                </>
              )}
              {/* Save status indicator */}
              {readerState.editing && readerState.saveStatus !== 'idle' && (
                <span className={`text-xs ml-1 ${
                  readerState.saveStatus === 'saving' ? 'text-[var(--color-text-tertiary)]' :
                  readerState.saveStatus === 'saved' ? 'text-green-500' :
                  'text-red-500'
                }`}>
                  {readerState.saveStatus === 'saving' ? 'Saving...' :
                   readerState.saveStatus === 'saved' ? 'Saved' : 'Save failed'}
                </span>
              )}
            </div>

            <div data-tauri-drag-region className="flex-1 h-full drag-region" />

            {/* Action buttons for atom reader — inline on desktop, overflow menu on mobile */}
            {readerState.atomId && !isMobile && (
              <div className="flex items-center gap-1">
                {/* Theme toggle */}
                <button
                  onClick={toggleReaderTheme}
                  className="p-1.5 rounded-md text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] hover:bg-[var(--color-bg-hover)] transition-colors"
                  title={readerTheme === 'dark' ? 'Switch to light mode' : 'Switch to dark mode'}
                >
                  {readerTheme === 'dark' ? (
                    <Sun className="w-4 h-4" strokeWidth={2} />
                  ) : (
                    <Moon className="w-4 h-4" strokeWidth={2} />
                  )}
                </button>
                {/* Edit / Done toggle */}
                <button
                  onClick={() => readerState.editing
                    ? readerEditorActions.current?.stopEditing()
                    : readerEditorActions.current?.startEditing(0)
                  }
                  className={`p-1.5 rounded-md transition-colors ${
                    readerState.editing
                      ? 'text-[var(--color-accent)] hover:bg-[var(--color-accent)]/20'
                      : 'text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] hover:bg-[var(--color-bg-hover)]'
                  }`}
                  title={readerState.editing ? 'Done (Esc)' : 'Edit'}
                >
                  {readerState.editing ? (
                    <Check className="w-4 h-4" strokeWidth={2} />
                  ) : (
                    <Pencil className="w-4 h-4" strokeWidth={2} />
                  )}
                </button>
                {/* Delete */}
                <button
                  onClick={() => setShowDeleteModal(true)}
                  className="p-1.5 rounded-md text-[var(--color-text-secondary)] hover:text-red-400 hover:bg-[var(--color-bg-hover)] transition-colors"
                  title="Delete"
                >
                  <Trash2 className="w-4 h-4" strokeWidth={2} />
                </button>
              </div>
            )}

            {/* Mobile reader overflow menu: edit is the primary inline action,
                theme + delete hide behind a ⋯ button. */}
            {readerState.atomId && isMobile && (
              <div className="flex items-center gap-1">
                {/* Edit / Done toggle (kept inline — primary action) */}
                <button
                  onClick={() => readerState.editing
                    ? readerEditorActions.current?.stopEditing()
                    : readerEditorActions.current?.startEditing(0)
                  }
                  className={`p-1.5 rounded-md transition-colors ${
                    readerState.editing
                      ? 'text-[var(--color-accent)] hover:bg-[var(--color-accent)]/20'
                      : 'text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] hover:bg-[var(--color-bg-hover)]'
                  }`}
                  title={readerState.editing ? 'Done (Esc)' : 'Edit'}
                >
                  {readerState.editing ? (
                    <Check className="w-4 h-4" strokeWidth={2} />
                  ) : (
                    <Pencil className="w-4 h-4" strokeWidth={2} />
                  )}
                </button>

                {/* Overflow menu */}
                <div className="relative" ref={readerMenuRef}>
                  <button
                    onClick={() => setReaderMenuOpen(v => !v)}
                    className="p-1.5 rounded-md text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] hover:bg-[var(--color-bg-hover)] transition-colors"
                    title="More actions"
                    aria-haspopup="menu"
                    aria-expanded={readerMenuOpen}
                  >
                    <MoreHorizontal className="w-4 h-4" strokeWidth={2} />
                  </button>
                  {readerMenuOpen && (
                    <div
                      role="menu"
                      className="absolute top-full right-0 mt-1 w-48 bg-[var(--color-bg-card)] border border-[var(--color-border)] rounded-md shadow-lg z-50 overflow-hidden"
                    >
                      <button
                        role="menuitem"
                        onClick={() => { toggleReaderTheme(); setReaderMenuOpen(false); }}
                        className="w-full flex items-center gap-2 px-3 py-2 text-sm text-left text-[var(--color-text-primary)] hover:bg-[var(--color-bg-hover)] transition-colors"
                      >
                        {readerTheme === 'dark' ? (
                          <Sun className="w-4 h-4" strokeWidth={2} />
                        ) : (
                          <Moon className="w-4 h-4" strokeWidth={2} />
                        )}
                        {readerTheme === 'dark' ? 'Light mode' : 'Dark mode'}
                      </button>
                      <button
                        role="menuitem"
                        onClick={() => { setShowDeleteModal(true); setReaderMenuOpen(false); }}
                        className="w-full flex items-center gap-2 px-3 py-2 text-sm text-left text-red-400 hover:bg-[var(--color-bg-hover)] transition-colors"
                      >
                        <Trash2 className="w-4 h-4" strokeWidth={2} />
                        Delete
                      </button>
                    </div>
                  )}
                </div>
              </div>
            )}

            {/* Chat sidebar toggle */}
            <button
              onClick={handleOpenChat}
              className={`p-1.5 rounded-md transition-colors ${
                chatSidebarOpen
                  ? 'text-[var(--color-text-primary)] hover:bg-[var(--color-bg-hover)]'
                  : 'text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] hover:bg-[var(--color-bg-hover)]'
              }`}
              title={chatSidebarOpen ? "Hide chat" : "Show chat"}
            >
              <MessageCircle className="w-4 h-4" strokeWidth={2} />
            </button>
          </>
        ) : (
          /* Normal browsing titlebar */
          <>
            {/* View Mode Toggle — desktop only; mobile accesses view mode via the filter sheet */}
            {!isMobile && (
              <>
                <div className="flex items-center bg-[var(--color-bg-card)] rounded-md border border-[var(--color-border)] shrink-0">
                  <button
                    onClick={() => setViewMode('dashboard')}
                    className={`p-1.5 rounded-l-md transition-colors ${
                      viewMode === 'dashboard'
                        ? 'bg-[var(--color-accent)] text-white'
                        : 'text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)]'
                    }`}
                    title="Dashboard"
                  >
                    <LayoutDashboard className="w-4 h-4" strokeWidth={2} />
                  </button>
                  <button
                    onClick={() => setViewMode('atoms')}
                    className={`p-1.5 transition-colors ${
                      viewMode === 'atoms'
                        ? 'bg-[var(--color-accent)] text-white'
                        : 'text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)]'
                    }`}
                    title="Atoms"
                  >
                    <Library className="w-4 h-4" strokeWidth={2} />
                  </button>
                  <button
                    onClick={() => setViewMode('canvas')}
                    className={`p-1.5 transition-colors ${
                      viewMode === 'canvas'
                        ? 'bg-[var(--color-accent)] text-white'
                        : 'text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)]'
                    }`}
                    title="Canvas view"
                  >
                    <Network className="w-4 h-4" strokeWidth={2} />
                  </button>
                  <button
                    onClick={() => setViewMode('wiki')}
                    className={`p-1.5 rounded-r-md transition-colors ${
                      viewMode === 'wiki'
                        ? 'bg-[var(--color-accent)] text-white'
                        : 'text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)]'
                    }`}
                    title="Wiki view"
                  >
                    <BookOpen className="w-4 h-4" strokeWidth={2} />
                  </button>
                </div>

                {/* Atoms layout sub-toggle — only visible when on the atoms view */}
                {viewMode === 'atoms' && (
                  <div className="flex items-center bg-[var(--color-bg-card)] rounded-md border border-[var(--color-border)] shrink-0">
                    <button
                      onClick={() => setAtomsLayout('grid')}
                      className={`p-1.5 rounded-l-md transition-colors ${
                        atomsLayout === 'grid'
                          ? 'text-[var(--color-text-primary)] bg-[var(--color-bg-hover)]'
                          : 'text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)]'
                      }`}
                      title="Grid layout"
                    >
                      <LayoutGrid className="w-4 h-4" strokeWidth={2} />
                    </button>
                    <button
                      onClick={() => setAtomsLayout('list')}
                      className={`p-1.5 rounded-r-md transition-colors ${
                        atomsLayout === 'list'
                          ? 'text-[var(--color-text-primary)] bg-[var(--color-bg-hover)]'
                          : 'text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)]'
                      }`}
                      title="List layout"
                    >
                      <ListIcon className="w-4 h-4" strokeWidth={2} />
                    </button>
                  </div>
                )}
              </>
            )}

            {/* Search button */}
            <button
              onClick={handleOpenSearch}
              className="p-1.5 rounded-md text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] hover:bg-[var(--color-bg-hover)] transition-colors"
              title="Search atoms"
            >
              <Search className="w-4 h-4" strokeWidth={2} />
            </button>

            <div data-tauri-drag-region className="flex-1 h-full drag-region" />

            {/* Filter toggle + atom count — right-aligned. On mobile, always show the
                filter button (it opens the filter sheet which hosts view mode too). */}
            {(isMobile || viewMode === 'atoms') && (
              <div className="flex items-center gap-2 shrink-0">
                <button
                  onClick={() => setFilterBarOpen(!filterBarOpen)}
                  className={`relative p-1.5 rounded-md transition-colors ${
                    filterBarOpen || hasActiveFilter
                      ? 'text-[var(--color-accent-light)] hover:text-[var(--color-accent)]'
                      : 'text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] hover:bg-[var(--color-bg-hover)]'
                  }`}
                  title="Filter & sort"
                >
                  <Filter className="w-4 h-4" strokeWidth={2} />
                  {hasActiveFilter && !filterBarOpen && (
                    <span className="absolute top-0.5 right-0.5 w-1.5 h-1.5 bg-[var(--color-accent)] rounded-full" />
                  )}
                </button>
                {!isMobile && (
                  <span className="text-sm text-[var(--color-text-secondary)]">
                    {displayCount} atom{displayCount !== 1 ? 's' : ''}
                  </span>
                )}
              </div>
            )}

            {/* Chat sidebar toggle — right-aligned */}
            <button
              onClick={handleOpenChat}
              className={`p-1.5 rounded-md transition-colors ${
                chatSidebarOpen
                  ? 'text-[var(--color-text-primary)] hover:bg-[var(--color-bg-hover)]'
                  : 'text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] hover:bg-[var(--color-bg-hover)]'
              }`}
              title={chatSidebarOpen ? "Hide chat" : "Show chat"}
            >
              <MessageCircle className="w-4 h-4" strokeWidth={2} />
            </button>
          </>
        )}
      </div>

      {/* Search results header - only show in atoms view */}
      {isSemanticSearch && viewMode === 'atoms' && (
        <div className="px-4 py-2 text-sm text-[var(--color-text-secondary)] border-b border-[var(--color-border)]">
          {semanticSearchResults.length > 0 ? (
            <span>
              {semanticSearchResults.length} results for "{semanticSearchQuery}"
            </span>
          ) : (
            <span>No atoms match your search</span>
          )}
        </div>
      )}

      {/* Filter bar — desktop inline strip, atoms view only */}
      {!isMobile && !isSemanticSearch && viewMode === 'atoms' && filterBarOpen && <FilterBar />}

      {/* Filter sheet — mobile bottom sheet hosts view mode + filter + sort */}
      {isMobile && (
        <FilterSheet
          isOpen={filterBarOpen}
          onClose={() => setFilterBarOpen(false)}
          displayCount={displayCount}
        />
      )}

      {/* Content */}
      <div className="flex-1 overflow-hidden relative">
        {localGraph.isOpen && localGraph.centerAtomId ? (
          <LocalGraphView />
        ) : readerState.atomId ? (
          <AtomReader atomId={readerState.atomId} highlightText={readerState.highlightText} initialEditing={readerState.editing} />
        ) : wikiReaderState.tagId && wikiReaderState.tagName ? (
          <WikiReader tagId={wikiReaderState.tagId} tagName={wikiReaderState.tagName} />
        ) : viewMode === 'dashboard' ? (
          <DashboardView />
        ) : viewMode === 'wiki' ? (
          <WikiFullView />
        ) : viewMode === 'canvas' ? (
          <SigmaCanvas />
        ) : atomsLayout === 'grid' ? (
          <AtomGrid
            atoms={displayAtoms}
            onAtomClick={handleAtomClick}
            getMatchingChunkContent={isSemanticSearch ? getMatchingChunkContent : undefined}
            onRetryEmbedding={handleRetryEmbedding}
            onRetryTagging={handleRetryTagging}
            onLoadMore={handleLoadMore}
            isLoading={isLoadingInitial}
            isLoadingMore={isLoadingMore}
          />
        ) : (
          <AtomList
            atoms={displayAtoms}
            onAtomClick={handleAtomClick}
            getMatchingChunkContent={isSemanticSearch ? getMatchingChunkContent : undefined}
            onRetryEmbedding={handleRetryEmbedding}
            onRetryTagging={handleRetryTagging}
            onLoadMore={handleLoadMore}
            isLoading={isLoadingInitial}
            isLoadingMore={isLoadingMore}
          />
        )}
      </div>

      {/* FAB — on atoms + dashboard views, and only when no overlay is open */}
      {(viewMode === 'atoms' || viewMode === 'dashboard') && !readerState.atomId && !wikiReaderState.tagId && !localGraph.isOpen && <FAB onClick={handleNewAtom} title="Create new atom" />}

      {/* Embedding progress overlay */}
      <EmbeddingProgressBanner />

      {/* Delete confirmation modal for reader */}
      <Modal
        isOpen={showDeleteModal}
        onClose={() => setShowDeleteModal(false)}
        title="Delete Atom"
        confirmLabel={isDeleting ? 'Deleting...' : 'Delete'}
        confirmVariant="danger"
        onConfirm={async () => {
          if (!readerState.atomId) return;
          setIsDeleting(true);
          try {
            await deleteAtom(readerState.atomId);
            await fetchTags();
            overlayDismiss();
          } catch (error) {
            console.error('Failed to delete atom:', error);
          } finally {
            setIsDeleting(false);
            setShowDeleteModal(false);
          }
        }}
      >
        <p>Are you sure you want to delete this atom? This action cannot be undone.</p>
      </Modal>
    </main>

    {/* Chat sidebar backdrop — mobile only */}
    <div
      className={`fixed inset-0 bg-black/40 z-30 md:hidden transition-opacity duration-200 ${
        chatSidebarOpen ? 'opacity-100' : 'opacity-0 pointer-events-none'
      }`}
      onClick={() => chatSidebarOpen && toggleChatSidebar()}
    />

    {/* Chat sidebar — available in all views.
        Desktop: flex sibling that animates width.
        Mobile: fixed overlay that slides in from the right. */}
    <div
      className={`
        relative flex-shrink-0 border-l border-[var(--color-border)] bg-[var(--color-bg-panel)] overflow-hidden
        max-md:fixed max-md:top-0 max-md:right-0 max-md:h-full max-md:w-full max-md:z-40 max-md:shadow-2xl
        max-md:pt-[env(safe-area-inset-top)] max-md:pb-[env(safe-area-inset-bottom)] max-md:pr-[env(safe-area-inset-right)]
        md:w-[var(--chat-w)]
        ${isResizingChat ? '' : 'transition-[width,transform] duration-300 ease-in-out'}
        ${chatSidebarOpen ? 'max-md:translate-x-0' : 'max-md:translate-x-full'}
        ${chatSidebarOpen ? '' : 'md:!w-0 md:border-l-0'}
      `}
      style={{ '--chat-w': `${chatSidebarWidth}px` } as React.CSSProperties}
    >
      {/* Resize handle — desktop only */}
      <div
        className="hidden md:block absolute left-0 top-0 h-full w-1.5 cursor-col-resize z-10 hover:bg-[var(--color-accent)]/20 active:bg-[var(--color-accent)]/30"
        onMouseDown={handleChatResizeStart}
      />
      <div className="w-full md:min-w-[var(--chat-w)] h-full">
        <ChatViewer />
      </div>
    </div>
    </>
  );
}
