import { useState, useEffect, useRef } from 'react';
import { createPortal } from 'react-dom';
import { Button } from '../ui/Button';
import { useSettingsStore } from '../../stores/settings';

interface SettingsModalProps {
  isOpen: boolean;
  onClose: () => void;
}

export function SettingsModal({ isOpen, onClose }: SettingsModalProps) {
  const { settings, fetchSettings, setSetting, testOpenRouterConnection } = useSettingsStore();
  
  const [apiKey, setApiKey] = useState('');
  const [showApiKey, setShowApiKey] = useState(false);
  const [autoTaggingEnabled, setAutoTaggingEnabled] = useState(true);
  const [isTesting, setIsTesting] = useState(false);
  const [testResult, setTestResult] = useState<'success' | 'error' | null>(null);
  const [testError, setTestError] = useState<string | null>(null);
  const [isSaving, setIsSaving] = useState(false);
  
  const overlayRef = useRef<HTMLDivElement>(null);
  
  useEffect(() => {
    if (isOpen) {
      fetchSettings();
    }
  }, [isOpen, fetchSettings]);
  
  useEffect(() => {
    setApiKey(settings.openrouter_api_key || '');
    setAutoTaggingEnabled(settings.auto_tagging_enabled !== 'false');
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
      <div className="bg-[#252525] rounded-lg shadow-xl border border-[#3d3d3d] w-full max-w-md mx-4 animate-in fade-in zoom-in-95 duration-200">
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
        <div className="px-6 py-4 space-y-6">
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

