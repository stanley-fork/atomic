import { useState, useEffect, useCallback, useLayoutEffect, ReactNode, useRef, useMemo } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { openExternalUrl } from '../../lib/platform';
import { Button } from '../ui/Button';
import { Modal } from '../ui/Modal';
import { SearchBar } from '../ui/SearchBar';
import { MarkdownImage } from '../ui/MarkdownImage';
import { TagChip } from '../tags/TagChip';
import { RelatedAtoms } from './RelatedAtoms';
import { AtomWithTags } from '../../stores/atoms';
import { useAtomsStore } from '../../stores/atoms';
import { useTagsStore } from '../../stores/tags';
import { useUIStore } from '../../stores/ui';
import { useContentSearch } from '../../hooks';
import { formatDate } from '../../lib/date';
import { chunkMarkdown, findChunkIndexForOffset } from '../../lib/markdown';

// Benchmarking helper
const PERF_DEBUG = true;
const perfLog = (label: string, startTime?: number) => {
  if (!PERF_DEBUG) return;
  if (startTime !== undefined) {
    console.log(`[AtomViewer] ${label}: ${(performance.now() - startTime).toFixed(2)}ms`);
  } else {
    console.log(`[AtomViewer] ${label}`);
  }
};

interface AtomViewerProps {
  atom: AtomWithTags;
  onClose: () => void;
  onEdit: () => void;
  highlightText?: string | null;  // Raw text to highlight and scroll to (from semantic search)
}

// Progressive rendering configuration
const CHUNK_SIZE = 8000; // ~8KB per chunk
const INITIAL_CHUNKS = 1; // Render 1 chunk immediately
const CHUNKS_PER_BATCH = 2; // Render 2 chunks at a time to reduce re-renders
const CHUNK_DELAY = 32; // ~2 frames between batches

export function AtomViewer({ atom, onClose, onEdit, highlightText }: AtomViewerProps) {
  const mountTimeRef = useRef(performance.now());
  const renderCountRef = useRef(0);

  const deleteAtom = useAtomsStore(s => s.deleteAtom);
  const fetchTags = useTagsStore(s => s.fetchTags);
  const setSelectedTag = useUIStore(s => s.setSelectedTag);
  const closeDrawer = useUIStore(s => s.closeDrawer);
  const openDrawer = useUIStore(s => s.openDrawer);
  const openLocalGraph = useUIStore(s => s.openLocalGraph);
  const [showDeleteModal, setShowDeleteModal] = useState(false);
  const [isDeleting, setIsDeleting] = useState(false);
  const [metadataExpanded, setMetadataExpanded] = useState(false);

  // Initial highlight state (from semantic search)
  const [initialHighlight, setInitialHighlight] = useState<string | null>(null);
  const [targetChunkIndex, setTargetChunkIndex] = useState<number | null>(null);
  const scrollContainerRef = useRef<HTMLDivElement | null>(null);

  // Progressive rendering: chunk the content
  const chunks = useMemo(() => {
    const result = chunkMarkdown(atom.content, CHUNK_SIZE);
    perfLog(`Content chunked: ${result.length} chunks from ${atom.content.length} chars`);
    return result;
  }, [atom.content]);

  // Track how many chunks are currently rendered
  const [renderedChunkCount, setRenderedChunkCount] = useState(INITIAL_CHUNKS);
  const isFullyRendered = renderedChunkCount >= chunks.length;

  // Progressive loading: render more chunks over time
  useEffect(() => {
    if (isFullyRendered) return;

    const loadNextBatch = () => {
      setRenderedChunkCount(prev => {
        const next = Math.min(prev + CHUNKS_PER_BATCH, chunks.length);
        if (next < chunks.length) {
          perfLog(`Rendered chunks ${prev + 1}-${next}/${chunks.length}`);
        } else {
          perfLog(`All ${chunks.length} chunks rendered`);
        }
        return next;
      });
    };

    // Use requestIdleCallback if available, otherwise setTimeout
    if ('requestIdleCallback' in window) {
      const id = requestIdleCallback(loadNextBatch, { timeout: 100 });
      return () => cancelIdleCallback(id);
    } else {
      const id = setTimeout(loadNextBatch, CHUNK_DELAY);
      return () => clearTimeout(id);
    }
  }, [renderedChunkCount, chunks.length, isFullyRendered]);

  // Reset chunk count when atom changes
  useEffect(() => {
    setRenderedChunkCount(INITIAL_CHUNKS);
  }, [atom.id]);

  // Calculate target chunk and highlight query from highlightText prop
  useEffect(() => {
    if (highlightText && atom.content) {
      // Find position of matching chunk in raw content
      const offset = atom.content.indexOf(highlightText);
      if (offset !== -1) {
        // Calculate which render chunk contains this offset
        const chunkIndex = findChunkIndexForOffset(atom.content, offset, CHUNK_SIZE);
        setTargetChunkIndex(chunkIndex);
        // Use first ~50 chars as highlight query for uniqueness
        setInitialHighlight(highlightText.slice(0, 50).trim());
        perfLog(`Initial highlight: targeting chunk ${chunkIndex} at offset ${offset}`);
      }
    } else {
      setInitialHighlight(null);
      setTargetChunkIndex(null);
    }
  }, [highlightText, atom.content]);

  // Ensure target chunk is rendered, then scroll to highlighted element
  useEffect(() => {
    if (targetChunkIndex === null || initialHighlight === null) return;

    if (targetChunkIndex >= renderedChunkCount) {
      // Need to render more chunks to reach target - force immediate render
      setRenderedChunkCount(targetChunkIndex + 1);
      return;
    }

    // Target chunk is rendered - scroll to highlighted element after DOM update
    const scrollTimeout = setTimeout(() => {
      const mark = document.querySelector('[data-initial-highlight]');
      if (mark && scrollContainerRef.current) {
        mark.scrollIntoView({ behavior: 'smooth', block: 'center' });
        perfLog('Scrolled to initial highlight');
      }
    }, 100); // Small delay to ensure DOM is ready

    // Clear highlight after 5 seconds
    const clearTimeout_ = setTimeout(() => {
      setInitialHighlight(null);
      setTargetChunkIndex(null);
      perfLog('Initial highlight cleared');
    }, 5000);

    return () => {
      clearTimeout(scrollTimeout);
      clearTimeout(clearTimeout_);
    };
  }, [targetChunkIndex, renderedChunkCount, initialHighlight]);

  // Track mount/unmount and render count
  useEffect(() => {
    perfLog(`MOUNTED (content: ${atom.content.length} chars, tags: ${atom.tags.length})`);
    perfLog('Time from component creation to mount', mountTimeRef.current);

    return () => {
      perfLog(`UNMOUNTED after ${renderCountRef.current} renders`);
    };
  }, []);

  // Track each render
  useEffect(() => {
    renderCountRef.current++;
  });

  // Track when markdown content is rendered to DOM using useLayoutEffect
  const renderStartRef = useRef<number>(performance.now());
  const articleRef = useRef<HTMLElement | null>(null);

  // Preserve scroll position when new chunks are added
  useLayoutEffect(() => {
    // This runs synchronously after DOM mutations, before browser paint
    // By doing nothing special here, we let the browser maintain scroll position naturally
    // The key is that we're adding content at the END, not the beginning
    if (articleRef.current) {
      const domNodes = articleRef.current.querySelectorAll('*').length;
      const images = articleRef.current.querySelectorAll('img').length;
      perfLog(`DOM ready (render #${renderCountRef.current})`, renderStartRef.current);
      perfLog(`  DOM nodes: ${domNodes}, Images: ${images}, Chunks: ${renderedChunkCount}/${chunks.length}`);
    }
    // Reset for next render
    renderStartRef.current = performance.now();
  }, [renderedChunkCount, chunks.length]);

  // Content search
  const {
    isOpen: isSearchOpen,
    query: searchQuery,
    searchedQuery,
    currentIndex,
    totalMatches,
    setQuery: setSearchQuery,
    openSearch,
    closeSearch,
    goToNext,
    goToPrevious,
    processChildren,
  } = useContentSearch(atom.content);

  // Keyboard handler for Ctrl+F / Cmd+F
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === 'f') {
        e.preventDefault();
        openSearch();
      }
    };
    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [openSearch]);

  // Helper to highlight initial search text (from semantic search)
  const highlightInitialText = useCallback(
    (text: string): ReactNode => {
      if (!initialHighlight || !text) return text;

      const lowerText = text.toLowerCase();
      const lowerQuery = initialHighlight.toLowerCase();
      const index = lowerText.indexOf(lowerQuery);

      if (index === -1) return text;

      const before = text.slice(0, index);
      const match = text.slice(index, index + initialHighlight.length);
      const after = text.slice(index + initialHighlight.length);

      return (
        <>
          {before}
          <mark data-initial-highlight="true" className="initial-highlight">
            {match}
          </mark>
          {after}
        </>
      );
    },
    [initialHighlight]
  );

  // Process children recursively for initial highlight
  const processInitialHighlight = useCallback(
    (children: ReactNode): ReactNode => {
      if (typeof children === 'string') {
        return highlightInitialText(children);
      }
      if (Array.isArray(children)) {
        return children.map((child, i) => (
          <span key={i}>{processInitialHighlight(child)}</span>
        ));
      }
      return children;
    },
    [highlightInitialText]
  );

  // Wrap children with search highlighting (both manual search and initial highlight)
  const wrapWithHighlight = useCallback(
    (children: ReactNode): ReactNode => {
      // First apply initial highlight if active
      if (initialHighlight) {
        return processInitialHighlight(children);
      }
      // Then apply manual search highlight if active
      if (isSearchOpen && searchQuery.trim()) {
        return processChildren(children);
      }
      return children;
    },
    [isSearchOpen, searchQuery, processChildren, initialHighlight, processInitialHighlight]
  );

  // Memoize markdown components to prevent re-renders from resetting image loading states
  const markdownComponents = useMemo(() => ({
    p: ({ children }: { children?: ReactNode }) => (
      <p>{wrapWithHighlight(children)}</p>
    ),
    li: ({ children }: { children?: ReactNode }) => (
      <li>{wrapWithHighlight(children)}</li>
    ),
    td: ({ children }: { children?: ReactNode }) => (
      <td>{wrapWithHighlight(children)}</td>
    ),
    th: ({ children }: { children?: ReactNode }) => (
      <th>{wrapWithHighlight(children)}</th>
    ),
    strong: ({ children }: { children?: ReactNode }) => (
      <strong>{wrapWithHighlight(children)}</strong>
    ),
    em: ({ children }: { children?: ReactNode }) => (
      <em>{wrapWithHighlight(children)}</em>
    ),
    del: ({ children }: { children?: ReactNode }) => (
      <del>{wrapWithHighlight(children)}</del>
    ),
    h1: ({ children }: { children?: ReactNode }) => (
      <h1>{wrapWithHighlight(children)}</h1>
    ),
    h2: ({ children }: { children?: ReactNode }) => (
      <h2>{wrapWithHighlight(children)}</h2>
    ),
    h3: ({ children }: { children?: ReactNode }) => (
      <h3>{wrapWithHighlight(children)}</h3>
    ),
    h4: ({ children }: { children?: ReactNode }) => (
      <h4>{wrapWithHighlight(children)}</h4>
    ),
    h5: ({ children }: { children?: ReactNode }) => (
      <h5>{wrapWithHighlight(children)}</h5>
    ),
    h6: ({ children }: { children?: ReactNode }) => (
      <h6>{wrapWithHighlight(children)}</h6>
    ),
    blockquote: ({ children }: { children?: ReactNode }) => (
      <blockquote>{wrapWithHighlight(children)}</blockquote>
    ),
    code: ({ className, children }: { className?: string; children?: ReactNode }) => {
      const isBlock = className?.startsWith('language-');
      if (isBlock) {
        return <code className={className}>{wrapWithHighlight(children)}</code>;
      }
      return <code>{wrapWithHighlight(children)}</code>;
    },
    pre: ({ children }: { children?: ReactNode }) => (
      <pre>{children}</pre>
    ),
    a: ({ href, children }: { href?: string; children?: ReactNode }) => (
      <a
        href={href}
        onClick={(e) => {
          e.preventDefault();
          if (href) {
            openExternalUrl(href).catch(err => console.error('Failed to open URL:', err));
          }
        }}
        className="cursor-pointer"
      >
        {wrapWithHighlight(children)}
      </a>
    ),
    img: ({ src, alt }: { src?: string; alt?: string }) => (
      <MarkdownImage src={src} alt={alt} />
    ),
  }), [wrapWithHighlight]);

  const handleViewNeighborhood = () => {
    closeDrawer();
    openLocalGraph(atom.id);
  };

  const MAX_VISIBLE_TAGS = 5;
  const visibleTags = atom.tags.slice(0, MAX_VISIBLE_TAGS);
  const hiddenCount = atom.tags.length - MAX_VISIBLE_TAGS;

  const handleDelete = async () => {
    setIsDeleting(true);
    try {
      await deleteAtom(atom.id);
      await fetchTags();
      closeDrawer();
    } catch (error) {
      console.error('Failed to delete atom:', error);
    } finally {
      setIsDeleting(false);
      setShowDeleteModal(false);
    }
  };

  const handleTagClick = (tagId: string) => {
    setSelectedTag(tagId);
    closeDrawer();
  };

  const handleRelatedAtomClick = (atomId: string) => {
    // Open the related atom in the viewer
    openDrawer('viewer', atomId);
  };

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center justify-between px-6 py-4 border-b border-[var(--color-border)]">
        <button
          onClick={onClose}
          className="text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] transition-colors"
        >
          <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
        <div className="flex items-center gap-2">
          {atom.embedding_status === 'complete' && (
            <>
              <Button variant="ghost" size="sm" onClick={handleViewNeighborhood} title="View neighborhood graph">
                <svg className="w-4 h-4 mr-1.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <circle cx="12" cy="12" r="3" strokeWidth={2} />
                  <circle cx="5" cy="8" r="2" strokeWidth={2} />
                  <circle cx="19" cy="8" r="2" strokeWidth={2} />
                  <circle cx="7" cy="18" r="2" strokeWidth={2} />
                  <circle cx="17" cy="18" r="2" strokeWidth={2} />
                  <path strokeLinecap="round" strokeWidth={2} d="M9.5 10.5L6.5 9M14.5 10.5L17.5 9M10 14L8 16.5M14 14L16 16.5" />
                </svg>
                Graph
              </Button>
            </>
          )}
          <Button variant="ghost" size="sm" onClick={onEdit}>
            <svg className="w-4 h-4 mr-1.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
            </svg>
            Edit
          </Button>
          <Button variant="ghost" size="sm" onClick={() => setShowDeleteModal(true)}>
            <svg className="w-4 h-4 mr-1.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
            </svg>
            Delete
          </Button>
        </div>
      </div>

      {/* Content */}
      <div ref={scrollContainerRef} className="flex-1 overflow-y-auto px-6 py-4 relative">
        {/* Search bar */}
        {isSearchOpen && (
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
        )}

        <article
          ref={articleRef}
          className="prose prose-invert prose-sm max-w-none prose-headings:text-[var(--color-text-primary)] prose-p:text-[var(--color-text-primary)] prose-a:text-[var(--color-text-primary)] prose-a:underline prose-a:decoration-[var(--color-border-hover)] hover:prose-a:decoration-current prose-strong:text-[var(--color-text-primary)] prose-code:text-[var(--color-accent-light)] prose-code:bg-[var(--color-bg-card)] prose-code:px-1 prose-code:py-0.5 prose-code:rounded prose-pre:bg-[var(--color-bg-card)] prose-pre:border prose-pre:border-[var(--color-border)] prose-blockquote:border-l-[var(--color-accent)] prose-blockquote:text-[var(--color-text-secondary)] prose-li:text-[var(--color-text-primary)] prose-hr:border-[var(--color-border)]"
        >
          {/* Render chunks progressively */}
          {chunks.slice(0, renderedChunkCount).map((chunk, index) => (
            <ReactMarkdown
              key={index}
              remarkPlugins={[remarkGfm]}
              components={markdownComponents}
            >
              {chunk}
            </ReactMarkdown>
          ))}

          {/* Loading indicator for remaining chunks - fixed height to prevent layout shift */}
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

      {/* Metadata */}
      <div className="border-t border-[var(--color-border)] px-6 py-4">
        {/* Source URL - always visible when present */}
        {atom.source_url && (
          <div className="flex items-center gap-2 text-sm mb-3">
            <svg className="w-3.5 h-3.5 text-[var(--color-text-tertiary)] shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13.828 10.172a4 4 0 00-5.656 0l-4 4a4 4 0 105.656 5.656l1.102-1.101m-.758-4.899a4 4 0 005.656 0l4-4a4 4 0 00-5.656-5.656l-1.1 1.1" />
            </svg>
            {atom.source && (
              <span className="text-xs text-[var(--color-text-tertiary)] bg-[var(--color-bg-panel)] px-1.5 py-0.5 rounded shrink-0">
                {atom.source}
              </span>
            )}
            <a
              href={atom.source_url}
              onClick={(e) => {
                e.preventDefault();
                openExternalUrl(atom.source_url!).catch(err => console.error('Failed to open URL:', err));
              }}
              className="text-[var(--color-accent)] hover:underline truncate cursor-pointer"
            >
              {atom.source_url}
            </a>
          </div>
        )}

        {/* Collapsible header with tags */}
        <button
          onClick={() => setMetadataExpanded(!metadataExpanded)}
          className="flex items-center justify-between w-full"
        >
          <div className="flex flex-wrap items-center gap-1.5 flex-1">
            {/* Show first 5 tags inline */}
            {atom.tags.length > 0 && (
              <>
                {visibleTags.map((tag) => (
                  <TagChip
                    key={tag.id}
                    name={tag.name}
                    size="md"
                    onClick={(e) => {
                      e.stopPropagation();
                      handleTagClick(tag.id);
                    }}
                  />
                ))}
                {hiddenCount > 0 && !metadataExpanded && (
                  <span className="text-sm text-[var(--color-text-secondary)] px-2">
                    +{hiddenCount} more
                  </span>
                )}
              </>
            )}
          </div>
          <svg
            className={`w-4 h-4 text-[var(--color-text-secondary)] transition-transform ml-2 flex-shrink-0 ${metadataExpanded ? 'rotate-180' : ''}`}
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
          </svg>
        </button>

        {/* Expanded metadata */}
        {metadataExpanded && (
          <div className="mt-3 space-y-3">
            {/* Additional tags when expanded */}
            {atom.tags.length > MAX_VISIBLE_TAGS && (
              <div className="space-y-1">
                <div className="flex flex-wrap gap-1.5">
                  {atom.tags.slice(MAX_VISIBLE_TAGS).map((tag) => (
                    <TagChip
                      key={tag.id}
                      name={tag.name}
                      size="md"
                      onClick={() => handleTagClick(tag.id)}
                    />
                  ))}
                </div>
              </div>
            )}

            {/* Dates */}
            <div className="text-xs text-[var(--color-text-tertiary)] space-y-1">
              {atom.published_at && (
                <p>Published: {formatDate(atom.published_at)}</p>
              )}
              <p>Created: {formatDate(atom.created_at)}</p>
              <p>Updated: {formatDate(atom.updated_at)}</p>
            </div>
          </div>
        )}
      </div>

      {/* Related Atoms & Neighborhood - only show if embedding is complete */}
      {atom.embedding_status === 'complete' && (
        <RelatedAtoms
          atomId={atom.id}
          onAtomClick={handleRelatedAtomClick}
          onViewGraph={handleViewNeighborhood}
        />
      )}

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

