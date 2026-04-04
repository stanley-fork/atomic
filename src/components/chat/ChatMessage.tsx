import { useState, useCallback, Fragment, ReactNode } from 'react';
import { ChatMessageWithContext, ChatCitation } from '../../stores/chat';
import { CitationLink, CitationPopover } from '../wiki';
import { MarkdownImage } from '../ui/MarkdownImage';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';

interface ChatMessageProps {
  message: ChatMessageWithContext;
  isStreaming?: boolean;
  onViewAtom?: (atomId: string) => void;
  searchQuery?: string;
  highlightText?: (text: string) => ReactNode;
}

export function ChatMessage({ message, isStreaming = false, onViewAtom, searchQuery = '', highlightText }: ChatMessageProps) {
  const isUser = message.role === 'user';
  const isAssistant = message.role === 'assistant';

  const [activeCitation, setActiveCitation] = useState<ChatCitation | null>(null);
  const [anchorRect, setAnchorRect] = useState<{ top: number; left: number; bottom: number; width: number } | null>(null);

  // Create a map of citation index to citation object
  const citationMap = new Map(
    (message.citations || []).map((c) => [c.citation_index, c])
  );

  const handleCitationClick = (citation: ChatCitation, element: HTMLElement) => {
    const rect = element.getBoundingClientRect();
    setActiveCitation(citation);
    setAnchorRect({ top: rect.top, left: rect.left, bottom: rect.bottom, width: rect.width });
  };

  const handleClosePopover = () => {
    setActiveCitation(null);
    setAnchorRect(null);
  };

  const handleViewAtom = (atomId: string) => {
    if (onViewAtom) {
      onViewAtom(atomId);
    }
    handleClosePopover();
  };

  // Process text to replace [N] patterns with CitationLink components
  // Returns array of strings and CitationLink elements (strings for highlighting, elements for citations)
  const processTextWithCitations = (text: string): (string | JSX.Element)[] => {
    const parts = text.split(/(\[\d+\])/g);
    return parts.map((part, i) => {
      const match = part.match(/\[(\d+)\]/);
      if (match) {
        const index = parseInt(match[1], 10);
        const citation = citationMap.get(index);
        if (citation) {
          return (
            <CitationLink
              key={`citation-${i}-${index}`}
              index={index}
              onClick={(e) => handleCitationClick(citation, e.currentTarget)}
            />
          );
        }
      }
      // Return raw string so highlighting can be applied
      return part;
    });
  };

  // Process children recursively to handle citations and search highlighting in all text nodes
  const processChildren = useCallback((children: ReactNode): ReactNode => {
    if (typeof children === 'string') {
      // First process citations, then apply highlighting
      const withCitations = processTextWithCitations(children);
      if (searchQuery.trim() && highlightText) {
        // Apply highlighting to string parts, keep citation elements as-is
        return withCitations.map((part, i) => {
          if (typeof part === 'string') {
            return <Fragment key={`hl-${i}`}>{highlightText(part)}</Fragment>;
          }
          // Citation link element - keep as is
          return part;
        });
      }
      // No search - wrap strings in fragments for valid React output
      return withCitations.map((part, i) =>
        typeof part === 'string' ? <Fragment key={`t-${i}`}>{part}</Fragment> : part
      );
    }
    if (Array.isArray(children)) {
      return children.map((child, i) => (
        <Fragment key={i}>{processChildren(child)}</Fragment>
      ));
    }
    return children;
  }, [searchQuery, highlightText]);

  // Custom components for react-markdown with citation processing
  const markdownComponents = {
    p: ({ children }: { children?: ReactNode }) => (
      <p>{processChildren(children)}</p>
    ),
    li: ({ children }: { children?: ReactNode }) => (
      <li>{processChildren(children)}</li>
    ),
    td: ({ children }: { children?: ReactNode }) => (
      <td>{processChildren(children)}</td>
    ),
    th: ({ children }: { children?: ReactNode }) => (
      <th>{processChildren(children)}</th>
    ),
    strong: ({ children }: { children?: ReactNode }) => (
      <strong>{processChildren(children)}</strong>
    ),
    em: ({ children }: { children?: ReactNode }) => (
      <em>{processChildren(children)}</em>
    ),
    del: ({ children }: { children?: ReactNode }) => (
      <del>{processChildren(children)}</del>
    ),
    h1: ({ children }: { children?: ReactNode }) => (
      <h1>{processChildren(children)}</h1>
    ),
    h2: ({ children }: { children?: ReactNode }) => (
      <h2>{processChildren(children)}</h2>
    ),
    h3: ({ children }: { children?: ReactNode }) => (
      <h3>{processChildren(children)}</h3>
    ),
    h4: ({ children }: { children?: ReactNode }) => (
      <h4>{processChildren(children)}</h4>
    ),
    h5: ({ children }: { children?: ReactNode }) => (
      <h5>{processChildren(children)}</h5>
    ),
    h6: ({ children }: { children?: ReactNode }) => (
      <h6>{processChildren(children)}</h6>
    ),
    blockquote: ({ children }: { children?: ReactNode }) => (
      <blockquote>{processChildren(children)}</blockquote>
    ),
    // Style links with search highlighting
    a: ({ children, href }: { children?: ReactNode; href?: string }) => (
      <a
        href={href}
        target="_blank"
        rel="noopener noreferrer"
        className="underline underline-offset-2 decoration-[var(--color-border-hover)] hover:decoration-current transition-colors"
      >
        {processChildren(children)}
      </a>
    ),
    // Style code with search highlighting
    code: ({ className, children }: { className?: string; children?: ReactNode }) => {
      const isBlock = className?.startsWith('language-');
      if (isBlock) {
        return <code className={className}>{processChildren(children)}</code>;
      }
      return (
        <code className="px-1 py-0.5 bg-[var(--color-bg-main)] rounded text-[#e5c07b]">
          {processChildren(children)}
        </code>
      );
    },
    pre: ({ children }: { children?: ReactNode }) => (
      <pre>{children}</pre>
    ),
    img: ({ src, alt }: { src?: string; alt?: string }) => (
      <MarkdownImage src={src} alt={alt} />
    ),
  };

  return (
    <>
      <div className={`flex ${isUser ? 'justify-end' : 'justify-start'}`}>
        <div
          className={`max-w-[85%] rounded-lg px-4 py-3 ${
            isUser
              ? 'bg-[var(--color-accent)] text-white'
              : 'bg-[var(--color-bg-card)] text-[var(--color-text-primary)]'
          }`}
        >
          {/* Message content */}
          {isAssistant ? (
            <div className="prose prose-invert prose-sm max-w-none">
              <ReactMarkdown
                remarkPlugins={[remarkGfm]}
                components={markdownComponents}
              >
                {message.content}
              </ReactMarkdown>
            </div>
          ) : (
            <p className="whitespace-pre-wrap">
              {searchQuery.trim() && highlightText
                ? highlightText(message.content)
                : message.content}
            </p>
          )}

          {/* Streaming indicator */}
          {isStreaming && (
            <span className="inline-block w-2 h-4 ml-1 bg-[var(--color-accent-light)] animate-pulse" />
          )}

          {/* Citations (for assistant messages) */}
          {isAssistant && message.citations && message.citations.length > 0 && (
            <div className="mt-3 pt-3 border-t border-[var(--color-border)]">
              <p className="text-xs text-[var(--color-text-secondary)] mb-2">Sources:</p>
              <div className="flex flex-wrap gap-1">
                {message.citations.map((citation) => (
                  <button
                    key={citation.id}
                    onClick={(e) => handleCitationClick(citation, e.currentTarget)}
                    className="px-2 py-0.5 text-xs rounded bg-[var(--color-bg-hover)] hover:bg-[var(--color-border-hover)] text-[var(--color-accent-light)] transition-colors cursor-pointer"
                    title={citation.excerpt}
                  >
                    [{citation.citation_index}]
                  </button>
                ))}
              </div>
            </div>
          )}

          {/* Tool calls (collapsible) */}
          {isAssistant && message.tool_calls && message.tool_calls.length > 0 && (
            <details className="mt-3 pt-3 border-t border-[var(--color-border)]">
              <summary className="text-xs text-[var(--color-text-secondary)] cursor-pointer hover:text-[var(--color-accent-light)]">
                {message.tool_calls.length} retrieval step{message.tool_calls.length !== 1 ? 's' : ''}
              </summary>
              <div className="mt-2 space-y-2">
                {message.tool_calls.map((toolCall) => (
                  <div
                    key={toolCall.id}
                    className="text-xs p-2 bg-[var(--color-bg-main)] rounded"
                  >
                    <span className="text-[var(--color-accent)]">{toolCall.tool_name}</span>
                    <span className="text-[var(--color-text-tertiary)] ml-2">
                      {toolCall.status === 'complete' ? '✓' : toolCall.status}
                    </span>
                  </div>
                ))}
              </div>
            </details>
          )}
        </div>
      </div>

      {/* Citation popover */}
      {activeCitation && anchorRect && (
        <CitationPopover
          citation={activeCitation}
          anchorRect={anchorRect}
          onClose={handleClosePopover}
          onViewAtom={handleViewAtom}
        />
      )}
    </>
  );
}
