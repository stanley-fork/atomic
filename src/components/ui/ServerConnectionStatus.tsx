import { useEffect, useRef, useState } from 'react';
import { toast } from 'sonner';
import { useUIStore } from '../../stores/ui';

// Don't surface transient disconnects. Mobile in particular cycles WS
// connections constantly (backgrounding, signal dips) and surfacing every
// blip is just noise. Only show the indicator once we've been down long
// enough that the user would notice something is broken.
const SHOW_OFFLINE_AFTER_MS = 4000;
// Only celebrate a reconnect if the outage was visible to the user —
// otherwise we're shouting about something they never saw.
const CELEBRATE_RECONNECT_AFTER_MS = 8000;

export function ServerConnectionStatus() {
  const serverConnected = useUIStore(s => s.serverConnected);
  const hasEverConnected = useRef(false);
  const disconnectedAt = useRef<number | null>(null);
  const [showOffline, setShowOffline] = useState(false);

  useEffect(() => {
    if (serverConnected) {
      const downFor = disconnectedAt.current
        ? Date.now() - disconnectedAt.current
        : 0;
      if (hasEverConnected.current && downFor > CELEBRATE_RECONNECT_AFTER_MS) {
        toast.success('Reconnected', { duration: 2500 });
      }
      hasEverConnected.current = true;
      disconnectedAt.current = null;
      setShowOffline(false);
      return;
    }

    // Just went offline (or started offline). Arm a timer; only flip the
    // indicator on if we're still down when it fires.
    if (!hasEverConnected.current) return;
    disconnectedAt.current ??= Date.now();
    const timer = setTimeout(() => setShowOffline(true), SHOW_OFFLINE_AFTER_MS);
    return () => clearTimeout(timer);
  }, [serverConnected]);

  if (!showOffline) return null;

  return (
    <div
      role="status"
      aria-label="Disconnected from server, attempting to reconnect"
      title="Disconnected — attempting to reconnect"
      className="fixed bottom-3 right-3 mb-[env(safe-area-inset-bottom)] mr-[env(safe-area-inset-right)] z-40 flex items-center gap-1.5 px-2 py-1 rounded-full bg-[var(--color-bg-card)]/90 border border-[var(--color-border)] shadow-sm backdrop-blur-sm animate-in fade-in duration-300"
    >
      <span className="relative flex h-2 w-2">
        <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-amber-400 opacity-60" />
        <span className="relative inline-flex rounded-full h-2 w-2 bg-amber-500" />
      </span>
      <span className="text-[11px] font-medium text-[var(--color-text-secondary)]">
        Offline
      </span>
    </div>
  );
}
