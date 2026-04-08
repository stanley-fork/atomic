import { useState, useEffect, useCallback, ReactNode, useRef, useMemo, memo } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { openExternalUrl } from '../../lib/platform';
import { Button } from '../ui/Button';
import { Modal } from '../ui/Modal';
import { SearchBar } from '../ui/SearchBar';
import { MarkdownImage } from '../ui/MarkdownImage';
import { TagChip } from '../tags/TagChip';
import { MiniGraphPreview } from '../canvas/MiniGraphPreview';
import { useAtomsStore, type AtomWithTags, type SimilarAtomResult } from '../../stores/atoms';
import { useTagsStore } from '../../stores/tags';
import { useUIStore } from '../../stores/ui';
import { useContentSearch } from '../../hooks';
import { formatDate } from '../../lib/date';
import { chunkMarkdown, findChunkIndexForOffset } from '../../lib/markdown';
import { getTransport } from '../../lib/transport';

// Progressive rendering configuration
const CHUNK_SIZE = 8000;
const INITIAL_CHUNKS = 1;
const CHUNKS_PER_BATCH = 2;
const CHUNK_DELAY = 32;

const remarkPluginsStable = [remarkGfm];

const MemoizedMarkdownChunk = memo(function MarkdownChunk({
  content,
  components,
}: {
  content: string;
  components: any;
}) {
  return (
    <ReactMarkdown remarkPlugins={remarkPluginsStable} components={components}>
      {content}
    </ReactMarkdown>
  );
});

interface AtomReaderProps {
  atomId: string;
  highlightText?: string | null;
}

export function AtomReader({ atomId, highlightText }: AtomReaderProps) {
  const deleteAtom = useAtomsStore(s => s.deleteAtom);
  const fetchTags = useTagsStore(s => s.fetchTags);
  const setSelectedTag = useUIStore(s => s.setSelectedTag);
  const overlayNavigate = useUIStore(s => s.overlayNavigate);
  const overlayBack = useUIStore(s => s.overlayBack);
  const overlayForward = useUIStore(s => s.overlayForward);
  const overlayDismiss = useUIStore(s => s.overlayDismiss);
  const overlayNav = useUIStore(s => s.overlayNav);
  const openDrawer = useUIStore(s => s.openDrawer);

  const [atom, setAtom] = useState<AtomWithTags | null>(null);
  const [isLoadingAtom, setIsLoadingAtom] = useState(true);
  const [showLoading, setShowLoading] = useState(false);


  // Watch the atoms store for updates to the currently viewed atom
  const storeAtom = useAtomsStore((s) =>
    s.atoms.find((a) => a.id === atomId)
  );

  // Fetch atom from database
  useEffect(() => {
    setIsLoadingAtom(true);
    setShowLoading(false);

    // Only show loading indicator if fetch takes longer than 200ms
    const loadingTimer = setTimeout(() => setShowLoading(true), 200);

    getTransport().invoke<AtomWithTags | null>('get_atom_by_id', { id: atomId })
      .then((fetchedAtom) => {
        clearTimeout(loadingTimer);
        setAtom(fetchedAtom);
        setIsLoadingAtom(false);
        lastFetchedAt.current = fetchedAtom?.updated_at ?? null;
      })
      .catch((error) => {
        clearTimeout(loadingTimer);
        console.error('Failed to fetch atom:', error);
        setAtom(null);
        setIsLoadingAtom(false);
        // atom loaded
      });

    return () => clearTimeout(loadingTimer);
  }, [atomId]);

  // Re-fetch when store summary changes (e.g., after tag extraction)
  const storeAtomUpdatedAt = storeAtom?.updated_at;
  const lastFetchedAt = useRef<string | null>(null);
  useEffect(() => {
    if (storeAtomUpdatedAt && !isLoadingAtom && storeAtomUpdatedAt !== lastFetchedAt.current) {
      lastFetchedAt.current = storeAtomUpdatedAt;
      getTransport().invoke<AtomWithTags | null>('get_atom_by_id', { id: atomId })
        .then((fetchedAtom) => {
          if (fetchedAtom) setAtom(fetchedAtom);
        })
        .catch(console.error);
    }
  }, [atomId, storeAtomUpdatedAt, isLoadingAtom]);

  const canGoBack = overlayNav.index > 0;
  const canGoForward = overlayNav.index < overlayNav.stack.length - 1;

  // Backdrop always renders immediately for instant feedback
  return (
    <div
      className="h-full transition-[backdrop-filter] duration-200 ease-out"
      style={{ backdropFilter: 'blur(20px)', WebkitBackdropFilter: 'blur(20px)', backgroundColor: 'rgba(30, 30, 30, 0.6)' }}
    >
      {isLoadingAtom ? (
        showLoading ? (
          <div className="flex items-center justify-center h-full text-[var(--color-text-secondary)]">
            Loading...
          </div>
        ) : null
      ) : !atom ? (
        <div className="flex items-center justify-center h-full text-[var(--color-text-secondary)]">
          Atom not found
        </div>
      ) : (
        <AtomReaderContent
          atom={atom}
          highlightText={highlightText}
          onBack={overlayBack}
          onForward={overlayForward}
          onDismiss={overlayDismiss}
          canGoBack={canGoBack}
          canGoForward={canGoForward}
          onEdit={() => openDrawer('editor', atomId)}
          onDelete={async () => {
            await deleteAtom(atomId);
            await fetchTags();
            overlayDismiss();
          }}
          onTagClick={(tagId) => { setSelectedTag(tagId); overlayDismiss(); }}
          onRelatedAtomClick={(id) => overlayNavigate({ type: 'reader', atomId: id })}
          onViewGraph={() => overlayNavigate({ type: 'graph', atomId })}
        />
      )}
    </div>
  );
}

interface AtomReaderContentProps {
  atom: AtomWithTags;
  highlightText?: string | null;
  onBack: () => void;
  onForward: () => void;
  onDismiss: () => void;
  canGoBack: boolean;
  canGoForward: boolean;
  onEdit: () => void;
  onDelete: () => Promise<void>;
  onTagClick: (tagId: string) => void;
  onRelatedAtomClick: (atomId: string) => void;
  onViewGraph: () => void;
}

function AtomReaderContent({
  atom, highlightText, onBack, onForward, onDismiss, canGoBack, canGoForward,
  onEdit, onDelete, onTagClick, onRelatedAtomClick, onViewGraph,
}: AtomReaderContentProps) {
  const readerTheme = useUIStore(s => s.readerTheme);
  const toggleReaderTheme = useUIStore(s => s.toggleReaderTheme);
  const scrollContainerRef = useRef<HTMLDivElement | null>(null);
  const articleRef = useRef<HTMLElement | null>(null);
  const [showDeleteModal, setShowDeleteModal] = useState(false);
  const [isDeleting, setIsDeleting] = useState(false);

  // Fade-in on mount
  const [revealed, setRevealed] = useState(false);
  useEffect(() => {
    const frame = requestAnimationFrame(() => setRevealed(true));
    return () => cancelAnimationFrame(frame);
  }, []);

  // Initial highlight state
  const [initialHighlight, setInitialHighlight] = useState<string | null>(null);
  const [targetChunkIndex, setTargetChunkIndex] = useState<number | null>(null);

  // Progressive rendering
  const chunks = useMemo(() => chunkMarkdown(atom.content, CHUNK_SIZE), [atom.content]);
  const [renderedChunkCount, setRenderedChunkCount] = useState(INITIAL_CHUNKS);
  const isFullyRendered = renderedChunkCount >= chunks.length;

  useEffect(() => {
    if (isFullyRendered) return;
    if ('requestIdleCallback' in window) {
      const id = requestIdleCallback(() => {
        setRenderedChunkCount(prev => Math.min(prev + CHUNKS_PER_BATCH, chunks.length));
      }, { timeout: 100 });
      return () => cancelIdleCallback(id);
    } else {
      const id = setTimeout(() => {
        setRenderedChunkCount(prev => Math.min(prev + CHUNKS_PER_BATCH, chunks.length));
      }, CHUNK_DELAY);
      return () => clearTimeout(id);
    }
  }, [renderedChunkCount, chunks.length, isFullyRendered]);

  useEffect(() => { setRenderedChunkCount(INITIAL_CHUNKS); }, [atom.id]);

  // Calculate target chunk from highlightText
  useEffect(() => {
    if (highlightText && atom.content) {
      const offset = atom.content.indexOf(highlightText);
      if (offset !== -1) {
        const chunkIndex = findChunkIndexForOffset(atom.content, offset, CHUNK_SIZE);
        setTargetChunkIndex(chunkIndex);
        setInitialHighlight(highlightText.slice(0, 50).trim());
      }
    } else {
      setInitialHighlight(null);
      setTargetChunkIndex(null);
    }
  }, [highlightText, atom.content]);

  // Scroll to highlight
  useEffect(() => {
    if (targetChunkIndex === null || initialHighlight === null) return;
    if (targetChunkIndex >= renderedChunkCount) {
      setRenderedChunkCount(targetChunkIndex + 1);
      return;
    }
    const scrollTimeout = setTimeout(() => {
      const mark = document.querySelector('[data-initial-highlight]');
      if (mark && scrollContainerRef.current) {
        mark.scrollIntoView({ behavior: 'smooth', block: 'center' });
      }
    }, 100);
    const clearTimer = setTimeout(() => {
      setInitialHighlight(null);
      setTargetChunkIndex(null);
    }, 5000);
    return () => { clearTimeout(scrollTimeout); clearTimeout(clearTimer); };
  }, [targetChunkIndex, renderedChunkCount, initialHighlight]);

  // Content search
  const {
    isOpen: isSearchOpen, query: searchQuery, searchedQuery,
    currentIndex, totalMatches,
    setQuery: setSearchQuery, openSearch, closeSearch, goToNext, goToPrevious, processChildren,
  } = useContentSearch(atom.content);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === 'f') {
        e.preventDefault();
        openSearch();
      }
      if (e.key === 'Escape') {
        if (showDeleteModal || isSearchOpen) return; // let inner layer handle it
        onDismiss();
      }
    };
    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [openSearch, showDeleteModal, isSearchOpen, onDismiss]);

  // Highlight helpers
  const highlightInitialText = useCallback((text: string): ReactNode => {
    if (!initialHighlight || !text) return text;
    const idx = text.toLowerCase().indexOf(initialHighlight.toLowerCase());
    if (idx === -1) return text;
    return (
      <>
        {text.slice(0, idx)}
        <mark data-initial-highlight="true" className="initial-highlight">
          {text.slice(idx, idx + initialHighlight.length)}
        </mark>
        {text.slice(idx + initialHighlight.length)}
      </>
    );
  }, [initialHighlight]);

  const processInitialHighlight = useCallback((children: ReactNode): ReactNode => {
    if (typeof children === 'string') return highlightInitialText(children);
    if (Array.isArray(children)) return children.map((child, i) => <span key={i}>{processInitialHighlight(child)}</span>);
    return children;
  }, [highlightInitialText]);

  const wrapWithHighlight = useCallback((children: ReactNode): ReactNode => {
    if (initialHighlight) return processInitialHighlight(children);
    if (isSearchOpen && searchQuery.trim()) return processChildren(children);
    return children;
  }, [isSearchOpen, searchQuery, processChildren, initialHighlight, processInitialHighlight]);

  const markdownComponents = useMemo(() => ({
    p: ({ children }: { children?: ReactNode }) => <p>{wrapWithHighlight(children)}</p>,
    li: ({ children }: { children?: ReactNode }) => <li>{wrapWithHighlight(children)}</li>,
    td: ({ children }: { children?: ReactNode }) => <td>{wrapWithHighlight(children)}</td>,
    th: ({ children }: { children?: ReactNode }) => <th>{wrapWithHighlight(children)}</th>,
    strong: ({ children }: { children?: ReactNode }) => <strong>{wrapWithHighlight(children)}</strong>,
    em: ({ children }: { children?: ReactNode }) => <em>{wrapWithHighlight(children)}</em>,
    del: ({ children }: { children?: ReactNode }) => <del>{wrapWithHighlight(children)}</del>,
    h1: ({ children }: { children?: ReactNode }) => <h1>{wrapWithHighlight(children)}</h1>,
    h2: ({ children }: { children?: ReactNode }) => <h2>{wrapWithHighlight(children)}</h2>,
    h3: ({ children }: { children?: ReactNode }) => <h3>{wrapWithHighlight(children)}</h3>,
    h4: ({ children }: { children?: ReactNode }) => <h4>{wrapWithHighlight(children)}</h4>,
    h5: ({ children }: { children?: ReactNode }) => <h5>{wrapWithHighlight(children)}</h5>,
    h6: ({ children }: { children?: ReactNode }) => <h6>{wrapWithHighlight(children)}</h6>,
    blockquote: ({ children }: { children?: ReactNode }) => <blockquote>{wrapWithHighlight(children)}</blockquote>,
    code: ({ className, children }: { className?: string; children?: ReactNode }) => {
      const isBlock = className?.startsWith('language-');
      if (isBlock) return <code className={className}>{wrapWithHighlight(children)}</code>;
      return <code>{wrapWithHighlight(children)}</code>;
    },
    pre: ({ children }: { children?: ReactNode }) => <pre>{children}</pre>,
    a: ({ href, children }: { href?: string; children?: ReactNode }) => {
      // If the link wraps an image, render the image unwrapped
      const childArray = Array.isArray(children) ? children : [children];
      if (childArray.some((c: any) => c?.type === MarkdownImage || c?.props?.src)) {
        return <>{children}</>;
      }
      return (
        <a href={href} onClick={(e) => { e.preventDefault(); if (href) openExternalUrl(href).catch(console.error); }} className="cursor-pointer">
          {wrapWithHighlight(children)}
        </a>
      );
    },
    img: ({ src, alt }: { src?: string; alt?: string }) => <MarkdownImage src={src} alt={alt} />,
  }), [wrapWithHighlight]);

  const handleDelete = async () => {
    setIsDeleting(true);
    try {
      await onDelete();
    } catch (error) {
      console.error('Failed to delete atom:', error);
    } finally {
      setIsDeleting(false);
      setShowDeleteModal(false);
    }
  };

  const proseClasses = `prose ${readerTheme === 'dark' ? 'prose-invert' : ''} max-w-none prose-headings:text-[var(--color-text-primary)] prose-p:text-[var(--color-text-primary)] prose-a:text-[var(--color-text-primary)] prose-a:underline prose-a:decoration-[var(--color-border-hover)] prose-a:hover:decoration-current prose-strong:text-[var(--color-text-primary)] prose-code:text-[var(--color-accent-light)] prose-code:bg-[var(--color-bg-card)] prose-code:px-1 prose-code:py-0.5 prose-code:rounded prose-pre:bg-[var(--color-bg-card)] prose-pre:border prose-pre:border-[var(--color-border)] prose-blockquote:border-l-[var(--color-accent)] prose-blockquote:text-[var(--color-text-secondary)] prose-li:text-[var(--color-text-primary)] prose-hr:border-[var(--color-border)]`;

  return (
    <div
      data-reader-theme={readerTheme}
      className={`h-full flex flex-col bg-[var(--color-bg-main)] transition-opacity duration-300 ease-out ${revealed ? 'opacity-100' : 'opacity-0'}`}
    >
      {/* Top bar */}
      <div className="flex items-center justify-between px-6 py-3 flex-shrink-0">
        <div className="flex items-center gap-1">
          {/* Back */}
          <button
            onClick={onBack}
            disabled={!canGoBack}
            className={`p-1.5 rounded-md transition-colors ${canGoBack ? 'text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] hover:bg-[var(--color-bg-hover)]' : 'text-[var(--color-text-tertiary)] cursor-default'}`}
            title="Back"
          >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 19l-7-7 7-7" />
            </svg>
          </button>
          {/* Forward */}
          <button
            onClick={onForward}
            disabled={!canGoForward}
            className={`p-1.5 rounded-md transition-colors ${canGoForward ? 'text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] hover:bg-[var(--color-bg-hover)]' : 'text-[var(--color-text-tertiary)] cursor-default'}`}
            title="Forward"
          >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
            </svg>
          </button>
        </div>
        <div className="flex items-center gap-3">
          {atom.embedding_status !== 'failed' && (
            <Button variant="ghost" size="sm" onClick={onViewGraph} title="View neighborhood graph">
              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <circle cx="12" cy="12" r="3" strokeWidth={2} />
                <circle cx="5" cy="8" r="2" strokeWidth={2} />
                <circle cx="19" cy="8" r="2" strokeWidth={2} />
                <circle cx="7" cy="18" r="2" strokeWidth={2} />
                <circle cx="17" cy="18" r="2" strokeWidth={2} />
                <path strokeLinecap="round" strokeWidth={2} d="M9.5 10.5L6.5 9M14.5 10.5L17.5 9M10 14L8 16.5M14 14L16 16.5" />
              </svg>
            </Button>
          )}
          <Button variant="ghost" size="sm" onClick={onEdit}>
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
            </svg>
          </Button>
          <Button variant="ghost" size="sm" onClick={() => setShowDeleteModal(true)}>
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
            </svg>
          </Button>
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
          <div className="w-px h-4 bg-[var(--color-border)] mx-1" />
          <button
            onClick={onDismiss}
            className="p-1.5 rounded-md text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] hover:bg-[var(--color-bg-hover)] transition-colors"
            title="Close"
          >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
      </div>

      {/* Scrollable content area */}
      <div ref={scrollContainerRef} className="flex-1 overflow-y-auto">
        {/* Search bar */}
        {isSearchOpen && (
          <div className="max-w-2xl mx-auto px-6">
            <SearchBar
              query={searchQuery}
              searchedQuery={searchedQuery}
              onQueryChange={setSearchQuery}
              currentIndex={currentIndex}
              totalMatches={totalMatches}
              onNext={goToNext}
              onPrevious={goToPrevious}
              onClose={closeSearch}
            />
          </div>
        )}

        {/* Content area */}
        <div className="max-w-6xl mx-auto px-6 py-6 lg:flex lg:gap-10">
          {/* Prose column */}
          <div className="flex-1 min-w-0">
            <article ref={articleRef} className={`max-w-3xl ${proseClasses}`}>
              {chunks.slice(0, renderedChunkCount).map((chunk, index) => (
                <MemoizedMarkdownChunk key={index} content={chunk} components={markdownComponents} />
              ))}

              <div className="h-8">
                {!isFullyRendered && (
                  <div className="flex items-center gap-2 text-[var(--color-text-tertiary)]">
                    <div className="w-4 h-4 border-2 border-[var(--color-text-tertiary)] border-t-transparent rounded-full animate-spin" />
                    <span className="text-sm">Loading...</span>
                  </div>
                )}
              </div>
            </article>

          </div>

          {/* Metadata sidebar — right side on lg+ screens, above prose on mobile */}
          <div className="hidden lg:block w-80 shrink-0 border border-[var(--color-border)] rounded-lg p-4 self-start">
            {/* Source */}
            {atom.source_url && (
              <div className="mb-4">
                <a
                  href={atom.source_url}
                  onClick={(e) => { e.preventDefault(); openExternalUrl(atom.source_url!).catch(console.error); }}
                  className="inline-block text-xs text-[var(--color-text-tertiary)] hover:text-[var(--color-accent)] bg-[var(--color-bg-card)] px-2 py-0.5 rounded-full cursor-pointer transition-colors"
                >
                  {atom.source || (() => { try { return new URL(atom.source_url!).hostname.replace(/^www\./, ''); } catch { return atom.source_url; } })()}
                </a>
              </div>
            )}

            {/* Tags */}
            {atom.tags.length > 0 && (
              <div className="flex flex-wrap gap-1.5 mb-4">
                {atom.tags.map((tag) => (
                  <TagChip
                    key={tag.id}
                    name={tag.name}
                    size="sm"
                    onClick={() => onTagClick(tag.id)}
                  />
                ))}
              </div>
            )}

            {/* Dates */}
            <div className="text-xs text-[var(--color-text-tertiary)] space-y-0.5">
              {atom.published_at && <p>{formatDate(atom.published_at)}</p>}
              <p>{formatDate(atom.updated_at)}</p>
            </div>

            {/* Neighborhood graph — always visible */}
            {atom.embedding_status !== 'failed' && (
              <div className="mt-4">
                <MiniGraphPreview atomId={atom.id} onExpand={onViewGraph} />
              </div>
            )}

            {/* Related atoms — collapsible */}
            {atom.embedding_status !== 'failed' && (
              <SidebarRelatedAtoms atomId={atom.id} onAtomClick={onRelatedAtomClick} />
            )}
          </div>
        </div>
      </div>

      {/* Delete Confirmation Modal */}
      <Modal
        isOpen={showDeleteModal}
        onClose={() => setShowDeleteModal(false)}
        title="Delete Atom"
        confirmLabel={isDeleting ? 'Deleting...' : 'Delete'}
        confirmVariant="danger"
        onConfirm={handleDelete}
      >
        <p>Are you sure you want to delete this atom? This action cannot be undone.</p>
      </Modal>
    </div>
  );
}

function SidebarRelatedAtoms({ atomId, onAtomClick }: { atomId: string; onAtomClick: (id: string) => void }) {
  const [relatedAtoms, setRelatedAtoms] = useState<SimilarAtomResult[]>([]);
  const [isCollapsed, setIsCollapsed] = useState(true);
  const [hasLoaded, setHasLoaded] = useState(false);
  const [isLoading, setIsLoading] = useState(false);

  // Reset when atomId changes so we re-fetch for the new atom
  useEffect(() => {
    setRelatedAtoms([]);
    setHasLoaded(false);
  }, [atomId]);

  useEffect(() => {
    if (!isCollapsed && !hasLoaded) {
      setIsLoading(true);
      getTransport().invoke<SimilarAtomResult[]>('find_similar_atoms', { atomId, limit: 5, threshold: 0.7 })
        .then((results) => { setRelatedAtoms(results); setHasLoaded(true); })
        .catch(console.error)
        .finally(() => setIsLoading(false));
    }
  }, [atomId, isCollapsed, hasLoaded]);

  return (
    <div className="mt-4">
      <button
        onClick={() => setIsCollapsed(!isCollapsed)}
        className="flex items-center justify-between w-full text-xs font-medium text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] transition-colors"
      >
        <span>Related atoms</span>
        <svg className={`w-3 h-3 transition-transform ${isCollapsed ? '' : 'rotate-180'}`} fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
        </svg>
      </button>
      {!isCollapsed && (
        <div className="mt-2 space-y-1.5">
          {isLoading ? (
            <div className="text-xs text-[var(--color-text-tertiary)]">Loading...</div>
          ) : relatedAtoms.length > 0 ? (
            relatedAtoms.map((result) => (
              <button
                key={result.id}
                onClick={() => onAtomClick(result.id)}
                className="w-full text-left p-2 rounded-md hover:bg-[var(--color-bg-hover)] transition-colors"
              >
                <p className="text-xs text-[var(--color-text-primary)] line-clamp-2">
                  {result.title || 'Untitled'}
                </p>
                <span className="text-[10px] text-[var(--color-accent)]">
                  {Math.round(result.similarity_score * 100)}% similar
                </span>
              </button>
            ))
          ) : hasLoaded ? (
            <div className="text-xs text-[var(--color-text-tertiary)]">No similar atoms found</div>
          ) : null}
        </div>
      )}
    </div>
  );
}
