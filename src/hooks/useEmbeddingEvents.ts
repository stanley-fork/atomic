import { useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { useAtomsStore } from '../stores/atoms';
import { useTagsStore } from '../stores/tags';

interface EmbeddingCompletePayload {
  atom_id: string;
  status: 'complete' | 'failed';
  error?: string;
  tags_extracted: string[];
  new_tags_created: string[];
}

export function useEmbeddingEvents() {
  const updateAtomStatus = useAtomsStore((s) => s.updateAtomStatus);
  const fetchTags = useTagsStore((s) => s.fetchTags);
  const fetchAtoms = useAtomsStore((s) => s.fetchAtoms);
  
  useEffect(() => {
    const unlisten = listen<EmbeddingCompletePayload>('embedding-complete', (event) => {
      console.log('Embedding complete event:', event.payload);
      updateAtomStatus(event.payload.atom_id, event.payload.status);
      
      // If new tags were created, refresh the tag tree
      if (event.payload.new_tags_created && event.payload.new_tags_created.length > 0) {
        console.log('New tags created:', event.payload.new_tags_created);
        fetchTags();
      }
      
      // If tags were extracted, refresh atoms to show updated tags
      if (event.payload.tags_extracted && event.payload.tags_extracted.length > 0) {
        console.log('Tags extracted:', event.payload.tags_extracted);
        fetchAtoms();
      }
    });
    
    return () => {
      unlisten.then(fn => fn());
    };
  }, [updateAtomStatus, fetchTags, fetchAtoms]);
}

