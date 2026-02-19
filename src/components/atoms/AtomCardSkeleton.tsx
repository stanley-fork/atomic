import { memo } from 'react';

interface AtomCardSkeletonProps {
  viewMode: 'grid' | 'list';
}

export const AtomCardSkeleton = memo(function AtomCardSkeleton({ viewMode }: AtomCardSkeletonProps) {
  if (viewMode === 'list') {
    return (
      <div className="flex items-center gap-3 px-3 py-2.5 bg-[var(--color-bg-card)] border border-[var(--color-border)] rounded-lg">
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <div className="h-4 w-32 bg-[var(--color-border)] rounded animate-pulse" />
            <div className="h-3 w-48 bg-[var(--color-border)] rounded animate-pulse opacity-60" />
          </div>
          <div className="flex items-center gap-1.5 mt-1">
            <div className="h-5 w-14 bg-[var(--color-border)] rounded-full animate-pulse opacity-50" />
            <div className="h-5 w-18 bg-[var(--color-border)] rounded-full animate-pulse opacity-50" />
          </div>
        </div>
        <div className="h-3 w-12 bg-[var(--color-border)] rounded animate-pulse opacity-40" />
      </div>
    );
  }

  return (
    <div className="flex flex-col p-4 bg-[var(--color-bg-card)] border border-[var(--color-border)] rounded-lg h-full">
      <div className="flex-1">
        <div className="h-4 w-3/5 bg-[var(--color-border)] rounded animate-pulse" />
        <div className="mt-2 space-y-2">
          <div className="h-3 w-full bg-[var(--color-border)] rounded animate-pulse opacity-60" />
          <div className="h-3 w-4/5 bg-[var(--color-border)] rounded animate-pulse opacity-60" />
          <div className="h-3 w-2/3 bg-[var(--color-border)] rounded animate-pulse opacity-60" />
        </div>
      </div>
      <div className="mt-3 pt-3 border-t border-[var(--color-border)]">
        <div className="flex items-center gap-1.5 mb-2">
          <div className="h-5 w-14 bg-[var(--color-border)] rounded-full animate-pulse opacity-50" />
          <div className="h-5 w-18 bg-[var(--color-border)] rounded-full animate-pulse opacity-50" />
        </div>
        <div className="h-3 w-16 bg-[var(--color-border)] rounded animate-pulse opacity-40" />
      </div>
    </div>
  );
});
