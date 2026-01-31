import { useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { useChatStore, ChatMessageWithContext, RetrievalStep } from '../stores/chat';

interface ChatStreamDelta {
  conversation_id: string;
  content: string;
}

interface ChatToolStart {
  conversation_id: string;
  tool_call_id: string;
  tool_name: string;
  tool_input: unknown;
}

interface ChatToolComplete {
  conversation_id: string;
  tool_call_id: string;
  results_count: number;
}

interface ChatComplete {
  conversation_id: string;
  message: ChatMessageWithContext;
}

interface ChatError {
  conversation_id: string;
  error: string;
}

export function useChatEvents(conversationId: string | null) {
  const appendStreamContent = useChatStore(s => s.appendStreamContent);
  const addRetrievalStep = useChatStore(s => s.addRetrievalStep);
  const completeMessage = useChatStore(s => s.completeMessage);
  const setStreamingError = useChatStore(s => s.setStreamingError);

  useEffect(() => {
    if (!conversationId) return;

    const unlisteners: Array<() => void> = [];

    // Listen for streaming content
    listen<ChatStreamDelta>('chat-stream-delta', (event) => {
      if (event.payload.conversation_id === conversationId) {
        appendStreamContent(event.payload.content);
      }
    }).then((unlisten) => unlisteners.push(unlisten));

    // Listen for tool start
    listen<ChatToolStart>('chat-tool-start', (event) => {
      if (event.payload.conversation_id === conversationId) {
        const step: RetrievalStep = {
          step_number: Date.now(), // Temporary, will be replaced
          tool_name: event.payload.tool_name,
          query: JSON.stringify(event.payload.tool_input),
          results_count: 0,
          timestamp: new Date().toISOString(),
        };
        addRetrievalStep(step);
      }
    }).then((unlisten) => unlisteners.push(unlisten));

    // Listen for tool complete
    listen<ChatToolComplete>('chat-tool-complete', (event) => {
      if (event.payload.conversation_id === conversationId) {
        // Update the last retrieval step with results count
        // For now, this is handled by the store
      }
    }).then((unlisten) => unlisteners.push(unlisten));

    // Listen for completion
    listen<ChatComplete>('chat-complete', (event) => {
      if (event.payload.conversation_id === conversationId) {
        completeMessage(event.payload.message);
      }
    }).then((unlisten) => unlisteners.push(unlisten));

    // Listen for errors
    listen<ChatError>('chat-error', (event) => {
      if (event.payload.conversation_id === conversationId) {
        setStreamingError(event.payload.error);
      }
    }).then((unlisten) => unlisteners.push(unlisten));

    return () => {
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, [conversationId, appendStreamContent, addRetrievalStep, completeMessage, setStreamingError]);
}
