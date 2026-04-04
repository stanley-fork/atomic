import { useState, useEffect } from 'react';
import { getTransport } from '../../lib/transport';
import CodeMirror from '@uiw/react-codemirror';
import { markdown } from '@codemirror/lang-markdown';
import { oneDark } from '@codemirror/theme-one-dark';
import { EditorView } from '@codemirror/view';
import { Button } from '../ui/Button';
import { Input } from '../ui/Input';
import { TagSelector } from '../tags/TagSelector';
import { useAtomsStore, AtomWithTags, Tag } from '../../stores/atoms';
import { useTagsStore } from '../../stores/tags';
import { useSettingsStore } from '../../stores/settings';
import { isValidUrl } from '../../lib/markdown';
import { useTheme } from '../../hooks/useTheme';

interface AtomEditorProps {
  atomId: string | null; // null for new atom
  onClose: () => void;
  onSaved?: (atom: AtomWithTags) => void;
}

export function AtomEditor({ atomId, onClose, onSaved }: AtomEditorProps) {
  const createAtom = useAtomsStore(s => s.createAtom);
  const updateAtom = useAtomsStore(s => s.updateAtom);
  const fetchTags = useTagsStore(s => s.fetchTags);
  const settings = useSettingsStore(s => s.settings);
  const fetchSettings = useSettingsStore(s => s.fetchSettings);
  const theme = useTheme();

  const [content, setContent] = useState('');
  const [sourceUrl, setSourceUrl] = useState('');
  const [selectedTags, setSelectedTags] = useState<Tag[]>([]);
  const [isSaving, setIsSaving] = useState(false);
  const [urlError, setUrlError] = useState<string | null>(null);
  const [existingAtom, setExistingAtom] = useState<AtomWithTags | null>(null);
  const [isLoadingAtom, setIsLoadingAtom] = useState(false);

  const isEditing = atomId !== null;
  const autoTaggingEnabled = settings.auto_tagging_enabled !== 'false' && !!settings.openrouter_api_key;

  useEffect(() => {
    fetchSettings();
  }, [fetchSettings]);

  // Fetch existing atom from database when editing
  useEffect(() => {
    if (isEditing && atomId) {
      setIsLoadingAtom(true);
      getTransport().invoke<AtomWithTags | null>('get_atom_by_id', { id: atomId })
        .then((fetchedAtom) => {
          setExistingAtom(fetchedAtom);
          setIsLoadingAtom(false);
        })
        .catch((error) => {
          console.error('Failed to fetch atom:', error);
          setExistingAtom(null);
          setIsLoadingAtom(false);
        });
    } else {
      setExistingAtom(null);
    }
  }, [isEditing, atomId]);

  useEffect(() => {
    if (existingAtom) {
      setContent(existingAtom.content);
      setSourceUrl(existingAtom.source_url || '');
      setSelectedTags(existingAtom.tags);
    }
  }, [existingAtom]);

  const handleSourceUrlChange = (value: string) => {
    setSourceUrl(value);
    if (value && !isValidUrl(value)) {
      setUrlError('Please enter a valid URL');
    } else {
      setUrlError(null);
    }
  };

  const handleSave = async () => {
    if (!content.trim() || isSaving) return;
    if (sourceUrl && !isValidUrl(sourceUrl)) return;

    setIsSaving(true);
    try {
      const tagIds = selectedTags.map((t) => t.id);
      const savedAtom = isEditing
        ? await updateAtom(atomId!, content, sourceUrl || undefined, tagIds)
        : await createAtom(content, sourceUrl || undefined, tagIds);
      
      // Refresh tags to update counts
      await fetchTags();
      
      onSaved?.(savedAtom);
      onClose();
    } catch (error) {
      console.error('Failed to save atom:', error);
    } finally {
      setIsSaving(false);
    }
  };

  // Show loading state when fetching atom for editing
  if (isEditing && isLoadingAtom) {
    return (
      <div className="flex items-center justify-center h-full p-4 text-[var(--color-text-secondary)]">
        Loading atom...
      </div>
    );
  }

  const canSave = content.trim().length > 0 && !urlError;

  // Custom theme extension for CodeMirror
  const customTheme = EditorView.theme({
    '&': {
      backgroundColor: 'var(--color-bg-card)',
      height: '100%',
    },
    '.cm-gutters': {
      backgroundColor: 'var(--color-bg-card)',
      borderRight: '1px solid var(--color-border)',
    },
    '.cm-activeLineGutter': {
      backgroundColor: 'var(--color-bg-hover)',
    },
    '.cm-activeLine': {
      backgroundColor: 'var(--color-bg-hover)',
    },
    '.cm-scroller': {
      fontFamily: 'var(--font-mono)',
    },
  });

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center justify-between px-6 py-4 border-b border-[var(--color-border)]">
        <h2 className="text-lg font-semibold text-[var(--color-text-primary)]">
          {isEditing ? 'Edit Atom' : 'New Atom'}
        </h2>
        <button
          onClick={onClose}
          className="text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] transition-colors"
        >
          <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>

      {/* Editor */}
      <div className="flex-1 overflow-hidden">
        <CodeMirror
          value={content}
          onChange={setContent}
          extensions={[markdown(), customTheme, EditorView.lineWrapping]}
          theme={theme === 'obsidian' ? oneDark : undefined}
          placeholder="Write your note in Markdown..."
          className="h-full"
          basicSetup={{
            lineNumbers: true,
            highlightActiveLineGutter: true,
            highlightActiveLine: true,
            foldGutter: true,
          }}
        />
      </div>

      {/* Form fields */}
      <div className="px-6 py-4 space-y-4 border-t border-[var(--color-border)]">
        <Input
          label="Source URL (optional)"
          value={sourceUrl}
          onChange={(e) => handleSourceUrlChange(e.target.value)}
          placeholder="https://example.com/article"
          error={urlError || undefined}
        />
        <TagSelector selectedTags={selectedTags} onTagsChange={setSelectedTags} />
        {autoTaggingEnabled && (
          <p className="text-xs text-[var(--color-text-secondary)] mt-1">
            <span className="inline-flex items-center gap-1">
              <svg className="w-3 h-3 text-[var(--color-accent)]" fill="currentColor" viewBox="0 0 20 20">
                <path fillRule="evenodd" d="M11.3 1.046A1 1 0 0112 2v5h4a1 1 0 01.82 1.573l-7 10A1 1 0 018 18v-5H4a1 1 0 01-.82-1.573l7-10a1 1 0 011.12-.38z" clipRule="evenodd" />
              </svg>
              Tags will be extracted automatically
            </span>
          </p>
        )}
      </div>

      {/* Footer */}
      <div className="flex justify-end gap-3 px-6 py-4 border-t border-[var(--color-border)]">
        <Button variant="secondary" onClick={onClose}>
          Cancel
        </Button>
        <Button onClick={handleSave} disabled={!canSave || isSaving}>
          {isSaving ? 'Saving...' : 'Save'}
        </Button>
      </div>
    </div>
  );
}

