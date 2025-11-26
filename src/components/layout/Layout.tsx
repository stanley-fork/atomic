import { useEffect } from 'react';
import { LeftPanel } from './LeftPanel';
import { MainView } from './MainView';
import { RightDrawer } from './RightDrawer';
import { LoadingIndicator } from '../ui/LoadingIndicator';
import { useAtomsStore } from '../../stores/atoms';
import { useTagsStore } from '../../stores/tags';
import { processPendingEmbeddings } from '../../lib/tauri';

export function Layout() {
  const { fetchAtoms } = useAtomsStore();
  const { fetchTags } = useTagsStore();

  // Fetch initial data and process pending embeddings
  useEffect(() => {
    const initializeApp = async () => {
      // Fetch initial data first
      await Promise.all([fetchAtoms(), fetchTags()]);

      // Process any pending embeddings in the background
      try {
        const count = await processPendingEmbeddings();
        if (count > 0) {
          console.log(`Processing ${count} pending embeddings in background...`);
          console.log(`Embeddings use local AI (fast). Tag extraction uses API (may be rate-limited).`);

          if (count > 100) {
            console.warn(
              `Large batch detected. Processing ${count} atoms may take 10-30 minutes. ` +
              `Watch for amber indicators on atoms - they'll update as processing completes.`
            );
          }
        }
      } catch (error) {
        console.error('Failed to start pending embeddings:', error);
        // Don't block app startup on embedding failure
      }
    };

    initializeApp();
  }, [fetchAtoms, fetchTags]);

  return (
    <div className="flex h-screen overflow-hidden bg-[#1e1e1e]">
      <LeftPanel />
      <MainView />
      <RightDrawer />
      <LoadingIndicator />
    </div>
  );
}

