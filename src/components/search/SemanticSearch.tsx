import { useState, useEffect, useRef } from 'react';
import { useAtomsStore, SearchMode } from '../../stores/atoms';

const SEARCH_MODE_CONFIG: Record<SearchMode, { label: string; placeholder: string; icon: React.ReactNode }> = {
  keyword: {
    label: 'Keyword',
    placeholder: 'Search by keywords...',
    icon: (
      <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 5h12M9 3v2m1.048 9.5A18.022 18.022 0 016.412 9m6.088 9h7M11 21l5-10 5 10M12.751 5C11.783 10.77 8.07 15.61 3 18.129" />
      </svg>
    ),
  },
  semantic: {
    label: 'Semantic',
    placeholder: 'Search by meaning...',
    icon: (
      <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z" />
      </svg>
    ),
  },
  hybrid: {
    label: 'Hybrid',
    placeholder: 'Search by keywords & meaning...',
    icon: (
      <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10" />
      </svg>
    ),
  },
};

export function SemanticSearch() {
  const semanticSearchQuery = useAtomsStore(s => s.semanticSearchQuery);
  const search = useAtomsStore(s => s.search);
  const clearSemanticSearch = useAtomsStore(s => s.clearSemanticSearch);
  const isSearching = useAtomsStore(s => s.isSearching);
  const searchMode = useAtomsStore(s => s.searchMode);
  const setSearchMode = useAtomsStore(s => s.setSearchMode);

  const [inputValue, setInputValue] = useState(semanticSearchQuery);
  const [showModeDropdown, setShowModeDropdown] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const dropdownRef = useRef<HTMLDivElement>(null);

  // Debounce search by 300ms
  useEffect(() => {
    const timer = setTimeout(() => {
      if (inputValue.trim()) {
        search(inputValue);
      } else {
        clearSemanticSearch();
      }
    }, 300);

    return () => clearTimeout(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [inputValue]);

  // Re-search when mode changes (if there's a query)
  useEffect(() => {
    if (inputValue.trim()) {
      search(inputValue);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [searchMode]);

  // Handle keyboard (Escape to clear)
  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      setInputValue('');
      clearSemanticSearch();
      setShowModeDropdown(false);
    }
  };

  // Close dropdown when clicking outside
  useEffect(() => {
    if (!showModeDropdown) return;

    const handleClickOutside = (e: MouseEvent) => {
      if (dropdownRef.current && !dropdownRef.current.contains(e.target as Node)) {
        setShowModeDropdown(false);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, [showModeDropdown]);

  const currentConfig = SEARCH_MODE_CONFIG[searchMode];

  return (
    <div className="relative flex-1 min-w-0" ref={dropdownRef}>
      {/* Search icon button (opens mode dropdown) or loading spinner */}
      {isSearching ? (
        <svg
          className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-[var(--color-text-secondary)] animate-spin"
          fill="none"
          viewBox="0 0 24 24"
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
      ) : (
        <button
          onClick={() => setShowModeDropdown(!showModeDropdown)}
          className="absolute left-2 top-1/2 -translate-y-1/2 p-1 rounded text-[var(--color-text-secondary)] hover:text-[var(--color-accent)] hover:bg-[var(--color-accent)]/10 transition-colors"
          title={`Search mode: ${currentConfig.label} (click to change)`}
        >
          <svg
            className="w-4 h-4"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={2}
              d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
            />
          </svg>
        </button>
      )}

      <input
        ref={inputRef}
        type="text"
        value={inputValue}
        onChange={(e) => setInputValue(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder={currentConfig.placeholder}
        autoComplete="off"
        autoCorrect="off"
        autoCapitalize="off"
        spellCheck={false}
        className="w-full pl-10 pr-10 py-2 bg-[var(--color-bg-card)] border border-[var(--color-border)] rounded-md text-[var(--color-text-primary)] placeholder-[var(--color-text-secondary)] focus:outline-none focus:ring-2 focus:ring-[var(--color-accent)] focus:border-transparent transition-colors text-sm"
      />

      {/* Clear button */}
      {inputValue && (
        <button
          onClick={() => {
            setInputValue('');
            clearSemanticSearch();
            inputRef.current?.focus();
          }}
          className="absolute right-3 top-1/2 -translate-y-1/2 text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] transition-colors"
        >
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={2}
              d="M6 18L18 6M6 6l12 12"
            />
          </svg>
        </button>
      )}

      {/* Search mode dropdown */}
      {showModeDropdown && (
        <div className="absolute top-full left-0 mt-1 bg-[var(--color-bg-card)] border border-[var(--color-border)] rounded-md shadow-lg z-50 min-w-[160px]">
          <div className="px-3 py-1.5 text-xs text-[var(--color-text-secondary)] border-b border-[var(--color-border)]">
            Search Mode
          </div>
          {(Object.keys(SEARCH_MODE_CONFIG) as SearchMode[]).map((mode) => {
            const config = SEARCH_MODE_CONFIG[mode];
            const isActive = mode === searchMode;
            return (
              <button
                key={mode}
                onClick={() => {
                  setSearchMode(mode);
                  setShowModeDropdown(false);
                  inputRef.current?.focus();
                }}
                className={`w-full flex items-center gap-2 px-3 py-2 text-left text-sm transition-colors ${
                  isActive
                    ? 'bg-[var(--color-accent)]/10 text-[var(--color-accent)]'
                    : 'text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-panel)] hover:text-[var(--color-text-primary)]'
                }`}
              >
                {config.icon}
                <span>{config.label}</span>
                {isActive && (
                  <svg className="w-3.5 h-3.5 ml-auto" fill="currentColor" viewBox="0 0 20 20">
                    <path fillRule="evenodd" d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z" clipRule="evenodd" />
                  </svg>
                )}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
