import { useMemo, useCallback, useEffect, useRef, useState } from 'react';
import { useShallow } from 'zustand/react/shallow';
import { AtomGrid } from '../atoms/AtomGrid';
import { AtomList } from '../atoms/AtomList';
import { AtomReader } from '../atoms/AtomReader';
import { FilterBar } from '../atoms/FilterBar';
import { SigmaCanvas } from '../canvas/SigmaCanvas';
import { LocalGraphView } from '../canvas/LocalGraphView';
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

  const { viewMode, searchQuery } = useUIStore(
    useShallow(s => ({
      viewMode: s.viewMode,
      searchQuery: s.searchQuery,
    }))
  );
  const leftPanelOpen = useUIStore(s => s.leftPanelOpen);
  const toggleLeftPanel = useUIStore(s => s.toggleLeftPanel);
  const setViewMode = useUIStore(s => s.setViewMode);
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
  const toggleChatSidebar = useUIStore(s => s.toggleChatSidebar);

  const [filterBarOpen, setFilterBarOpen] = useState(false);
  const [showDeleteModal, setShowDeleteModal] = useState(false);
  const [isDeleting, setIsDeleting] = useState(false);
  const hasActiveFilter = sourceFilter !== 'all' || !!sourceValue || sortBy !== 'updated' || sortOrder !== 'desc';

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
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <rect x="3" y="3" width="18" height="18" rx="2" />
            <line x1="9" y1="3" x2="9" y2="21" />
          </svg>
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
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                </svg>
              </button>
              {/* In edit mode: undo/redo. In view mode: back/forward */}
              {readerState.atomId && readerState.editing ? (
                <>
                  <button
                    onClick={() => readerEditorActions.current?.undo()}
                    className="p-1.5 rounded-md text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] hover:bg-[var(--color-bg-hover)] transition-colors"
                    title="Undo (Cmd+Z)"
                  >
                    <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 10h10a5 5 0 015 5v2M3 10l4-4M3 10l4 4" />
                    </svg>
                  </button>
                  <button
                    onClick={() => readerEditorActions.current?.redo()}
                    className="p-1.5 rounded-md text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] hover:bg-[var(--color-bg-hover)] transition-colors"
                    title="Redo (Cmd+Shift+Z)"
                  >
                    <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 10H11a5 5 0 00-5 5v2M21 10l-4-4M21 10l-4 4" />
                    </svg>
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
                    <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 19l-7-7 7-7" />
                    </svg>
                  </button>
                  <button
                    onClick={overlayForward}
                    disabled={overlayNav.index >= overlayNav.stack.length - 1}
                    className={`p-1.5 rounded-md transition-colors ${overlayNav.index < overlayNav.stack.length - 1 ? 'text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] hover:bg-[var(--color-bg-hover)]' : 'text-[var(--color-text-tertiary)] cursor-default'}`}
                    title="Forward"
                  >
                    <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
                    </svg>
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

            {/* Action buttons for atom reader */}
            {readerState.atomId && (
              <div className="flex items-center gap-1">
                {/* Theme toggle */}
                <button
                  onClick={toggleReaderTheme}
                  className="p-1.5 rounded-md text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] hover:bg-[var(--color-bg-hover)] transition-colors"
                  title={readerTheme === 'dark' ? 'Switch to light mode' : 'Switch to dark mode'}
                >
                  {readerTheme === 'dark' ? (
                    <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 3v1m0 16v1m9-9h-1M4 12H3m15.364 6.364l-.707-.707M6.343 6.343l-.707-.707m12.728 0l-.707.707M6.343 17.657l-.707.707M16 12a4 4 0 11-8 0 4 4 0 018 0z" />
                    </svg>
                  ) : (
                    <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M20.354 15.354A9 9 0 018.646 3.646 9.003 9.003 0 0012 21a9.003 9.003 0 008.354-5.646z" />
                    </svg>
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
                    <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
                    </svg>
                  ) : (
                    <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
                    </svg>
                  )}
                </button>
                {/* Delete */}
                <button
                  onClick={() => setShowDeleteModal(true)}
                  className="p-1.5 rounded-md text-[var(--color-text-secondary)] hover:text-red-400 hover:bg-[var(--color-bg-hover)] transition-colors"
                  title="Delete"
                >
                  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                  </svg>
                </button>
              </div>
            )}

            {/* Chat sidebar toggle */}
            <button
              onClick={handleOpenChat}
              className={`hidden md:block p-1.5 rounded-md transition-colors ${
                chatSidebarOpen
                  ? 'text-[var(--color-text-primary)] hover:bg-[var(--color-bg-hover)]'
                  : 'text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] hover:bg-[var(--color-bg-hover)]'
              }`}
              title={chatSidebarOpen ? "Hide chat" : "Show chat"}
            >
              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z" />
              </svg>
            </button>
          </>
        ) : (
          /* Normal browsing titlebar */
          <>
            {/* View Mode Toggle */}
            <div className="flex items-center bg-[var(--color-bg-card)] rounded-md border border-[var(--color-border)] shrink-0">
              <button
                onClick={() => setViewMode('grid')}
                className={`p-1.5 rounded-l-md transition-colors ${
                  viewMode === 'grid'
                    ? 'bg-[var(--color-accent)] text-white'
                    : 'text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)]'
                }`}
                title="Grid view"
              >
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2V6zM14 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2V6zM4 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2v-2zM14 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2v-2z" />
                </svg>
              </button>
              <button
                onClick={() => setViewMode('list')}
                className={`p-1.5 transition-colors ${
                  viewMode === 'list'
                    ? 'bg-[var(--color-accent)] text-white'
                    : 'text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)]'
                }`}
                title="List view"
              >
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 6h16M4 12h16M4 18h16" />
                </svg>
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
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" strokeWidth={2}>
                  <circle cx="6" cy="6" r="2" />
                  <circle cx="18" cy="6" r="2" />
                  <circle cx="6" cy="18" r="2" />
                  <circle cx="18" cy="18" r="2" />
                  <circle cx="12" cy="12" r="2" />
                  <path strokeLinecap="round" d="M8 7l2.5 3.5M16 7l-2.5 3.5M8 17l2.5-3.5M16 17l-2.5-3.5" />
                </svg>
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
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 6.253v13m0-13C10.832 5.477 9.246 5 7.5 5S4.168 5.477 3 6.253v13C4.168 18.477 5.754 18 7.5 18s3.332.477 4.5 1.253m0-13C13.168 5.477 14.754 5 16.5 5c1.747 0 3.332.477 4.5 1.253v13C19.832 18.477 18.247 18 16.5 18c-1.746 0-3.332.477-4.5 1.253" />
                </svg>
              </button>
            </div>

            {/* Search button */}
            <button
              onClick={handleOpenSearch}
              className="p-1.5 rounded-md text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] hover:bg-[var(--color-bg-hover)] transition-colors"
              title="Search atoms"
            >
              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
              </svg>
            </button>

            <div data-tauri-drag-region className="flex-1 h-full drag-region" />

            {/* Filter toggle + atom count — right-aligned, hide for canvas/wiki */}
            {viewMode !== 'canvas' && viewMode !== 'wiki' && (
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
                  <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 4a1 1 0 011-1h16a1 1 0 011 1v2.586a1 1 0 01-.293.707l-6.414 6.414a1 1 0 00-.293.707V17l-4 4v-6.586a1 1 0 00-.293-.707L3.293 7.293A1 1 0 013 6.586V4z" />
                  </svg>
                  {hasActiveFilter && !filterBarOpen && (
                    <span className="absolute top-0.5 right-0.5 w-1.5 h-1.5 bg-[var(--color-accent)] rounded-full" />
                  )}
                </button>
                <span className="text-sm text-[var(--color-text-secondary)]">
                  {displayCount} atom{displayCount !== 1 ? 's' : ''}
                </span>
              </div>
            )}

            {/* Chat sidebar toggle — right-aligned */}
            <button
              onClick={handleOpenChat}
              className={`hidden md:block p-1.5 rounded-md transition-colors ${
                chatSidebarOpen
                  ? 'text-[var(--color-text-primary)] hover:bg-[var(--color-bg-hover)]'
                  : 'text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] hover:bg-[var(--color-bg-hover)]'
              }`}
              title={chatSidebarOpen ? "Hide chat" : "Show chat"}
            >
              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z" />
              </svg>
            </button>
          </>
        )}
      </div>

      {/* Search results header - only show for grid/list views */}
      {isSemanticSearch && viewMode !== 'canvas' && viewMode !== 'wiki' && (
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

      {/* Filter bar - visible for grid/list views when toggled open */}
      {!isSemanticSearch && viewMode !== 'canvas' && viewMode !== 'wiki' && filterBarOpen && <FilterBar />}

      {/* Content */}
      <div className="flex-1 overflow-hidden relative">
        {localGraph.isOpen && localGraph.centerAtomId ? (
          <LocalGraphView />
        ) : readerState.atomId ? (
          <AtomReader atomId={readerState.atomId} highlightText={readerState.highlightText} initialEditing={readerState.editing} />
        ) : wikiReaderState.tagId && wikiReaderState.tagName ? (
          <WikiReader tagId={wikiReaderState.tagId} tagName={wikiReaderState.tagName} />
        ) : viewMode === 'wiki' ? (
          <WikiFullView />
        ) : viewMode === 'canvas' ? (
          <SigmaCanvas />
        ) : viewMode === 'grid' ? (
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

      {/* FAB — hide in wiki, canvas, reader, and graph */}
      {viewMode !== 'wiki' && viewMode !== 'canvas' && !readerState.atomId && !wikiReaderState.tagId && !localGraph.isOpen && <FAB onClick={handleNewAtom} title="Create new atom" />}

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

    {/* Chat sidebar — available in all views */}
    <div
      className={`hidden md:block flex-shrink-0 border-l border-[var(--color-border)] overflow-hidden bg-[var(--color-bg-panel)] transition-[width] duration-300 ease-in-out ${
        chatSidebarOpen ? 'w-96' : 'w-0 border-l-0'
      }`}
    >
      <div className="w-96 h-full">
        <ChatViewer />
      </div>
    </div>
    </>
  );
}
