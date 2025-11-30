import { useState, useEffect, useRef, useCallback } from 'react';
import { createPortal } from 'react-dom';
import { Button } from '../ui/Button';
import { useSettingsStore } from '../../stores/settings';
import { getAvailableLlmModels, type AvailableModel } from '../../lib/tauri';

interface SelectOption {
  value: string;
  label: string;
}

interface CustomSelectProps {
  value: string;
  onChange: (value: string) => void;
  options: SelectOption[];
}

function CustomSelect({ value, onChange, options }: CustomSelectProps) {
  const [isOpen, setIsOpen] = useState(false);
  const selectRef = useRef<HTMLDivElement>(null);

  const selectedOption = options.find(opt => opt.value === value);

  useEffect(() => {
    const handleClickOutside = (e: MouseEvent) => {
      if (selectRef.current && !selectRef.current.contains(e.target as Node)) {
        setIsOpen(false);
      }
    };

    if (isOpen) {
      document.addEventListener('mousedown', handleClickOutside);
    }

    return () => {
      document.removeEventListener('mousedown', handleClickOutside);
    };
  }, [isOpen]);

  return (
    <div ref={selectRef} className="relative">
      <button
        type="button"
        onClick={() => setIsOpen(!isOpen)}
        className="w-full px-3 py-2 bg-[#2d2d2d] border border-[#3d3d3d] rounded-md text-[#dcddde] text-left text-sm focus:outline-none focus:ring-2 focus:ring-[#7c3aed] focus:border-transparent transition-colors duration-150 flex items-center justify-between"
      >
        <span>{selectedOption?.label || value}</span>
        <svg
          className={`w-4 h-4 text-[#888888] transition-transform ${isOpen ? 'rotate-180' : ''}`}
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
        </svg>
      </button>

      {isOpen && (
        <div className="absolute z-10 w-full mt-1 bg-[#2d2d2d] border border-[#3d3d3d] rounded-md shadow-lg overflow-hidden">
          {options.map((option) => (
            <button
              key={option.value}
              type="button"
              onClick={() => {
                onChange(option.value);
                setIsOpen(false);
              }}
              className={`w-full px-3 py-2 text-left text-sm transition-colors ${
                option.value === value
                  ? 'bg-[#7c3aed] text-white'
                  : 'text-[#dcddde] hover:bg-[#3d3d3d]'
              }`}
            >
              {option.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

// Fuzzy search function - checks if search chars appear in order in target
function fuzzyMatch(search: string, target: string): { match: boolean; score: number } {
  const searchLower = search.toLowerCase();
  const targetLower = target.toLowerCase();

  if (!search) return { match: true, score: 1 };

  // Exact match gets highest score
  if (targetLower.includes(searchLower)) {
    return { match: true, score: 2 + (1 - searchLower.length / targetLower.length) };
  }

  // Fuzzy match - chars must appear in order
  let searchIdx = 0;
  let consecutiveBonus = 0;
  let lastMatchIdx = -2;

  for (let i = 0; i < targetLower.length && searchIdx < searchLower.length; i++) {
    if (targetLower[i] === searchLower[searchIdx]) {
      if (i === lastMatchIdx + 1) consecutiveBonus += 0.1;
      lastMatchIdx = i;
      searchIdx++;
    }
  }

  if (searchIdx === searchLower.length) {
    return { match: true, score: 1 + consecutiveBonus };
  }

  return { match: false, score: 0 };
}

interface SearchableSelectProps {
  value: string;
  onChange: (value: string) => void;
  options: AvailableModel[];
  isLoading?: boolean;
  placeholder?: string;
}

function SearchableSelect({ value, onChange, options, isLoading, placeholder = 'Select...' }: SearchableSelectProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [search, setSearch] = useState('');
  const [highlightedIndex, setHighlightedIndex] = useState(0);
  const selectRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);

  // Find selected option
  const selectedOption = options.find(opt => opt.id === value);

  // Filter and sort options by fuzzy match
  const filteredOptions = options
    .map(opt => ({
      ...opt,
      ...fuzzyMatch(search, `${opt.name} ${opt.id}`)
    }))
    .filter(opt => opt.match)
    .sort((a, b) => b.score - a.score);

  // Reset highlight when filtered options change
  useEffect(() => {
    setHighlightedIndex(0);
  }, [search]);

  // Scroll highlighted item into view
  useEffect(() => {
    if (isOpen && listRef.current) {
      const highlighted = listRef.current.querySelector('[data-highlighted="true"]');
      if (highlighted) {
        highlighted.scrollIntoView({ block: 'nearest' });
      }
    }
  }, [highlightedIndex, isOpen]);

  // Handle click outside
  useEffect(() => {
    const handleClickOutside = (e: MouseEvent) => {
      if (selectRef.current && !selectRef.current.contains(e.target as Node)) {
        setIsOpen(false);
        setSearch('');
      }
    };

    if (isOpen) {
      document.addEventListener('mousedown', handleClickOutside);
    }

    return () => {
      document.removeEventListener('mousedown', handleClickOutside);
    };
  }, [isOpen]);

  // Handle keyboard navigation
  const handleKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (!isOpen) {
      if (e.key === 'Enter' || e.key === ' ' || e.key === 'ArrowDown') {
        e.preventDefault();
        setIsOpen(true);
      }
      return;
    }

    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault();
        setHighlightedIndex(prev => Math.min(prev + 1, filteredOptions.length - 1));
        break;
      case 'ArrowUp':
        e.preventDefault();
        setHighlightedIndex(prev => Math.max(prev - 1, 0));
        break;
      case 'Enter':
        e.preventDefault();
        if (filteredOptions[highlightedIndex]) {
          onChange(filteredOptions[highlightedIndex].id);
          setIsOpen(false);
          setSearch('');
        }
        break;
      case 'Escape':
        e.preventDefault();
        setIsOpen(false);
        setSearch('');
        break;
    }
  }, [isOpen, filteredOptions, highlightedIndex, onChange]);

  const handleOpen = () => {
    setIsOpen(true);
    setSearch('');
    setTimeout(() => inputRef.current?.focus(), 0);
  };

  return (
    <div ref={selectRef} className="relative">
      {/* Selected value / trigger button */}
      <button
        type="button"
        onClick={handleOpen}
        onKeyDown={handleKeyDown}
        className="w-full px-3 py-2 bg-[#2d2d2d] border border-[#3d3d3d] rounded-md text-[#dcddde] text-left text-sm focus:outline-none focus:ring-2 focus:ring-[#7c3aed] focus:border-transparent transition-colors duration-150 flex items-center justify-between"
      >
        <span className={selectedOption ? '' : 'text-[#888888]'}>
          {isLoading ? 'Loading models...' : (selectedOption?.name || value || placeholder)}
        </span>
        <svg
          className={`w-4 h-4 text-[#888888] transition-transform ${isOpen ? 'rotate-180' : ''}`}
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
        </svg>
      </button>

      {/* Dropdown */}
      {isOpen && (
        <div className="absolute z-10 w-full mt-1 bg-[#2d2d2d] border border-[#3d3d3d] rounded-md shadow-lg overflow-hidden">
          {/* Search input */}
          <div className="p-2 border-b border-[#3d3d3d]">
            <input
              ref={inputRef}
              type="text"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="Search models..."
              autoComplete="off"
              autoCorrect="off"
              autoCapitalize="off"
              spellCheck={false}
              className="w-full px-2 py-1.5 bg-[#1e1e1e] border border-[#3d3d3d] rounded text-[#dcddde] text-sm placeholder-[#888888] focus:outline-none focus:ring-1 focus:ring-[#7c3aed]"
            />
          </div>

          {/* Options list */}
          <div ref={listRef} className="max-h-60 overflow-y-auto">
            {isLoading ? (
              <div className="px-3 py-4 text-center text-sm text-[#888888]">
                <svg className="w-5 h-5 animate-spin mx-auto mb-2" fill="none" viewBox="0 0 24 24">
                  <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                  <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
                </svg>
                Loading models...
              </div>
            ) : filteredOptions.length === 0 ? (
              <div className="px-3 py-4 text-center text-sm text-[#888888]">
                No models found
              </div>
            ) : (
              filteredOptions.map((option, index) => (
                <button
                  key={option.id}
                  type="button"
                  data-highlighted={index === highlightedIndex}
                  onClick={() => {
                    onChange(option.id);
                    setIsOpen(false);
                    setSearch('');
                  }}
                  onMouseEnter={() => setHighlightedIndex(index)}
                  className={`w-full px-3 py-2 text-left text-sm transition-colors ${
                    option.id === value
                      ? 'bg-[#7c3aed] text-white'
                      : index === highlightedIndex
                      ? 'bg-[#3d3d3d] text-[#dcddde]'
                      : 'text-[#dcddde] hover:bg-[#3d3d3d]'
                  }`}
                >
                  <div className="font-medium">{option.name}</div>
                  <div className={`text-xs ${option.id === value ? 'text-white/70' : 'text-[#888888]'}`}>
                    {option.id}
                  </div>
                </button>
              ))
            )}
          </div>
        </div>
      )}
    </div>
  );
}

interface SettingsModalProps {
  isOpen: boolean;
  onClose: () => void;
}

export function SettingsModal({ isOpen, onClose }: SettingsModalProps) {
  const { settings, fetchSettings, setSetting, testOpenRouterConnection } = useSettingsStore();

  const [apiKey, setApiKey] = useState('');
  const [showApiKey, setShowApiKey] = useState(false);
  const [autoTaggingEnabled, setAutoTaggingEnabled] = useState(true);
  const [embeddingModel, setEmbeddingModel] = useState('openai/text-embedding-3-small');
  const [taggingModel, setTaggingModel] = useState('openai/gpt-4o-mini');
  const [wikiModel, setWikiModel] = useState('anthropic/claude-sonnet-4.5');
  const [chatModel, setChatModel] = useState('anthropic/claude-sonnet-4.5');
  const [isTesting, setIsTesting] = useState(false);
  const [testResult, setTestResult] = useState<'success' | 'error' | null>(null);
  const [testError, setTestError] = useState<string | null>(null);
  const [isSaving, setIsSaving] = useState(false);
  const [showAdvanced, setShowAdvanced] = useState(false);

  // Dynamic model loading
  const [availableModels, setAvailableModels] = useState<AvailableModel[]>([]);
  const [isLoadingModels, setIsLoadingModels] = useState(false);

  const overlayRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (isOpen) {
      fetchSettings();
      // Fetch available models
      setIsLoadingModels(true);
      getAvailableLlmModels()
        .then(models => setAvailableModels(models))
        .catch(err => console.error('Failed to load models:', err))
        .finally(() => setIsLoadingModels(false));
    }
  }, [isOpen, fetchSettings]);
  
  useEffect(() => {
    setApiKey(settings.openrouter_api_key || '');
    setAutoTaggingEnabled(settings.auto_tagging_enabled !== 'false');
    setEmbeddingModel(settings.embedding_model || 'openai/text-embedding-3-small');
    setTaggingModel(settings.tagging_model || 'openai/gpt-4o-mini');
    setWikiModel(settings.wiki_model || 'anthropic/claude-sonnet-4.5');
    setChatModel(settings.chat_model || 'anthropic/claude-sonnet-4.5');
  }, [settings]);
  
  useEffect(() => {
    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        onClose();
      }
    };

    if (isOpen) {
      document.addEventListener('keydown', handleEscape);
      document.body.style.overflow = 'hidden';
    }

    return () => {
      document.removeEventListener('keydown', handleEscape);
      document.body.style.overflow = '';
    };
  }, [isOpen, onClose]);
  
  const handleOverlayClick = (e: React.MouseEvent) => {
    if (e.target === overlayRef.current) {
      onClose();
    }
  };
  
  const handleTestConnection = async () => {
    if (!apiKey.trim()) return;
    
    setIsTesting(true);
    setTestResult(null);
    setTestError(null);
    
    try {
      await testOpenRouterConnection(apiKey);
      setTestResult('success');
    } catch (e) {
      setTestResult('error');
      setTestError(String(e));
    } finally {
      setIsTesting(false);
    }
  };
  
  const handleSave = async () => {
    setIsSaving(true);
    try {
      await setSetting('openrouter_api_key', apiKey);
      await setSetting('auto_tagging_enabled', autoTaggingEnabled ? 'true' : 'false');
      await setSetting('embedding_model', embeddingModel);
      await setSetting('tagging_model', taggingModel);
      await setSetting('wiki_model', wikiModel);
      await setSetting('chat_model', chatModel);
      onClose();
    } catch (e) {
      console.error('Failed to save settings:', e);
    } finally {
      setIsSaving(false);
    }
  };
  
  // Reset test result when API key changes
  const handleApiKeyChange = (value: string) => {
    setApiKey(value);
    setTestResult(null);
    setTestError(null);
  };
  
  if (!isOpen) return null;
  
  return createPortal(
    <div
      ref={overlayRef}
      onClick={handleOverlayClick}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm"
    >
      <div className="bg-[#252525] rounded-lg shadow-xl border border-[#3d3d3d] w-full max-w-md mx-4 max-h-[90vh] flex flex-col animate-in fade-in zoom-in-95 duration-200">
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-[#3d3d3d]">
          <h2 className="text-lg font-semibold text-[#dcddde]">Settings</h2>
          <button
            onClick={onClose}
            className="text-[#888888] hover:text-[#dcddde] transition-colors"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
        
        {/* Content */}
        <div className="px-6 py-4 space-y-6 overflow-y-auto flex-1">
          {/* OpenRouter API Key Section */}
          <div className="space-y-2">
            <label className="block text-sm font-medium text-[#dcddde]">
              OpenRouter API Key
            </label>
            <p className="text-xs text-[#888888]">
              Required for automatic tag extraction using AI
            </p>
            <div className="flex gap-2">
              <div className="relative flex-1">
                <input
                  type={showApiKey ? 'text' : 'password'}
                  value={apiKey}
                  onChange={(e) => handleApiKeyChange(e.target.value)}
                  placeholder="sk-or-..."
                  className="w-full px-3 py-2 pr-10 bg-[#2d2d2d] border border-[#3d3d3d] rounded-md text-[#dcddde] placeholder-[#888888] focus:outline-none focus:ring-2 focus:ring-[#7c3aed] focus:border-transparent transition-colors duration-150"
                />
                <button
                  type="button"
                  onClick={() => setShowApiKey(!showApiKey)}
                  className="absolute right-2 top-1/2 -translate-y-1/2 text-[#888888] hover:text-[#dcddde] transition-colors"
                >
                  {showApiKey ? (
                    <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13.875 18.825A10.05 10.05 0 0112 19c-4.478 0-8.268-2.943-9.543-7a9.97 9.97 0 011.563-3.029m5.858.908a3 3 0 114.243 4.243M9.878 9.878l4.242 4.242M9.88 9.88l-3.29-3.29m7.532 7.532l3.29 3.29M3 3l3.59 3.59m0 0A9.953 9.953 0 0112 5c4.478 0 8.268 2.943 9.543 7a10.025 10.025 0 01-4.132 5.411m0 0L21 21" />
                    </svg>
                  ) : (
                    <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
                    </svg>
                  )}
                </button>
              </div>
              <Button
                variant="secondary"
                onClick={handleTestConnection}
                disabled={!apiKey.trim() || isTesting}
                className="whitespace-nowrap"
              >
                {isTesting ? (
                  <svg className="w-4 h-4 animate-spin" fill="none" viewBox="0 0 24 24">
                    <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                    <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
                  </svg>
                ) : (
                  'Test'
                )}
              </Button>
            </div>
            
            {/* Test Result */}
            {testResult === 'success' && (
              <div className="flex items-center gap-2 text-sm text-green-500">
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
                </svg>
                Connection successful
              </div>
            )}
            {testResult === 'error' && (
              <div className="flex items-start gap-2 text-sm text-red-500">
                <svg className="w-4 h-4 mt-0.5 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                </svg>
                <span>{testError || 'Connection failed'}</span>
              </div>
            )}
          </div>
          
          {/* Auto-tagging Toggle Section */}
          <div className="flex items-center justify-between">
            <div className="space-y-1">
              <label className="block text-sm font-medium text-[#dcddde]">
                Automatic Tag Extraction
              </label>
              <p className="text-xs text-[#888888]">
                Automatically suggest tags when creating atoms
              </p>
            </div>
            <button
              type="button"
              role="switch"
              aria-checked={autoTaggingEnabled}
              onClick={() => setAutoTaggingEnabled(!autoTaggingEnabled)}
              className={`relative inline-flex h-6 w-11 flex-shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors duration-200 ease-in-out focus:outline-none focus:ring-2 focus:ring-[#7c3aed] focus:ring-offset-2 focus:ring-offset-[#252525] ${
                autoTaggingEnabled ? 'bg-[#7c3aed]' : 'bg-[#3d3d3d]'
              }`}
            >
              <span
                className={`pointer-events-none inline-block h-5 w-5 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out ${
                  autoTaggingEnabled ? 'translate-x-5' : 'translate-x-0'
                }`}
              />
            </button>
          </div>
          
          {/* Model Configuration Section */}
          <div className="space-y-3">
            <button
              type="button"
              onClick={() => setShowAdvanced(!showAdvanced)}
              className="flex items-center gap-2 text-sm font-medium text-[#dcddde] hover:text-white transition-colors"
            >
              <svg
                className={`w-4 h-4 transition-transform ${showAdvanced ? 'rotate-90' : ''}`}
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
              </svg>
              Model Configuration
            </button>

            {showAdvanced && (
              <div className="space-y-4 pl-6 border-l-2 border-[#3d3d3d]">
                <p className="text-xs text-[#888888]">
                  Select models for different AI tasks. Only models supporting structured outputs are available.
                </p>

                {/* Embedding Model */}
                <div className="space-y-1">
                  <label className="block text-sm font-medium text-[#dcddde]">
                    Embedding Model
                  </label>
                  <p className="text-xs text-[#888888]">
                    Used for semantic search. Changing this requires re-embedding all atoms.
                  </p>
                  <CustomSelect
                    value={embeddingModel}
                    onChange={setEmbeddingModel}
                    options={[
                      { value: 'openai/text-embedding-3-small', label: 'openai/text-embedding-3-small (1536 dim)' },
                      { value: 'openai/text-embedding-3-large', label: 'openai/text-embedding-3-large (3072 dim)' },
                    ]}
                  />
                </div>

                {/* Tagging Model */}
                <div className="space-y-1">
                  <label className="block text-sm font-medium text-[#dcddde]">
                    Tagging Model
                  </label>
                  <p className="text-xs text-[#888888]">
                    Used for automatic tag extraction
                  </p>
                  <SearchableSelect
                    value={taggingModel}
                    onChange={setTaggingModel}
                    options={availableModels}
                    isLoading={isLoadingModels}
                    placeholder="Select tagging model..."
                  />
                </div>

                {/* Wiki Model */}
                <div className="space-y-1">
                  <label className="block text-sm font-medium text-[#dcddde]">
                    Wiki Model
                  </label>
                  <p className="text-xs text-[#888888]">
                    Used for wiki article generation and updates
                  </p>
                  <SearchableSelect
                    value={wikiModel}
                    onChange={setWikiModel}
                    options={availableModels}
                    isLoading={isLoadingModels}
                    placeholder="Select wiki model..."
                  />
                </div>

                {/* Chat Model */}
                <div className="space-y-1">
                  <label className="block text-sm font-medium text-[#dcddde]">
                    Chat Model
                  </label>
                  <p className="text-xs text-[#888888]">
                    Used for conversational AI assistant
                  </p>
                  <SearchableSelect
                    value={chatModel}
                    onChange={setChatModel}
                    options={availableModels}
                    isLoading={isLoadingModels}
                    placeholder="Select chat model..."
                  />
                </div>
              </div>
            )}
          </div>
        </div>
        
        {/* Footer */}
        <div className="flex justify-end gap-3 px-6 py-4 border-t border-[#3d3d3d]">
          <Button variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          <Button onClick={handleSave} disabled={isSaving}>
            {isSaving ? 'Saving...' : 'Save'}
          </Button>
        </div>
      </div>
    </div>,
    document.body
  );
}

