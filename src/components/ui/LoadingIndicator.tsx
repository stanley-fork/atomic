import { Loader2 } from 'lucide-react';
import { useUIStore } from '../../stores/ui';

export function LoadingIndicator() {
  const loadingOperations = useUIStore(s => s.loadingOperations);

  if (loadingOperations.length === 0) return null;

  // Get most recent operation message
  const latestOp = loadingOperations[loadingOperations.length - 1];
  const count = loadingOperations.length;

  return (
    <div
      className="fixed bottom-5 right-5 mb-[env(safe-area-inset-bottom)] mr-[env(safe-area-inset-right)] z-40 bg-[var(--color-bg-card)] border border-[var(--color-border)] rounded-lg px-4 py-3 shadow-lg animate-in fade-in slide-in-from-bottom-2 duration-200"
      role="status"
      aria-live="polite"
    >
      <div className="flex items-center gap-3">
        {/* Spinner */}
        <Loader2
          className="w-4 h-4 animate-spin text-[var(--color-accent)]"
          strokeWidth={2}
          aria-hidden="true"
        />

        {/* Message */}
        <span className="text-sm text-[var(--color-text-primary)]">{latestOp.message}</span>

        {/* Count badge (if multiple) */}
        {count > 1 && (
          <span className="text-xs px-2 py-0.5 bg-[var(--color-bg-hover)] rounded-full text-[var(--color-text-secondary)]">
            {count}
          </span>
        )}
      </div>
    </div>
  );
}
