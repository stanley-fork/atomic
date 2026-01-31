import { useEffect, useRef } from 'react';
import { listen } from '@tauri-apps/api/event';
import { useAtomsStore } from '../stores/atoms';
import { useTagsStore } from '../stores/tags';
import { useUIStore } from '../stores/ui';
import type { AtomWithTags } from '../stores/atoms';

// Embedding complete - fast, no tags (just embedding status update)
interface EmbeddingCompletePayload {
  atom_id: string;
  status: 'complete' | 'failed';
  error?: string;
}

// Tagging complete - slower, has tag info
interface TaggingCompletePayload {
  atom_id: string;
  status: 'complete' | 'failed' | 'skipped';
  error?: string;
  tags_extracted: string[];
  new_tags_created: string[];
}

// Embeddings reset - when provider/model changes and all atoms need re-embedding
interface EmbeddingsResetPayload {
  pending_count: number;
  reason: string;
}

const DEBOUNCE_MS = 2000;
const STATUS_BATCH_MS = 500;

export function useEmbeddingEvents() {
  // Batching refs for embedding status updates
  const pendingStatusUpdates = useRef<Array<{atomId: string, status: string}>>([]);
  const statusBatchTimer = useRef<ReturnType<typeof setTimeout>>();

  // Debounce refs for tag/atom refetches
  const needsAtomRefresh = useRef(false);
  const needsTagRefresh = useRef(false);
  const refetchDebounceTimer = useRef<ReturnType<typeof setTimeout>>();

  // Setup event listeners once on mount
  // Use getState() inside callbacks to get latest store functions
  // This avoids re-registering listeners when store state changes
  useEffect(() => {
    // Listen for atom-created events (from HTTP API / browser extension)
    const unlistenAtomCreated = listen<AtomWithTags>('atom-created', (event) => {
      console.log('Atom created via HTTP API:', event.payload);
      useAtomsStore.getState().addAtom(event.payload);
    });

    // Listen for embedding-complete events (fast, embedding only)
    // Batch these: collect status updates and flush every STATUS_BATCH_MS
    const unlistenEmbeddingComplete = listen<EmbeddingCompletePayload>('embedding-complete', (event) => {
      pendingStatusUpdates.current.push({
        atomId: event.payload.atom_id,
        status: event.payload.status,
      });

      clearTimeout(statusBatchTimer.current);
      statusBatchTimer.current = setTimeout(() => {
        const updates = pendingStatusUpdates.current;
        if (updates.length > 0) {
          pendingStatusUpdates.current = [];
          useAtomsStore.getState().batchUpdateAtomStatuses(updates);
        }
      }, STATUS_BATCH_MS);
    });

    // Listen for tagging-complete events (slower, has tag info)
    // Debounce these: accumulate and do a single refetch after events settle
    const unlistenTaggingComplete = listen<TaggingCompletePayload>('tagging-complete', (event) => {
      // If new tags were created, we need to refresh the tag tree
      if (event.payload.new_tags_created && event.payload.new_tags_created.length > 0) {
        needsTagRefresh.current = true;
      }

      // If tags were extracted, we need to refresh atoms to show updated tags
      if (event.payload.tags_extracted && event.payload.tags_extracted.length > 0) {
        needsAtomRefresh.current = true;
      }

      // Reset debounce timer — wait for events to settle before fetching
      clearTimeout(refetchDebounceTimer.current);
      refetchDebounceTimer.current = setTimeout(() => {
        const { addLoadingOperation, removeLoadingOperation } = useUIStore.getState();

        if (needsAtomRefresh.current) {
          needsAtomRefresh.current = false;
          const opId = `fetch-atoms-${Date.now()}`;
          addLoadingOperation(opId, 'Updating atoms...');
          useAtomsStore.getState().fetchAtoms().finally(() => removeLoadingOperation(opId));
        }

        if (needsTagRefresh.current) {
          needsTagRefresh.current = false;
          const opId = `fetch-tags-${Date.now()}`;
          addLoadingOperation(opId, 'Refreshing tags...');
          useTagsStore.getState().fetchTags().finally(() => removeLoadingOperation(opId));
        }
      }, DEBOUNCE_MS);
    });

    // Listen for embeddings-reset events (provider/model change triggers re-embedding)
    const unlistenEmbeddingsReset = listen<EmbeddingsResetPayload>('embeddings-reset', (event) => {
      console.log('Embeddings reset event:', event.payload);
      const { addLoadingOperation, removeLoadingOperation } = useUIStore.getState();
      // Re-fetch atoms to show updated pending status
      const opId = `fetch-atoms-reset-${Date.now()}`;
      addLoadingOperation(opId, `Re-embedding ${event.payload.pending_count} atoms...`);
      useAtomsStore.getState().fetchAtoms().finally(() => removeLoadingOperation(opId));
    });

    return () => {
      clearTimeout(statusBatchTimer.current);
      clearTimeout(refetchDebounceTimer.current);
      unlistenAtomCreated.then(fn => fn());
      unlistenEmbeddingComplete.then(fn => fn());
      unlistenTaggingComplete.then(fn => fn());
      unlistenEmbeddingsReset.then(fn => fn());
    };
  }, []); // Empty deps - only run once on mount
}
