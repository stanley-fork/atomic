import { useState, useRef, useCallback, useEffect } from 'react';
import { useAtomsStore, type AtomWithTags, type Tag } from '../stores/atoms';
import { useTagsStore } from '../stores/tags';
import { useUIStore } from '../stores/ui';

const AUTO_SAVE_DELAY = 1500; // ms

export type SaveStatus = 'idle' | 'saving' | 'saved' | 'error';

interface UseInlineEditorOptions {
  atom: AtomWithTags;
  onAtomUpdated?: (atom: AtomWithTags) => void;
}

interface UseInlineEditorReturn {
  isEditing: boolean;
  isTransitioning: boolean;
  editContent: string;
  editSourceUrl: string;
  editTags: Tag[];
  saveStatus: SaveStatus;
  cursorOffset: number | null;

  startEditing: (cursorOffset?: number) => void;
  stopEditing: () => void;
  setEditContent: (content: string) => void;
  setEditSourceUrl: (url: string) => void;
  setEditTags: (tags: Tag[]) => void;
  saveNow: () => Promise<void>;
}

export function useInlineEditor({
  atom,
  onAtomUpdated,
}: UseInlineEditorOptions): UseInlineEditorReturn {
  const updateAtomContentOnly = useAtomsStore(s => s.updateAtomContentOnly);
  const updateAtom = useAtomsStore(s => s.updateAtom);
  const deleteAtom = useAtomsStore(s => s.deleteAtom);
  const fetchTags = useTagsStore(s => s.fetchTags);

  // Track if the atom was created empty (new atom flow)
  const wasCreatedEmpty = useRef(!atom.content.trim());

  const [isEditing, setIsEditing] = useState(false);
  const [isTransitioning, setIsTransitioning] = useState(false);
  const [editContent, setEditContent] = useState(atom.content);
  const [editSourceUrl, setEditSourceUrl] = useState(atom.source_url || '');
  const [editTags, setEditTags] = useState<Tag[]>(atom.tags);
  const [saveStatus, setSaveStatus] = useState<SaveStatus>('idle');
  const [cursorOffset, setCursorOffset] = useState<number | null>(null);

  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const isSavingRef = useRef(false);
  const isEditingRef = useRef(false);
  const savingPromiseRef = useRef<Promise<void>>(Promise.resolve());
  const needsPipelineRef = useRef(false);
  // Track what was last saved to detect dirty state
  const lastSavedRef = useRef({
    content: atom.content,
    sourceUrl: atom.source_url || '',
    tagIds: atom.tags.map(t => t.id).sort().join(','),
  });

  // Keep refs in sync
  useEffect(() => {
    isEditingRef.current = isEditing;
  }, [isEditing]);

  // Sync from atom prop when not editing (e.g., external updates)
  useEffect(() => {
    if (!isEditingRef.current) {
      setEditContent(atom.content);
      setEditSourceUrl(atom.source_url || '');
      setEditTags(atom.tags);
      lastSavedRef.current = {
        content: atom.content,
        sourceUrl: atom.source_url || '',
        tagIds: atom.tags.map(t => t.id).sort().join(','),
      };
    }
  }, [atom.id, atom.content, atom.source_url, atom.tags]);

  const isDirty = useCallback(() => {
    const currentTagIds = editTags.map(t => t.id).sort().join(',');
    return (
      editContent !== lastSavedRef.current.content ||
      editSourceUrl !== lastSavedRef.current.sourceUrl ||
      currentTagIds !== lastSavedRef.current.tagIds
    );
  }, [editContent, editSourceUrl, editTags]);

  /** Content-only save (no pipeline). */
  const doContentSave = useCallback(async () => {
    if (isSavingRef.current) return;
    isSavingRef.current = true;
    setSaveStatus('saving');
    const promise = (async () => {
      try {
        const tagIds = editTags.map(t => t.id);
        const saved = await updateAtomContentOnly(
          atom.id,
          editContent,
          editSourceUrl || undefined,
          tagIds,
        );
        lastSavedRef.current = {
          content: editContent,
          sourceUrl: editSourceUrl,
          tagIds: tagIds.sort().join(','),
        };
        needsPipelineRef.current = true;
        setSaveStatus('saved');
        onAtomUpdated?.(saved);
      } catch {
        setSaveStatus('error');
      } finally {
        isSavingRef.current = false;
      }
    })();
    savingPromiseRef.current = promise;
    await promise;
  }, [atom.id, editContent, editSourceUrl, editTags, updateAtomContentOnly, onAtomUpdated]);

  /** Full save with pipeline (embedding + tagging). */
  const doFullSave = useCallback(async () => {
    // Wait for any in-flight content-only save to complete first
    await savingPromiseRef.current;
    if (isSavingRef.current) return;
    isSavingRef.current = true;
    setSaveStatus('saving');
    try {
      const tagIds = editTags.map(t => t.id);
      const saved = await updateAtom(
        atom.id,
        editContent,
        editSourceUrl || undefined,
        tagIds,
      );
      lastSavedRef.current = {
        content: editContent,
        sourceUrl: editSourceUrl,
        tagIds: tagIds.sort().join(','),
      };
      needsPipelineRef.current = false;
      setSaveStatus('saved');
      onAtomUpdated?.(saved);
      await fetchTags();
    } catch {
      setSaveStatus('error');
    } finally {
      isSavingRef.current = false;
    }
  }, [atom.id, editContent, editSourceUrl, editTags, updateAtom, fetchTags, onAtomUpdated]);

  /** Schedule a debounced content-only save. */
  const scheduleSave = useCallback(() => {
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(() => {
      debounceRef.current = null;
      doContentSave();
    }, AUTO_SAVE_DELAY);
  }, [doContentSave]);

  /** Wrappers that schedule auto-save on change. */
  const handleSetContent = useCallback((content: string) => {
    setEditContent(content);
    scheduleSave();
  }, [scheduleSave]);

  const handleSetSourceUrl = useCallback((url: string) => {
    setEditSourceUrl(url);
    scheduleSave();
  }, [scheduleSave]);

  const handleSetTags = useCallback((tags: Tag[]) => {
    setEditTags(tags);
    scheduleSave();
  }, [scheduleSave]);

  const startEditing = useCallback((offset?: number) => {
    needsPipelineRef.current = false;
    setIsEditing(true);
    setCursorOffset(offset ?? null);
    setSaveStatus('idle');
  }, []);

  const stopEditing = useCallback(() => {
    // Cancel pending debounced save
    if (debounceRef.current) {
      clearTimeout(debounceRef.current);
      debounceRef.current = null;
    }

    // If atom was created empty and still has no content, delete it and dismiss
    if (wasCreatedEmpty.current && !editContent.trim()) {
      deleteAtom(atom.id).catch(console.error);
      fetchTags().catch(console.error);
      useUIStore.getState().overlayDismiss();
      return;
    }

    // Phase 1: blur the editor content (rendered this frame)
    setIsTransitioning(true);

    // Phase 2: after the blur paints, save + swap to view mode
    requestAnimationFrame(() => {
      const finish = () => {
        setIsEditing(false);
        setCursorOffset(null);
        // Phase 3: after the rendered markdown mounts, clear blur
        requestAnimationFrame(() => setIsTransitioning(false));
      };
      if (isDirty() || needsPipelineRef.current) {
        doFullSave().then(finish, finish);
      } else {
        finish();
      }
    });
  }, [isDirty, doFullSave, editContent, atom.id, deleteAtom, fetchTags]);

  /** Immediate content-only save (for Cmd+S). */
  const saveNow = useCallback(async () => {
    if (debounceRef.current) {
      clearTimeout(debounceRef.current);
      debounceRef.current = null;
    }
    if (isDirty()) {
      await doContentSave();
    }
  }, [isDirty, doContentSave]);

  // Refs for unmount save (closures in cleanup can be stale)
  const editContentRef = useRef(editContent);
  const editSourceUrlRef = useRef(editSourceUrl);
  const editTagsRef = useRef(editTags);
  useEffect(() => { editContentRef.current = editContent; }, [editContent]);
  useEffect(() => { editSourceUrlRef.current = editSourceUrl; }, [editSourceUrl]);
  useEffect(() => { editTagsRef.current = editTags; }, [editTags]);

  // Cleanup on unmount: delete if empty, otherwise save
  useEffect(() => {
    return () => {
      if (debounceRef.current) {
        clearTimeout(debounceRef.current);
      }
      if (isEditingRef.current) {
        const content = editContentRef.current;
        if (wasCreatedEmpty.current && !content.trim()) {
          // Never had content — clean up the empty atom
          useAtomsStore.getState().deleteAtom(atom.id).catch(console.error);
        } else {
          const sourceUrl = editSourceUrlRef.current;
          const tags = editTagsRef.current;
          const tagIds = tags.map(t => t.id);
          useAtomsStore.getState().updateAtom(atom.id, content, sourceUrl || undefined, tagIds).catch(console.error);
        }
      }
    };
  }, [atom.id]);

  // Fade save status back to idle after 2s
  useEffect(() => {
    if (saveStatus === 'saved') {
      const timer = setTimeout(() => setSaveStatus('idle'), 2000);
      return () => clearTimeout(timer);
    }
  }, [saveStatus]);

  return {
    isEditing,
    isTransitioning,
    editContent,
    editSourceUrl,
    editTags,
    saveStatus,
    cursorOffset,
    startEditing,
    stopEditing,
    setEditContent: handleSetContent,
    setEditSourceUrl: handleSetSourceUrl,
    setEditTags: handleSetTags,
    saveNow,
  };
}
