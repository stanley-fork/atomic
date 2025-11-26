import { useUIStore } from '../../stores/ui';

export function LoadingIndicator() {
  const { loadingOperations } = useUIStore();

  if (loadingOperations.length === 0) return null;

  // Get most recent operation message
  const latestOp = loadingOperations[loadingOperations.length - 1];
  const count = loadingOperations.length;

  return (
    <div
      className="fixed bottom-5 right-5 z-40 bg-[#2d2d2d] border border-[#3d3d3d] rounded-lg px-4 py-3 shadow-lg animate-in fade-in slide-in-from-bottom-2 duration-200"
      role="status"
      aria-live="polite"
    >
      <div className="flex items-center gap-3">
        {/* Spinner */}
        <svg
          className="w-4 h-4 animate-spin text-[#7c3aed]"
          fill="none"
          viewBox="0 0 24 24"
          aria-hidden="true"
        >
          <circle
            className="opacity-25"
            cx="12"
            cy="12"
            r="10"
            stroke="currentColor"
            strokeWidth="4"
          />
          <path
            className="opacity-75"
            fill="currentColor"
            d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
          />
        </svg>

        {/* Message */}
        <span className="text-sm text-[#dcddde]">{latestOp.message}</span>

        {/* Count badge (if multiple) */}
        {count > 1 && (
          <span className="text-xs px-2 py-0.5 bg-[#3d3d3d] rounded-full text-[#888888]">
            {count}
          </span>
        )}
      </div>
    </div>
  );
}
