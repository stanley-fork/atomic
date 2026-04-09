import { useRef, useEffect, useState } from 'react';
import { toast } from 'sonner';
import { getTransport } from '../../lib/transport';
import { isTauri } from '../../lib/platform';
import { AtomViewer } from '../atoms/AtomViewer';
import { WikiViewer } from '../wiki/WikiViewer';
import { WikiListViewer } from '../wiki/WikiListViewer';
import { useUIStore } from '../../stores/ui';
import { useAtomsStore, type AtomWithTags } from '../../stores/atoms';
import { useClickOutside } from '../../hooks/useClickOutside';
import { useKeyboard } from '../../hooks/useKeyboard';

// Benchmarking helper
const PERF_DEBUG = true;
const perfLog = (label: string, startTime?: number) => {
  if (!PERF_DEBUG) return;
  if (startTime !== undefined) {
    console.log(`[RightDrawer] ${label}: ${(performance.now() - startTime).toFixed(2)}ms`);
  } else {
    console.log(`[RightDrawer] ${label}`);
  }
};

export function RightDrawer() {
  const drawerState = useUIStore(s => s.drawerState);
  const closeDrawer = useUIStore(s => s.closeDrawer);
  // openDrawer removed — editor mode replaced by inline editing in AtomReader
  const drawerRef = useRef<HTMLDivElement>(null);
  const openTimeRef = useRef<number | null>(null);

  const { isOpen, mode, atomId, tagId, tagName, highlightText } = drawerState;

  const [atom, setAtom] = useState<AtomWithTags | null>(null);
  const [isLoadingAtom, setIsLoadingAtom] = useState(false);

  // Watch the atoms store for updates to the currently viewed atom
  const storeAtom = useAtomsStore((s) =>
    atomId ? s.atoms.find((a) => a.id === atomId) : undefined
  );

  // Track drawer open/close timing
  const closeStartRef = useRef<number | null>(null);
  useEffect(() => {
    if (isOpen) {
      openTimeRef.current = performance.now();
      perfLog(`Drawer OPENING (mode=${mode}, atomId=${atomId?.slice(0, 8)}...)`);
    } else if (openTimeRef.current !== null) {
      perfLog('Drawer CLOSED, total open duration', openTimeRef.current);
      openTimeRef.current = null;
    }
  }, [isOpen, mode, atomId]);

  // Track when isOpen changes to false (close initiated)
  useEffect(() => {
    if (!isOpen && closeStartRef.current === null && drawerState.mode) {
      closeStartRef.current = performance.now();
      perfLog('Close INITIATED - starting render cycle');
    } else if (isOpen) {
      closeStartRef.current = null;
    }
  });

  // Fetch atom from database when viewing
  useEffect(() => {
    if (mode === 'viewer' && atomId) {
      const fetchStart = performance.now();
      perfLog('Atom fetch START');
      setIsLoadingAtom(true);
      getTransport().invoke<AtomWithTags | null>('get_atom_by_id', { id: atomId })
        .then((fetchedAtom) => {
          perfLog('Atom fetch COMPLETE', fetchStart);
          if (fetchedAtom) {
            perfLog(`  Content length: ${fetchedAtom.content.length} chars`);
            perfLog(`  Tags: ${fetchedAtom.tags.length}`);
          }
          setAtom(fetchedAtom);
          setIsLoadingAtom(false);
        })
        .catch((error) => {
          console.error('Failed to fetch atom:', error);
          perfLog('Atom fetch FAILED', fetchStart);
          setAtom(null);
          setIsLoadingAtom(false);
        });
    } else {
      setAtom(null);
    }
  }, [mode, atomId]);

  // Re-fetch full atom when the store summary changes (e.g., after tag extraction)
  const storeAtomUpdatedAt = storeAtom?.updated_at;
  useEffect(() => {
    if (mode === 'viewer' && atomId && storeAtomUpdatedAt && !isLoadingAtom) {
      // Store has summaries now, so re-fetch the full atom to get updated tags/content
      getTransport().invoke<AtomWithTags | null>('get_atom_by_id', { id: atomId })
        .then((fetchedAtom) => {
          if (fetchedAtom) setAtom(fetchedAtom);
        })
        .catch((e) => { console.error('Failed to refresh atom:', e); toast.error('Failed to refresh atom', { id: 'atom-refresh-error', description: String(e) }); });
    }
  }, [mode, atomId, storeAtomUpdatedAt, isLoadingAtom]);

  // Close on click outside
  useClickOutside(drawerRef, closeDrawer, isOpen);

  // Close on Escape key
  useKeyboard('Escape', closeDrawer, isOpen);

  // Prevent body scroll when drawer is open
  useEffect(() => {
    if (isOpen) {
      document.body.style.overflow = 'hidden';
    } else {
      document.body.style.overflow = '';
    }
    return () => {
      document.body.style.overflow = '';
    };
  }, [isOpen]);

  const renderContent = () => {
    const renderStart = performance.now();
    let result: React.ReactNode = null;
    let contentType = 'unknown';

    switch (mode) {
      case 'editor':
      case 'viewer':
        // Don't render heavy content when drawer is closing - allows smooth animation
        if (!isOpen) {
          contentType = 'viewer-closing';
          result = null;
          break;
        }
        if (isLoadingAtom) {
          contentType = 'viewer-loading';
          result = (
            <div className="flex items-center justify-center h-full text-[var(--color-text-secondary)]">
              Loading...
            </div>
          );
          break;
        }
        if (!atom) {
          contentType = 'viewer-not-found';
          result = (
            <div className="flex items-center justify-center h-full text-[var(--color-text-secondary)]">
              Atom not found
            </div>
          );
          break;
        }
        contentType = 'viewer-atom';
        result = <AtomViewer atom={atom} onClose={closeDrawer} onEdit={() => {}} highlightText={highlightText} />;
        break;
      case 'wiki':
        if (!isOpen) {
          contentType = 'wiki-closing';
          result = null;
          break;
        }
        // If no tagId, show wiki list view; otherwise show specific wiki article
        if (!tagId) {
          contentType = 'wiki-list';
          result = <WikiListViewer />;
          break;
        }
        if (!tagName) {
          contentType = 'wiki-no-tag';
          result = (
            <div className="flex items-center justify-center h-full text-[var(--color-text-secondary)]">
              No tag selected
            </div>
          );
          break;
        }
        contentType = 'wiki';
        result = <WikiViewer tagId={tagId} tagName={tagName} />;
        break;
      default:
        contentType = 'null';
        result = null;
    }

    perfLog(`renderContent (${contentType}) JSX creation`, renderStart);
    return result;
  };

  return (
    <>
      {/* Backdrop */}
      <div
        className={`fixed inset-0 bg-black/50 z-40 transition-opacity duration-200 ${
          isOpen ? 'opacity-100' : 'opacity-0 pointer-events-none'
        }`}
      />

      {/* Drawer */}
      <div
        ref={drawerRef}
        className={`fixed top-0 right-0 h-full w-full md:w-[75vw] md:min-w-[600px] md:max-w-[1200px] bg-[var(--color-bg-panel)] border-l border-[var(--color-border)] shadow-2xl z-50 transition-transform duration-200 ease-out ${isTauri() ? 'pt-[28px]' : ''} ${
          isOpen ? 'translate-x-0' : 'translate-x-full'
        }`}
        style={{ backdropFilter: 'blur(var(--backdrop-blur))' }}
      >
        {renderContent()}
      </div>
    </>
  );
}

