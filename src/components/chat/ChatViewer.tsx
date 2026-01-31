import { useEffect, useRef } from 'react';
import { useChatStore } from '../../stores/chat';
import { useUIStore } from '../../stores/ui';
import { ConversationsList } from './ConversationsList';
import { ChatView } from './ChatView';

interface ChatViewerProps {
  initialTagId?: string | null;
  initialConversationId?: string | null;
}

export function ChatViewer({ initialTagId, initialConversationId }: ChatViewerProps) {
  const view = useChatStore(s => s.view);
  const showList = useChatStore(s => s.showList);
  const openConversation = useChatStore(s => s.openConversation);
  const openOrCreateForTag = useChatStore(s => s.openOrCreateForTag);
  const reset = useChatStore(s => s.reset);
  const closeDrawer = useUIStore(s => s.closeDrawer);
  const initializedRef = useRef(false);

  // Initialize the chat view based on props - only run once
  useEffect(() => {
    // Prevent double initialization in React Strict Mode
    if (initializedRef.current) return;
    initializedRef.current = true;

    if (initialConversationId) {
      // Open specific conversation
      openConversation(initialConversationId);
    } else if (initialTagId) {
      // Find existing conversation with exactly this tag, or create new one
      openOrCreateForTag(initialTagId);
    } else {
      // Show list with no filter
      showList();
    }
  }, [initialTagId, initialConversationId, showList, openConversation, openOrCreateForTag]);

  // Separate cleanup effect that only runs on unmount
  useEffect(() => {
    return () => {
      reset();
    };
  }, [reset]);

  return (
    <div className="h-full flex flex-col bg-[var(--color-bg-panel)]">
      {/* Header */}
      <div className="flex-shrink-0 flex items-center justify-between px-4 py-3 border-b border-[var(--color-border)]">
        <h2 className="text-lg font-semibold text-[var(--color-text-primary)]">
          {view === 'list' ? 'Conversations' : 'Chat'}
        </h2>
        <button
          onClick={closeDrawer}
          className="p-1 text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] transition-colors"
          aria-label="Close"
        >
          <svg
            className="w-5 h-5"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={2}
              d="M6 18L18 6M6 6l12 12"
            />
          </svg>
        </button>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-hidden">
        {view === 'list' ? <ConversationsList /> : <ChatView />}
      </div>
    </div>
  );
}
