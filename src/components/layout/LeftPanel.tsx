import { useState, useRef, useEffect } from 'react';
import { TagTree } from '../tags/TagTree';
import { SettingsButton, SettingsModal, type SettingsTab } from '../settings';
import { DatabaseSwitcher } from '../DatabaseSwitcher';
import { useUIStore } from '../../stores/ui';
import { isTauri } from '../../lib/platform';

const COLLAPSE_BREAKPOINT = 768;

export function LeftPanel() {
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [settingsInitialTab, setSettingsInitialTab] = useState<SettingsTab | undefined>(undefined);
  const leftPanelOpen = useUIStore(s => s.leftPanelOpen);
  const setLeftPanelOpen = useUIStore(s => s.setLeftPanelOpen);
  const panelRef = useRef<HTMLDivElement>(null);

  // Auto-collapse on small screens, auto-expand on large screens. When an
  // overlay is active (reader, wiki, graph) we leave the panel alone — the
  // overlay flow has already decided the panel should be hidden, and we
  // don't want to clobber that on mount or on a resize event.
  useEffect(() => {
    const mq = window.matchMedia(`(max-width: ${COLLAPSE_BREAKPOINT}px)`);
    const handleChange = (e: MediaQueryListEvent | MediaQueryList) => {
      if (useUIStore.getState().overlayNav.index !== -1) return;
      setLeftPanelOpen(!e.matches);
    };
    handleChange(mq);
    mq.addEventListener('change', handleChange);
    return () => mq.removeEventListener('change', handleChange);
  }, [setLeftPanelOpen]);

  // Close panel on outside click when in overlay mode (small screens)
  useEffect(() => {
    if (!leftPanelOpen) return;
    const mq = window.matchMedia(`(max-width: ${COLLAPSE_BREAKPOINT}px)`);
    if (!mq.matches) return;

    const handleClick = (e: MouseEvent) => {
      if (panelRef.current && !panelRef.current.contains(e.target as Node)) {
        setLeftPanelOpen(false);
      }
    };
    document.addEventListener('mousedown', handleClick);
    return () => document.removeEventListener('mousedown', handleClick);
  }, [leftPanelOpen, setLeftPanelOpen]);

  return (
    <>
      {/* Backdrop for overlay mode on small screens */}
      <div
        className={`fixed inset-0 bg-black/40 z-30 md:hidden transition-opacity duration-200 ${
          leftPanelOpen ? 'opacity-100' : 'opacity-0 pointer-events-none'
        }`}
      />

      {/* Panel */}
      <aside
        ref={panelRef}
        className={`
          h-full bg-[var(--color-bg-panel)]/80 border-r border-[var(--color-border)] flex flex-col transition-all duration-300 ease-in-out backdrop-blur-xl z-10 overflow-hidden flex-shrink-0
          max-md:fixed max-md:top-0 max-md:left-0 max-md:z-40 max-md:shadow-2xl max-md:w-[250px]
          max-md:pt-[env(safe-area-inset-top)] max-md:pb-[env(safe-area-inset-bottom)] max-md:pl-[env(safe-area-inset-left)]
          ${leftPanelOpen ? 'max-md:translate-x-0' : 'max-md:-translate-x-full'}
          ${leftPanelOpen ? 'md:w-[250px] md:border-r' : 'md:w-0 md:border-r-0'}
        `}
      >
        <div className="w-[250px] h-full flex flex-col">
          {/* Titlebar row with settings button */}
          <div className={`h-[52px] flex items-center px-3 flex-shrink-0 gap-1 ${isTauri() ? 'pl-[78px]' : ''}`} data-tauri-drag-region>
            <DatabaseSwitcher />
            <SettingsButton onClick={() => { setSettingsInitialTab(undefined); setIsSettingsOpen(true); }} />
          </div>

          {/* Tag Tree with integrated search */}
          <div className="flex-1 overflow-hidden">
            <TagTree
              onOpenTagSettings={() => {
                setSettingsInitialTab('tag-categories');
                setIsSettingsOpen(true);
              }}
            />
          </div>
        </div>

        <SettingsModal
          isOpen={isSettingsOpen}
          onClose={() => setIsSettingsOpen(false)}
          initialTab={settingsInitialTab}
        />
      </aside>
    </>
  );
}
