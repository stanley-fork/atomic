import { memo } from 'react';
import { AtomWithTags } from '../../stores/atoms';
import { TagChip } from '../tags/TagChip';
import { extractTitleAndSnippet } from '../../lib/markdown';
import { formatRelativeDate } from '../../lib/date';

interface AtomCardProps {
  atom: AtomWithTags;
  onAtomClick: (atomId: string) => void;
  viewMode: 'grid' | 'list';
  matchingChunkContent?: string;  // For search results
  onRetryEmbedding?: (atomId: string) => void;  // For retry action
}

function ProcessingStatusIndicator({
  embeddingStatus,
  taggingStatus,
  onRetry,
}: {
  embeddingStatus: AtomWithTags['embedding_status'];
  taggingStatus: AtomWithTags['tagging_status'];
  onRetry?: () => void;
}) {
  // Show failed state if embedding failed
  if (embeddingStatus === 'failed') {
    return (
      <button
        onClick={(e) => {
          e.stopPropagation();
          onRetry?.();
        }}
        className="absolute top-2 right-2 text-red-500 hover:text-red-400 transition-colors"
        title="Embedding failed - click to retry"
      >
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"
          />
        </svg>
      </button>
    );
  }

  // Determine if still processing (either embedding or tagging not complete)
  const isEmbedding = embeddingStatus === 'pending' || embeddingStatus === 'processing';
  const isTagging = taggingStatus === 'pending' || taggingStatus === 'processing';
  const taggingFailed = taggingStatus === 'failed';

  // Both complete (or tagging skipped) - no indicator needed
  if (embeddingStatus === 'complete' && (taggingStatus === 'complete' || taggingStatus === 'skipped')) {
    return null;
  }

  // Show amber indicator for pending/processing states
  if (isEmbedding || isTagging) {
    let title = 'Processing...';
    if (isEmbedding) {
      title = embeddingStatus === 'pending' ? 'Embedding pending' : 'Embedding in progress';
    } else if (isTagging) {
      title = taggingStatus === 'pending' ? 'Tag extraction pending' : 'Tag extraction in progress';
    }

    return (
      <div
        className="absolute top-2 right-2 w-2.5 h-2.5 bg-amber-500 rounded-full animate-pulse"
        title={title}
      />
    );
  }

  // Tagging failed (but embedding succeeded) - show warning but less critical
  if (taggingFailed) {
    return (
      <div
        className="absolute top-2 right-2 w-2.5 h-2.5 bg-orange-500 rounded-full"
        title="Tag extraction failed - atom is still searchable"
      />
    );
  }

  return null;
}

export const AtomCard = memo(function AtomCard({
  atom,
  onAtomClick,
  viewMode,
  matchingChunkContent,
  onRetryEmbedding,
}: AtomCardProps) {
  const handleClick = () => onAtomClick(atom.id);
  const handleRetry = onRetryEmbedding ? () => onRetryEmbedding(atom.id) : undefined;
  const { title, snippet } = extractTitleAndSnippet(atom.content, 120);

  const maxVisibleTags = viewMode === 'grid' ? 2 : 3;
  const visibleTags = atom.tags.slice(0, maxVisibleTags);
  const remainingTags = atom.tags.length - maxVisibleTags;

  if (viewMode === 'list') {
    return (
      <div
        onClick={handleClick}
        className="relative flex items-center gap-4 p-4 bg-[var(--color-bg-card)] border border-[var(--color-border)] rounded-lg cursor-pointer hover:border-[var(--color-border-hover)] hover:bg-[var(--color-bg-hover)] transition-all duration-150"
      >
        <ProcessingStatusIndicator
          embeddingStatus={atom.embedding_status}
          taggingStatus={atom.tagging_status}
          onRetry={handleRetry}
        />
        <div className="flex-1 min-w-0">
          <p
            className={`text-sm font-medium line-clamp-1 ${
              matchingChunkContent ? 'text-[var(--color-accent-light)]' : 'text-[var(--color-text-primary)]'
            }`}
          >
            {title || 'Untitled'}
          </p>
          {atom.tags.length > 0 && (
            <div className="flex items-center gap-1.5 mt-2">
              {visibleTags.map((tag) => (
                <TagChip key={tag.id} name={tag.name} size="sm" />
              ))}
              {remainingTags > 0 && (
                <span className="text-xs text-[var(--color-text-tertiary)]">+{remainingTags} more</span>
              )}
            </div>
          )}
        </div>
        <span className="text-xs text-[var(--color-text-tertiary)] whitespace-nowrap">
          {formatRelativeDate(atom.created_at)}
        </span>
      </div>
    );
  }

  return (
    <div
      onClick={handleClick}
      className="relative flex flex-col p-4 bg-[var(--color-bg-card)] border border-[var(--color-border)] rounded-lg cursor-pointer hover:border-[var(--color-border-hover)] hover:bg-[var(--color-bg-hover)] transition-all duration-150 h-full"
    >
      <ProcessingStatusIndicator
        embeddingStatus={atom.embedding_status}
        taggingStatus={atom.tagging_status}
        onRetry={handleRetry}
      />
      <div className="flex-1 min-h-0">
        <p
          className={`text-sm font-medium line-clamp-2 ${
            matchingChunkContent ? 'text-[var(--color-accent-light)]' : 'text-[var(--color-text-primary)]'
          }`}
        >
          {title || 'Untitled'}
        </p>
        {snippet && (
          <p className="text-sm text-[var(--color-text-secondary)] line-clamp-2 mt-1 leading-relaxed">
            {snippet}
          </p>
        )}
      </div>
      <div className="mt-3 pt-3 border-t border-[var(--color-border)]">
        {atom.tags.length > 0 && (
          <div className="flex items-center gap-1.5 mb-2">
            {visibleTags.map((tag) => (
              <TagChip key={tag.id} name={tag.name} size="sm" />
            ))}
            {remainingTags > 0 && (
              <span className="text-xs text-[var(--color-text-tertiary)] shrink-0">+{remainingTags}</span>
            )}
          </div>
        )}
        <span className="text-xs text-[var(--color-text-tertiary)]">
          {formatRelativeDate(atom.created_at)}
        </span>
      </div>
    </div>
  );
}, (prev, next) => {
  return prev.atom.id === next.atom.id
    && prev.atom.updated_at === next.atom.updated_at
    && prev.atom.embedding_status === next.atom.embedding_status
    && prev.atom.tagging_status === next.atom.tagging_status
    && prev.atom.tags.length === next.atom.tags.length
    && prev.viewMode === next.viewMode
    && prev.matchingChunkContent === next.matchingChunkContent;
});

