import { memo, useRef, useEffect } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { DisplayAtom } from '../../stores/atoms';
import { AtomCard } from './AtomCard';
import { AtomCardSkeleton } from './AtomCardSkeleton';
import { useContainerWidth } from '../../hooks/useContainerWidth';

const CARD_MIN_WIDTH = 260;
const CARD_GAP = 16;
const PADDING = 16;
const ROW_HEIGHT = 220;

interface AtomGridProps {
  atoms: DisplayAtom[];
  onAtomClick: (atomId: string) => void;
  getMatchingChunkContent?: (atomId: string) => string | undefined;
  onRetryEmbedding?: (atomId: string) => void;
  onLoadMore?: () => void;
  isLoading?: boolean;
  isLoadingMore?: boolean;
}

export const AtomGrid = memo(function AtomGrid({
  atoms,
  onAtomClick,
  getMatchingChunkContent,
  onRetryEmbedding,
  onLoadMore,
  isLoading,
  isLoadingMore,
}: AtomGridProps) {
  const parentRef = useRef<HTMLDivElement>(null);
  const containerWidth = useContainerWidth(parentRef);

  const ready = containerWidth > 0;
  const columnCount = ready
    ? Math.max(1, Math.floor((containerWidth - PADDING * 2 + CARD_GAP) / (CARD_MIN_WIDTH + CARD_GAP)))
    : 1;
  const rowCount = Math.ceil(atoms.length / columnCount);

  const virtualizer = useVirtualizer({
    count: rowCount,
    getScrollElement: () => parentRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 3,
    gap: CARD_GAP,
    enabled: ready,
  });

  // Load more when nearing the end
  useEffect(() => {
    if (!onLoadMore || !ready) return;
    const items = virtualizer.getVirtualItems();
    if (items.length === 0) return;
    const lastItem = items[items.length - 1];
    if (lastItem && lastItem.index >= rowCount - 3) {
      onLoadMore();
    }
  }, [virtualizer.getVirtualItems(), rowCount, onLoadMore, ready]);

  if (atoms.length === 0 && isLoading) {
    return (
      <div ref={parentRef} className="h-full overflow-y-auto p-4">
        <div
          style={{
            display: 'grid',
            gridTemplateColumns: `repeat(auto-fill, minmax(${CARD_MIN_WIDTH}px, 1fr))`,
            gap: `${CARD_GAP}px`,
          }}
        >
          {Array.from({ length: 8 }, (_, i) => (
            <AtomCardSkeleton key={i} viewMode="grid" />
          ))}
        </div>
      </div>
    );
  }

  if (atoms.length === 0) {
    return (
      <div ref={parentRef} className="flex flex-col items-center justify-center h-full text-center p-8">
        <svg
          className="w-16 h-16 text-[var(--color-border)] mb-4"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={1.5}
            d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
          />
        </svg>
        <h3 className="text-lg font-medium text-[var(--color-text-primary)] mb-2">No atoms yet</h3>
        <p className="text-sm text-[var(--color-text-secondary)] max-w-sm">
          Click the + button to create your first atom and start building your knowledge base.
        </p>
      </div>
    );
  }

  return (
    <div ref={parentRef} className="h-full overflow-y-auto">
      <div
        className="relative w-full p-4"
        style={{ height: `${virtualizer.getTotalSize() + PADDING * 2 + (isLoadingMore ? 48 : 0)}px` }}
      >
        {ready && virtualizer.getVirtualItems().map((virtualRow) => {
          const startIndex = virtualRow.index * columnCount;
          const rowAtoms = atoms.slice(startIndex, startIndex + columnCount);
          return (
            <div
              key={virtualRow.key}
              className="absolute left-4 right-4"
              style={{
                top: `${virtualRow.start}px`,
                height: `${virtualRow.size}px`,
                display: 'grid',
                gridTemplateColumns: `repeat(${columnCount}, 1fr)`,
                gap: `${CARD_GAP}px`,
              }}
            >
              {rowAtoms.map((atom) => (
                <AtomCard
                  key={atom.id}
                  atom={atom}
                  onAtomClick={onAtomClick}
                  viewMode="grid"
                  matchingChunkContent={getMatchingChunkContent?.(atom.id)}
                  onRetryEmbedding={onRetryEmbedding}
                />
              ))}
            </div>
          );
        })}
        {isLoadingMore && (
          <div
            className="absolute left-0 right-0 flex justify-center py-3"
            style={{ top: `${virtualizer.getTotalSize()}px` }}
          >
            <div className="h-5 w-5 border-2 border-[var(--color-border)] border-t-[var(--color-accent)] rounded-full animate-spin" />
          </div>
        )}
      </div>
    </div>
  );
});
