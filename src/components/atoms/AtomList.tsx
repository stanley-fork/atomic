import { memo, useRef } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { AtomWithTags } from '../../stores/atoms';
import { AtomCard } from './AtomCard';

interface AtomListProps {
  atoms: AtomWithTags[];
  onAtomClick: (atomId: string) => void;
  getMatchingChunkContent?: (atomId: string) => string | undefined;
  onRetryEmbedding?: (atomId: string) => void;
}

export const AtomList = memo(function AtomList({
  atoms,
  onAtomClick,
  getMatchingChunkContent,
  onRetryEmbedding,
}: AtomListProps) {
  const parentRef = useRef<HTMLDivElement>(null);

  const virtualizer = useVirtualizer({
    count: atoms.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 76,
    overscan: 10,
    gap: 8,
  });

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
        className="relative w-full px-4 pt-4"
        style={{ height: `${virtualizer.getTotalSize() + 16}px` }}
      >
        {virtualizer.getVirtualItems().map((virtualItem) => {
          const atom = atoms[virtualItem.index];
          return (
            <div
              key={atom.id}
              className="absolute left-4 right-4"
              style={{
                top: `${virtualItem.start}px`,
              }}
              ref={virtualizer.measureElement}
              data-index={virtualItem.index}
            >
              <AtomCard
                atom={atom}
                onAtomClick={onAtomClick}
                viewMode="list"
                matchingChunkContent={getMatchingChunkContent?.(atom.id)}
                onRetryEmbedding={onRetryEmbedding}
              />
            </div>
          );
        })}
      </div>
    </div>
  );
});
