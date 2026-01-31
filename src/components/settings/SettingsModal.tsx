import { useState, useEffect, useRef, useCallback } from 'react';
import { createPortal } from 'react-dom';
import { Button } from '../ui/Button';
import { useSettingsStore } from '../../stores/settings';
import { useAtomsStore } from '../../stores/atoms';
import { useTagsStore } from '../../stores/tags';
import { THEMES, Theme } from '../../hooks/useTheme';
import {
  getAvailableLlmModels,
  testOllamaConnection,
  getOllamaModels,
  importObsidianVault,
  getMcpConfig,
  type AvailableModel,
  type OllamaModel,
  type ImportResult,
  type McpConfig
} from '../../lib/tauri';
import { open } from '@tauri-apps/plugin-dialog';

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
        className="w-full px-3 py-2 bg-[var(--color-bg-card)] border border-[var(--color-border)] rounded-md text-[var(--color-text-primary)] text-left text-sm focus:outline-none focus:ring-2 focus:ring-[var(--color-accent)] focus:border-transparent transition-colors duration-150 flex items-center justify-between"
      >
        <span>{selectedOption?.label || value}</span>
        <svg
          className={`w-4 h-4 text-[var(--color-text-secondary)] transition-transform ${isOpen ? 'rotate-180' : ''}`}
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
        </svg>
      </button>

      {isOpen && (
        <div className="absolute z-10 w-full mt-1 bg-[var(--color-bg-card)] border border-[var(--color-border)] rounded-md shadow-lg overflow-hidden">
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
                  ? 'bg-[var(--color-accent)] text-white'
                  : 'text-[var(--color-text-primary)] hover:bg-[var(--color-bg-hover)]'
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
        className="w-full px-3 py-2 bg-[var(--color-bg-card)] border border-[var(--color-border)] rounded-md text-[var(--color-text-primary)] text-left text-sm focus:outline-none focus:ring-2 focus:ring-[var(--color-accent)] focus:border-transparent transition-colors duration-150 flex items-center justify-between"
      >
        <span className={selectedOption ? '' : 'text-[var(--color-text-secondary)]'}>
          {isLoading ? 'Loading models...' : (selectedOption?.name || value || placeholder)}
        </span>
        <svg
          className={`w-4 h-4 text-[var(--color-text-secondary)] transition-transform ${isOpen ? 'rotate-180' : ''}`}
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
        </svg>
      </button>

      {/* Dropdown */}
      {isOpen && (
        <div className="absolute z-10 w-full mt-1 bg-[var(--color-bg-card)] border border-[var(--color-border)] rounded-md shadow-lg overflow-hidden">
          {/* Search input */}
          <div className="p-2 border-b border-[var(--color-border)]">
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
              className="w-full px-2 py-1.5 bg-[var(--color-bg-main)] border border-[var(--color-border)] rounded text-[var(--color-text-primary)] text-sm placeholder-[var(--color-text-secondary)] focus:outline-none focus:ring-1 focus:ring-[var(--color-accent)]"
            />
          </div>

          {/* Options list */}
          <div ref={listRef} className="max-h-60 overflow-y-auto">
            {isLoading ? (
              <div className="px-3 py-4 text-center text-sm text-[var(--color-text-secondary)]">
                <svg className="w-5 h-5 animate-spin mx-auto mb-2" fill="none" viewBox="0 0 24 24">
                  <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                  <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
                </svg>
                Loading models...
              </div>
            ) : filteredOptions.length === 0 ? (
              <div className="px-3 py-4 text-center text-sm text-[var(--color-text-secondary)]">
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
                      ? 'bg-[var(--color-accent)] text-white'
                      : index === highlightedIndex
                      ? 'bg-[var(--color-bg-hover)] text-[var(--color-text-primary)]'
                      : 'text-[var(--color-text-primary)] hover:bg-[var(--color-bg-hover)]'
                  }`}
                >
                  <div className="font-medium">{option.name}</div>
                  <div className={`text-xs ${option.id === value ? 'text-white/70' : 'text-[var(--color-text-secondary)]'}`}>
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

// Connection status indicator component
function ConnectionStatus({ status, error }: { status: 'checking' | 'connected' | 'disconnected'; error?: string }) {
  return (
    <div className="flex items-center gap-2 text-sm">
      {status === 'checking' && (
        <>
          <svg className="w-4 h-4 animate-spin text-[var(--color-text-secondary)]" fill="none" viewBox="0 0 24 24">
            <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
            <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
          </svg>
          <span className="text-[var(--color-text-secondary)]">Checking connection...</span>
        </>
      )}
      {status === 'connected' && (
        <>
          <div className="w-2 h-2 rounded-full bg-green-500" />
          <span className="text-green-500">Connected</span>
        </>
      )}
      {status === 'disconnected' && (
        <>
          <div className="w-2 h-2 rounded-full bg-red-500" />
          <span className="text-red-500">{error || 'Not connected'}</span>
        </>
      )}
    </div>
  );
}

interface SettingsModalProps {
  isOpen: boolean;
  onClose: () => void;
  isSetupMode?: boolean; // When true, modal cannot be closed without valid config
}

export function SettingsModal({ isOpen, onClose, isSetupMode = false }: SettingsModalProps) {
  const settings = useSettingsStore(s => s.settings);
  const fetchSettings = useSettingsStore(s => s.fetchSettings);
  const setSetting = useSettingsStore(s => s.setSetting);
  const testOpenRouterConnection = useSettingsStore(s => s.testOpenRouterConnection);

  // Theme
  const [theme, setTheme] = useState<Theme>('obsidian');

  // Provider selection
  const [provider, setProvider] = useState<'openrouter' | 'ollama'>('openrouter');

  // OpenRouter settings
  const [apiKey, setApiKey] = useState('');
  const [showApiKey, setShowApiKey] = useState(false);
  const [isTesting, setIsTesting] = useState(false);
  const [testResult, setTestResult] = useState<'success' | 'error' | null>(null);
  const [testError, setTestError] = useState<string | null>(null);

  // Ollama settings
  const [ollamaHost, setOllamaHost] = useState('http://127.0.0.1:11434');
  const [ollamaStatus, setOllamaStatus] = useState<'checking' | 'connected' | 'disconnected'>('checking');
  const [ollamaError, setOllamaError] = useState<string | undefined>();
  const [ollamaModels, setOllamaModels] = useState<OllamaModel[]>([]);
  const [ollamaEmbeddingModel, setOllamaEmbeddingModel] = useState('nomic-embed-text');
  const [ollamaLlmModel, setOllamaLlmModel] = useState('llama3.2');
  const [isLoadingOllamaModels, setIsLoadingOllamaModels] = useState(false);

  // Common settings
  const [autoTaggingEnabled, setAutoTaggingEnabled] = useState(true);
  const [embeddingModel, setEmbeddingModel] = useState('openai/text-embedding-3-small');
  const [taggingModel, setTaggingModel] = useState('openai/gpt-4o-mini');
  const [wikiModel, setWikiModel] = useState('anthropic/claude-sonnet-4.5');
  const [chatModel, setChatModel] = useState('anthropic/claude-sonnet-4.5');
  const [isSaving, setIsSaving] = useState(false);
  const [isConnecting, setIsConnecting] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [showAdvanced, setShowAdvanced] = useState(false);

  // OpenRouter model loading
  const [availableModels, setAvailableModels] = useState<AvailableModel[]>([]);
  const [isLoadingModels, setIsLoadingModels] = useState(false);

  // Import state
  const [isImporting, setIsImporting] = useState(false);
  const [importResult, setImportResult] = useState<ImportResult | null>(null);
  const [importError, setImportError] = useState<string | null>(null);

  // MCP setup state
  const [showMcpSetup, setShowMcpSetup] = useState(false);
  const [mcpConfig, setMcpConfig] = useState<McpConfig | null>(null);
  const [mcpConfigCopied, setMcpConfigCopied] = useState(false);

  const overlayRef = useRef<HTMLDivElement>(null);

  // Check Ollama connection
  const checkOllamaConnection = useCallback(async (host: string) => {
    setOllamaStatus('checking');
    setOllamaError(undefined);
    try {
      const connected = await testOllamaConnection(host);
      if (connected) {
        setOllamaStatus('connected');
        // Fetch available models
        setIsLoadingOllamaModels(true);
        const models = await getOllamaModels(host);
        setOllamaModels(models);
        setIsLoadingOllamaModels(false);
      } else {
        setOllamaStatus('disconnected');
        setOllamaError('Could not connect to Ollama');
      }
    } catch (e) {
      setOllamaStatus('disconnected');
      setOllamaError(String(e));
      setIsLoadingOllamaModels(false);
    }
  }, []);

  useEffect(() => {
    if (isOpen) {
      fetchSettings();
      // Fetch OpenRouter models
      setIsLoadingModels(true);
      getAvailableLlmModels()
        .then(models => setAvailableModels(models))
        .catch(err => console.error('Failed to load models:', err))
        .finally(() => setIsLoadingModels(false));
    }
  }, [isOpen, fetchSettings]);

  // Load settings into state
  useEffect(() => {
    const p = settings.provider as 'openrouter' | 'ollama' | undefined;
    setTheme((settings.theme as Theme) || 'obsidian');
    setProvider(p || 'openrouter');
    setApiKey(settings.openrouter_api_key || '');
    setAutoTaggingEnabled(settings.auto_tagging_enabled !== 'false');
    setEmbeddingModel(settings.embedding_model || 'openai/text-embedding-3-small');
    setTaggingModel(settings.tagging_model || 'openai/gpt-4o-mini');
    setWikiModel(settings.wiki_model || 'anthropic/claude-sonnet-4.5');
    setChatModel(settings.chat_model || 'anthropic/claude-sonnet-4.5');
    setOllamaHost(settings.ollama_host || 'http://127.0.0.1:11434');
    setOllamaEmbeddingModel(settings.ollama_embedding_model || 'nomic-embed-text');
    setOllamaLlmModel(settings.ollama_llm_model || 'llama3.2');
  }, [settings]);

  // Check Ollama connection when provider is ollama or host changes
  useEffect(() => {
    if (isOpen && provider === 'ollama') {
      checkOllamaConnection(ollamaHost);
    }
  }, [isOpen, provider, ollamaHost, checkOllamaConnection]);

  useEffect(() => {
    const handleEscape = (e: KeyboardEvent) => {
      // Don't allow escape to close in setup mode
      if (e.key === 'Escape' && !isSetupMode) {
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
  }, [isOpen, onClose, isSetupMode]);

  const handleOverlayClick = (e: React.MouseEvent) => {
    // Don't allow overlay click to close in setup mode
    if (e.target === overlayRef.current && !isSetupMode) {
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

  // Allow save when there's an API key (OpenRouter) or connected (Ollama)
  // Connection test happens automatically on save if not already verified
  const canSave = provider === 'openrouter'
    ? !!apiKey.trim()
    : ollamaStatus === 'connected';

  const handleSave = async () => {
    setSaveError(null);

    // For OpenRouter, test connection if not already verified
    if (provider === 'openrouter' && testResult !== 'success') {
      if (!apiKey.trim()) {
        setSaveError('Please enter an API key');
        return;
      }

      setIsConnecting(true);
      try {
        await testOpenRouterConnection(apiKey);
        setTestResult('success');
      } catch (e) {
        setTestResult('error');
        setTestError(String(e));
        setSaveError('Connection failed. Please check your API key.');
        setIsConnecting(false);
        return;
      }
      setIsConnecting(false);
    }

    // For Ollama, verify connection status
    if (provider === 'ollama' && ollamaStatus !== 'connected') {
      setSaveError('Please connect to Ollama first');
      return;
    }

    setIsSaving(true);
    try {
      // Backend handles dimension change detection and triggers re-embedding automatically
      await setSetting('theme', theme);
      await setSetting('provider', provider);

      if (provider === 'openrouter') {
        await setSetting('openrouter_api_key', apiKey);
        await setSetting('embedding_model', embeddingModel);
        await setSetting('tagging_model', taggingModel);
        await setSetting('wiki_model', wikiModel);
        await setSetting('chat_model', chatModel);
      } else {
        await setSetting('ollama_host', ollamaHost);
        await setSetting('ollama_embedding_model', ollamaEmbeddingModel);
        await setSetting('ollama_llm_model', ollamaLlmModel);
      }

      await setSetting('auto_tagging_enabled', autoTaggingEnabled ? 'true' : 'false');

      onClose();
    } catch (e) {
      console.error('Failed to save settings:', e);
      setSaveError('Failed to save settings');
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

  // Handle MCP setup expand
  const handleMcpExpand = async () => {
    const newState = !showMcpSetup;
    setShowMcpSetup(newState);
    if (newState && !mcpConfig) {
      try {
        const config = await getMcpConfig();
        setMcpConfig(config);
      } catch (e) {
        console.error('Failed to get MCP config:', e);
      }
    }
  };

  // Copy MCP config to clipboard
  const handleCopyMcpConfig = async () => {
    if (!mcpConfig) return;
    try {
      await navigator.clipboard.writeText(JSON.stringify(mcpConfig, null, 2));
      setMcpConfigCopied(true);
      setTimeout(() => setMcpConfigCopied(false), 2000);
    } catch (e) {
      console.error('Failed to copy:', e);
    }
  };

  // Handle Obsidian import
  const fetchAtoms = useAtomsStore((state) => state.fetchAtoms);
  const fetchTags = useTagsStore((state) => state.fetchTags);

  const handleObsidianImport = async () => {
    setImportResult(null);
    setImportError(null);

    try {
      // Open folder picker dialog
      const selected = await open({
        directory: true,
        multiple: false,
        title: 'Select Obsidian Vault',
      });

      if (!selected) {
        return; // User cancelled
      }

      setIsImporting(true);

      const result = await importObsidianVault(selected as string);
      setImportResult(result);

      // Refresh atoms and tags to show imported content
      if (result.imported > 0) {
        await Promise.all([fetchAtoms(), fetchTags()]);
      }
    } catch (e) {
      setImportError(String(e));
    } finally {
      setIsImporting(false);
    }
  };

  // Get Ollama embedding models
  const ollamaEmbeddingModels: AvailableModel[] = ollamaModels
    .filter(m => m.is_embedding)
    .map(m => ({ id: m.id, name: m.name }));

  // Get Ollama LLM models
  const ollamaLlmModels: AvailableModel[] = ollamaModels
    .filter(m => !m.is_embedding)
    .map(m => ({ id: m.id, name: m.name }));

  if (!isOpen) return null;

  return createPortal(
    <div
      ref={overlayRef}
      onClick={handleOverlayClick}
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm"
    >
      <div className="bg-[var(--color-bg-panel)] rounded-lg shadow-xl border border-[var(--color-border)] w-full max-w-md mx-4 max-h-[90vh] flex flex-col animate-in fade-in zoom-in-95 duration-200">
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-[var(--color-border)]">
          <div>
            <h2 className="text-lg font-semibold text-[var(--color-text-primary)]">
              {isSetupMode ? 'Welcome to Atomic' : 'Settings'}
            </h2>
            {isSetupMode && (
              <p className="text-sm text-[var(--color-text-secondary)] mt-1">
                Configure an AI provider to get started
              </p>
            )}
          </div>
          {!isSetupMode && (
            <button
              onClick={onClose}
              className="text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] transition-colors"
            >
              <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          )}
        </div>

        {/* Content */}
        <div className="px-6 py-4 space-y-6 overflow-y-auto flex-1">
          {/* Theme Selector */}
          <div className="space-y-2">
            <label className="block text-sm font-medium text-[var(--color-text-primary)]">
              Theme
            </label>
            <CustomSelect
              value={theme}
              onChange={(v) => setTheme(v as Theme)}
              options={THEMES}
            />
          </div>

          {/* Provider Selector */}
          <div className="space-y-2">
            <label className="block text-sm font-medium text-[var(--color-text-primary)]">
              AI Provider
            </label>
            <p className="text-xs text-[var(--color-text-secondary)]">
              Choose between cloud (OpenRouter) or local (Ollama) AI models
            </p>
            <CustomSelect
              value={provider}
              onChange={(v) => setProvider(v as 'openrouter' | 'ollama')}
              options={[
                { value: 'openrouter', label: 'OpenRouter (Cloud)' },
                { value: 'ollama', label: 'Ollama (Local)' },
              ]}
            />
          </div>

          {/* OpenRouter Settings */}
          {provider === 'openrouter' && (
            <>
              <div className="space-y-2">
                <label className="block text-sm font-medium text-[var(--color-text-primary)]">
                  OpenRouter API Key
                </label>
                <p className="text-xs text-[var(--color-text-secondary)]">
                  Required for AI features. Get your key at openrouter.ai
                </p>
                <div className="flex gap-2">
                  <div className="relative flex-1">
                    <input
                      type={showApiKey ? 'text' : 'password'}
                      value={apiKey}
                      onChange={(e) => handleApiKeyChange(e.target.value)}
                      placeholder="sk-or-..."
                      className="w-full px-3 py-2 pr-10 bg-[var(--color-bg-card)] border border-[var(--color-border)] rounded-md text-[var(--color-text-primary)] placeholder-[var(--color-text-secondary)] focus:outline-none focus:ring-2 focus:ring-[var(--color-accent)] focus:border-transparent transition-colors duration-150"
                    />
                    <button
                      type="button"
                      onClick={() => setShowApiKey(!showApiKey)}
                      className="absolute right-2 top-1/2 -translate-y-1/2 text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] transition-colors"
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

              {/* Model Configuration for OpenRouter */}
              <div className="space-y-3">
                <button
                  type="button"
                  onClick={() => setShowAdvanced(!showAdvanced)}
                  className="flex items-center gap-2 text-sm font-medium text-[var(--color-text-primary)] hover:text-white transition-colors"
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
                  <div className="space-y-4 pl-6 border-l-2 border-[var(--color-border)]">
                    <p className="text-xs text-[var(--color-text-secondary)]">
                      Select models for different AI tasks.
                    </p>

                    {/* Embedding Model */}
                    <div className="space-y-1">
                      <label className="block text-sm font-medium text-[var(--color-text-primary)]">
                        Embedding Model
                      </label>
                      <p className="text-xs text-[var(--color-text-secondary)]">
                        Used for semantic search. Changing this requires re-embedding all atoms.
                      </p>
                      <CustomSelect
                        value={embeddingModel}
                        onChange={setEmbeddingModel}
                        options={[
                          { value: 'openai/text-embedding-3-small', label: 'text-embedding-3-small (1536 dim)' },
                          { value: 'openai/text-embedding-3-large', label: 'text-embedding-3-large (3072 dim)' },
                        ]}
                      />
                    </div>

                    {/* Tagging Model */}
                    <div className="space-y-1">
                      <label className="block text-sm font-medium text-[var(--color-text-primary)]">
                        Tagging Model
                      </label>
                      <p className="text-xs text-[var(--color-text-secondary)]">
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
                      <label className="block text-sm font-medium text-[var(--color-text-primary)]">
                        Wiki Model
                      </label>
                      <p className="text-xs text-[var(--color-text-secondary)]">
                        Used for wiki article generation
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
                      <label className="block text-sm font-medium text-[var(--color-text-primary)]">
                        Chat Model
                      </label>
                      <p className="text-xs text-[var(--color-text-secondary)]">
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
            </>
          )}

          {/* Ollama Settings */}
          {provider === 'ollama' && (
            <>
              <div className="space-y-2">
                <label className="block text-sm font-medium text-[var(--color-text-primary)]">
                  Ollama Server URL
                </label>
                <p className="text-xs text-[var(--color-text-secondary)]">
                  URL of your local Ollama server (default: http://127.0.0.1:11434)
                </p>
                <input
                  type="text"
                  value={ollamaHost}
                  onChange={(e) => setOllamaHost(e.target.value)}
                  placeholder="http://127.0.0.1:11434"
                  className="w-full px-3 py-2 bg-[var(--color-bg-card)] border border-[var(--color-border)] rounded-md text-[var(--color-text-primary)] placeholder-[var(--color-text-secondary)] focus:outline-none focus:ring-2 focus:ring-[var(--color-accent)] focus:border-transparent transition-colors duration-150"
                />
                <ConnectionStatus status={ollamaStatus} error={ollamaError} />
              </div>

              {ollamaStatus === 'connected' && (
                <div className="space-y-4">
                  {/* Ollama Embedding Model */}
                  <div className="space-y-1">
                    <label className="block text-sm font-medium text-[var(--color-text-primary)]">
                      Embedding Model
                    </label>
                    <p className="text-xs text-[var(--color-text-secondary)]">
                      Used for semantic search. Pull nomic-embed-text if not available.
                    </p>
                    {ollamaEmbeddingModels.length > 0 ? (
                      <SearchableSelect
                        value={ollamaEmbeddingModel}
                        onChange={setOllamaEmbeddingModel}
                        options={ollamaEmbeddingModels}
                        isLoading={isLoadingOllamaModels}
                        placeholder="Select embedding model..."
                      />
                    ) : (
                      <div className="px-3 py-2 bg-[var(--color-bg-card)] border border-amber-500/50 rounded-md text-sm text-amber-400">
                        No embedding models found. Run: ollama pull nomic-embed-text
                      </div>
                    )}
                  </div>

                  {/* Ollama LLM Model */}
                  <div className="space-y-1">
                    <label className="block text-sm font-medium text-[var(--color-text-primary)]">
                      LLM Model
                    </label>
                    <p className="text-xs text-[var(--color-text-secondary)]">
                      Used for tagging, wiki generation, and chat
                    </p>
                    {ollamaLlmModels.length > 0 ? (
                      <SearchableSelect
                        value={ollamaLlmModel}
                        onChange={setOllamaLlmModel}
                        options={ollamaLlmModels}
                        isLoading={isLoadingOllamaModels}
                        placeholder="Select LLM model..."
                      />
                    ) : (
                      <div className="px-3 py-2 bg-[var(--color-bg-card)] border border-amber-500/50 rounded-md text-sm text-amber-400">
                        No LLM models found. Run: ollama pull llama3.2
                      </div>
                    )}
                  </div>
                </div>
              )}

              {ollamaStatus === 'disconnected' && (
                <div className="p-4 bg-[var(--color-bg-card)] border border-[var(--color-border)] rounded-md space-y-2">
                  <p className="text-sm text-[var(--color-text-primary)]">Make sure Ollama is running:</p>
                  <ol className="text-xs text-[var(--color-text-secondary)] space-y-1 list-decimal list-inside">
                    <li>Install Ollama from ollama.com</li>
                    <li>Start Ollama (it runs in the background)</li>
                    <li>Pull required models: ollama pull llama3.2 && ollama pull nomic-embed-text</li>
                  </ol>
                  <Button
                    variant="secondary"
                    onClick={() => checkOllamaConnection(ollamaHost)}
                    className="mt-2"
                  >
                    Retry Connection
                  </Button>
                </div>
              )}
            </>
          )}

          {/* Auto-tagging Toggle Section */}
          <div className="flex items-center justify-between">
            <div className="space-y-1">
              <label className="block text-sm font-medium text-[var(--color-text-primary)]">
                Automatic Tag Extraction
              </label>
              <p className="text-xs text-[var(--color-text-secondary)]">
                Automatically suggest tags when creating atoms
              </p>
            </div>
            <button
              type="button"
              role="switch"
              aria-checked={autoTaggingEnabled}
              onClick={() => setAutoTaggingEnabled(!autoTaggingEnabled)}
              className={`relative inline-flex h-6 w-11 flex-shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors duration-200 ease-in-out focus:outline-none focus:ring-2 focus:ring-[var(--color-accent)] focus:ring-offset-2 focus:ring-offset-[var(--color-bg-panel)] ${
                autoTaggingEnabled ? 'bg-[var(--color-accent)]' : 'bg-[var(--color-bg-hover)]'
              }`}
            >
              <span
                className={`pointer-events-none inline-block h-5 w-5 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out ${
                  autoTaggingEnabled ? 'translate-x-5' : 'translate-x-0'
                }`}
              />
            </button>
          </div>

          {/* Import Section */}
          {!isSetupMode && (
            <div className="space-y-3 pt-4 border-t border-[var(--color-border)]">
              <div className="space-y-1">
                <label className="block text-sm font-medium text-[var(--color-text-primary)]">
                  Import Notes
                </label>
                <p className="text-xs text-[var(--color-text-secondary)]">
                  Import notes from other applications
                </p>
              </div>

              <Button
                variant="secondary"
                onClick={handleObsidianImport}
                disabled={isImporting}
                className="w-full justify-center"
              >
                {isImporting ? (
                  <>
                    <svg className="w-4 h-4 animate-spin mr-2" fill="none" viewBox="0 0 24 24">
                      <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                      <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
                    </svg>
                    Importing...
                  </>
                ) : (
                  <>
                    <svg className="w-4 h-4 mr-2" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12" />
                    </svg>
                    Import from Obsidian
                  </>
                )}
              </Button>

              {/* Import Result */}
              {importResult && (
                <div className="p-3 bg-green-500/10 border border-green-500/30 rounded-md text-sm">
                  <div className="text-green-400 font-medium mb-1">Import complete!</div>
                  <div className="text-[var(--color-text-secondary)] space-y-0.5">
                    <div>Imported: {importResult.imported} notes</div>
                    {importResult.tags_created > 0 && (
                      <div>Tags created: {importResult.tags_created}</div>
                    )}
                    {importResult.skipped > 0 && (
                      <div>Skipped: {importResult.skipped} (duplicates/empty)</div>
                    )}
                  </div>
                </div>
              )}

              {/* Import Error */}
              {importError && (
                <div className="p-3 bg-red-500/10 border border-red-500/30 rounded-md text-sm">
                  <div className="text-red-400 font-medium mb-1">Import failed</div>
                  <div className="text-[var(--color-text-secondary)]">{importError}</div>
                </div>
              )}
            </div>
          )}

          {/* MCP Server Setup Section */}
          {!isSetupMode && (
            <div className="space-y-3 pt-4 border-t border-[var(--color-border)]">
              <button
                type="button"
                onClick={handleMcpExpand}
                className="flex items-center gap-2 text-sm font-medium text-[var(--color-text-primary)] hover:text-white transition-colors w-full"
              >
                <svg
                  className={`w-4 h-4 transition-transform ${showMcpSetup ? 'rotate-90' : ''}`}
                  fill="none"
                  stroke="currentColor"
                  viewBox="0 0 24 24"
                >
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
                </svg>
                Claude Desktop Integration
              </button>

              {showMcpSetup && (
                <div className="space-y-4 pl-6 border-l-2 border-[var(--color-border)]">
                  <p className="text-xs text-[var(--color-text-secondary)]">
                    Connect Atomic to Claude Desktop as an MCP server. This allows Claude to search and create notes in your knowledge base.
                  </p>

                  <div className="space-y-2">
                    <div className="text-sm font-medium text-[var(--color-text-primary)]">Setup Instructions</div>
                    <ol className="text-xs text-[var(--color-text-secondary)] space-y-2 list-decimal list-inside">
                      <li>
                        Open Claude Desktop settings
                        <span className="text-[var(--color-text-tertiary)]"> (Claude → Settings → Developer)</span>
                      </li>
                      <li>Click "Edit Config" to open claude_desktop_config.json</li>
                      <li>Add the following configuration:</li>
                    </ol>
                  </div>

                  {/* Config JSON */}
                  <div className="relative">
                    <pre className="p-3 bg-[var(--color-bg-main)] border border-[var(--color-border)] rounded-md text-xs text-[var(--color-text-primary)] overflow-x-auto">
                      {mcpConfig ? JSON.stringify(mcpConfig, null, 2) : 'Loading...'}
                    </pre>
                    <button
                      type="button"
                      onClick={handleCopyMcpConfig}
                      disabled={!mcpConfig}
                      className="absolute top-2 right-2 p-1.5 bg-[var(--color-bg-card)] border border-[var(--color-border)] rounded text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] hover:bg-[var(--color-bg-hover)] transition-colors disabled:opacity-50"
                      title="Copy to clipboard"
                    >
                      {mcpConfigCopied ? (
                        <svg className="w-4 h-4 text-green-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
                        </svg>
                      ) : (
                        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z" />
                        </svg>
                      )}
                    </button>
                  </div>

                  <ol start={4} className="text-xs text-[var(--color-text-secondary)] space-y-2 list-decimal list-inside">
                    <li>Save the config file and restart Claude Desktop</li>
                    <li>
                      Verify by checking for "atomic" in Claude's MCP servers
                      <span className="text-[var(--color-text-tertiary)]"> (hammer icon)</span>
                    </li>
                  </ol>

                  <div className="p-3 bg-green-500/10 border border-green-500/30 rounded-md text-xs text-green-400">
                    <strong>Note:</strong> The MCP server runs independently and connects directly to your Atomic database. Atomic doesn't need to be running for Claude to access your notes.
                  </div>
                </div>
              )}
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="px-6 py-4 border-t border-[var(--color-border)] space-y-3">
          {/* Save Error */}
          {saveError && (
            <div className="flex items-start gap-2 text-sm text-red-500">
              <svg className="w-4 h-4 mt-0.5 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
              </svg>
              <span>{saveError}</span>
            </div>
          )}
          <div className="flex justify-end gap-3">
            {!isSetupMode && (
              <Button variant="secondary" onClick={onClose}>
                Cancel
              </Button>
            )}
            <Button onClick={handleSave} disabled={isSaving || isConnecting || !canSave}>
              {isConnecting ? 'Connecting...' : isSaving ? 'Saving...' : isSetupMode ? 'Get Started' : 'Save'}
            </Button>
          </div>
        </div>
      </div>
    </div>,
    document.body
  );
}
