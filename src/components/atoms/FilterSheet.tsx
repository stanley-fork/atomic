import { useEffect } from 'react';
import { X, Check } from 'lucide-react';
import { createPortal } from 'react-dom';
import { useAtomsStore, SourceFilterType, SortField, SortOrder } from '../../stores/atoms';
import { useUIStore, ViewMode, AtomsLayout } from '../../stores/ui';

interface FilterSheetProps {
  isOpen: boolean;
  onClose: () => void;
  displayCount: number;
}

const SORT_OPTIONS: { field: SortField; order: SortOrder; label: string }[] = [
  { field: 'updated', order: 'desc', label: 'Updated (newest)' },
  { field: 'updated', order: 'asc', label: 'Updated (oldest)' },
  { field: 'created', order: 'desc', label: 'Created (newest)' },
  { field: 'created', order: 'asc', label: 'Created (oldest)' },
  { field: 'published', order: 'desc', label: 'Published (newest)' },
  { field: 'published', order: 'asc', label: 'Published (oldest)' },
  { field: 'title', order: 'asc', label: 'Title (A-Z)' },
  { field: 'title', order: 'desc', label: 'Title (Z-A)' },
];

const VIEW_MODES: { id: ViewMode; label: string }[] = [
  { id: 'dashboard', label: 'Dashboard' },
  { id: 'atoms', label: 'Atoms' },
  { id: 'canvas', label: 'Canvas' },
  { id: 'wiki', label: 'Wiki' },
];

const ATOM_LAYOUTS: { id: AtomsLayout; label: string }[] = [
  { id: 'grid', label: 'Grid' },
  { id: 'list', label: 'List' },
];

export function FilterSheet({ isOpen, onClose, displayCount }: FilterSheetProps) {
  const viewMode = useUIStore(s => s.viewMode);
  const setViewMode = useUIStore(s => s.setViewMode);
  const atomsLayout = useUIStore(s => s.atomsLayout);
  const setAtomsLayout = useUIStore(s => s.setAtomsLayout);

  const sourceFilter = useAtomsStore(s => s.sourceFilter);
  const sourceValue = useAtomsStore(s => s.sourceValue);
  const sortBy = useAtomsStore(s => s.sortBy);
  const sortOrder = useAtomsStore(s => s.sortOrder);
  const availableSources = useAtomsStore(s => s.availableSources);
  const setSourceFilter = useAtomsStore(s => s.setSourceFilter);
  const setSourceValue = useAtomsStore(s => s.setSourceValue);
  const setSortBy = useAtomsStore(s => s.setSortBy);
  const setSortOrder = useAtomsStore(s => s.setSortOrder);
  const fetchSources = useAtomsStore(s => s.fetchSources);

  useEffect(() => {
    if (isOpen) fetchSources();
  }, [isOpen, fetchSources]);

  // Lock body scroll while the sheet is open
  useEffect(() => {
    if (!isOpen) return;
    const original = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    return () => { document.body.style.overflow = original; };
  }, [isOpen]);

  const handleSortChange = (field: SortField, order: SortOrder) => {
    setSortBy(field);
    setSortOrder(order);
  };

  return createPortal(
    <>
      {/* Backdrop */}
      <div
        className={`fixed inset-0 bg-black/50 z-40 transition-opacity duration-200 ${
          isOpen ? 'opacity-100' : 'opacity-0 pointer-events-none'
        }`}
        onClick={onClose}
      />

      {/* Sheet */}
      <div
        className={`fixed inset-x-0 bottom-0 z-50 bg-[var(--color-bg-panel)] border-t border-[var(--color-border)] rounded-t-2xl shadow-2xl max-h-[85vh] flex flex-col transition-transform duration-300 ease-out pb-[env(safe-area-inset-bottom)] ${
          isOpen ? 'translate-y-0' : 'translate-y-full'
        }`}
        role="dialog"
        aria-modal="true"
        aria-label="Filter and sort"
      >
        {/* Drag handle */}
        <div className="flex justify-center pt-2 pb-1 shrink-0">
          <div className="w-10 h-1 rounded-full bg-[var(--color-border)]" />
        </div>

        {/* Header */}
        <div className="flex items-center justify-between px-4 py-2 border-b border-[var(--color-border)] shrink-0">
          <div>
            <h2 className="text-base font-semibold text-[var(--color-text-primary)]">
              View & filter
            </h2>
            <p className="text-xs text-[var(--color-text-secondary)]">
              {displayCount} atom{displayCount !== 1 ? 's' : ''}
            </p>
          </div>
          <button
            onClick={onClose}
            className="p-1.5 rounded-md text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] hover:bg-[var(--color-bg-hover)] transition-colors"
            aria-label="Close"
          >
            <X className="w-5 h-5" strokeWidth={2} />
          </button>
        </div>

        {/* Scrollable body */}
        <div className="flex-1 overflow-y-auto px-4 py-4 space-y-6">
          {/* View mode */}
          <section>
            <h3 className="text-xs font-medium uppercase tracking-wider text-[var(--color-text-tertiary)] mb-2">
              View
            </h3>
            <div className="grid grid-cols-2 gap-2">
              {VIEW_MODES.map(vm => (
                <button
                  key={vm.id}
                  onClick={() => setViewMode(vm.id)}
                  className={`py-2 px-3 rounded-md text-sm transition-colors border ${
                    viewMode === vm.id
                      ? 'bg-[var(--color-accent)] text-white border-[var(--color-accent)]'
                      : 'bg-[var(--color-bg-card)] text-[var(--color-text-primary)] border-[var(--color-border)] hover:bg-[var(--color-bg-hover)]'
                  }`}
                >
                  {vm.label}
                </button>
              ))}
            </div>

            {viewMode === 'atoms' && (
              <div className="mt-3">
                <h4 className="text-[11px] font-medium uppercase tracking-wider text-[var(--color-text-tertiary)] mb-2">
                  Layout
                </h4>
                <div className="grid grid-cols-2 gap-2">
                  {ATOM_LAYOUTS.map(l => (
                    <button
                      key={l.id}
                      onClick={() => setAtomsLayout(l.id)}
                      className={`py-2 px-3 rounded-md text-sm transition-colors border ${
                        atomsLayout === l.id
                          ? 'bg-[var(--color-accent)]/15 text-[var(--color-accent-light)] border-[var(--color-accent)]/40'
                          : 'bg-[var(--color-bg-card)] text-[var(--color-text-primary)] border-[var(--color-border)] hover:bg-[var(--color-bg-hover)]'
                      }`}
                    >
                      {l.label}
                    </button>
                  ))}
                </div>
              </div>
            )}
          </section>

          {/* Source filter */}
          <section>
            <h3 className="text-xs font-medium uppercase tracking-wider text-[var(--color-text-tertiary)] mb-2">
              Source
            </h3>
            <div className="flex flex-wrap gap-2">
              {(['all', 'manual', 'external'] as SourceFilterType[]).map(f => (
                <button
                  key={f}
                  onClick={() => { setSourceFilter(f); if (f !== 'external') setSourceValue(null); }}
                  className={`py-1.5 px-3 rounded-full text-sm transition-colors border ${
                    sourceFilter === f && !sourceValue
                      ? 'bg-[var(--color-accent)]/15 text-[var(--color-accent-light)] border-[var(--color-accent)]/40'
                      : 'bg-[var(--color-bg-card)] text-[var(--color-text-primary)] border-[var(--color-border)] hover:bg-[var(--color-bg-hover)]'
                  }`}
                >
                  {f === 'all' ? 'All' : f === 'manual' ? 'Manual' : 'External'}
                </button>
              ))}
            </div>

            {availableSources.length > 0 && (
              <div className="mt-3">
                <div className="text-xs text-[var(--color-text-tertiary)] mb-1.5">
                  Specific source
                </div>
                <div className="flex flex-wrap gap-1.5 max-h-40 overflow-y-auto">
                  {availableSources.map(s => (
                    <button
                      key={s.source}
                      onClick={() => setSourceValue(sourceValue === s.source ? null : s.source)}
                      className={`flex items-center gap-1 text-xs px-2.5 py-1 rounded-full border transition-colors ${
                        sourceValue === s.source
                          ? 'bg-[var(--color-accent)]/15 text-[var(--color-accent-light)] border-[var(--color-accent)]/40'
                          : 'bg-[var(--color-bg-card)] text-[var(--color-text-primary)] border-[var(--color-border)] hover:bg-[var(--color-bg-hover)]'
                      }`}
                    >
                      <span className="truncate max-w-[140px]">{s.source}</span>
                      <span className="text-[var(--color-text-tertiary)]">{s.atom_count}</span>
                    </button>
                  ))}
                </div>
              </div>
            )}
          </section>

          {/* Sort */}
          <section>
            <h3 className="text-xs font-medium uppercase tracking-wider text-[var(--color-text-tertiary)] mb-2">
              Sort by
            </h3>
            <div className="flex flex-col divide-y divide-[var(--color-border)] border border-[var(--color-border)] rounded-md overflow-hidden bg-[var(--color-bg-card)]">
              {SORT_OPTIONS.map(opt => {
                const isActive = sortBy === opt.field && sortOrder === opt.order;
                return (
                  <button
                    key={`${opt.field}-${opt.order}`}
                    onClick={() => handleSortChange(opt.field, opt.order)}
                    className={`flex items-center justify-between px-3 py-2.5 text-sm text-left transition-colors ${
                      isActive
                        ? 'text-[var(--color-accent-light)] bg-[var(--color-accent)]/10'
                        : 'text-[var(--color-text-primary)] hover:bg-[var(--color-bg-hover)]'
                    }`}
                  >
                    <span>{opt.label}</span>
                    {isActive && (
                      <Check className="w-4 h-4" strokeWidth={2} />
                    )}
                  </button>
                );
              })}
            </div>
          </section>
        </div>

        {/* Safe area padding for iOS home bar */}
        <div className="shrink-0" style={{ height: 'env(safe-area-inset-bottom)' }} />
      </div>
    </>,
    document.body,
  );
}
