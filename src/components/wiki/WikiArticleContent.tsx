import { useState, Fragment, ReactNode } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { WikiArticle, WikiCitation } from '../../stores/wiki';
import { CitationLink } from './CitationLink';
import { CitationPopover } from './CitationPopover';

interface WikiArticleContentProps {
  article: WikiArticle;
  citations: WikiCitation[];
  onViewAtom: (atomId: string) => void;
}

export function WikiArticleContent({ article, citations, onViewAtom }: WikiArticleContentProps) {
  const [activeCitation, setActiveCitation] = useState<WikiCitation | null>(null);
  const [anchorEl, setAnchorEl] = useState<HTMLElement | null>(null);

  // Create a map of citation index to citation object
  const citationMap = new Map(citations.map(c => [c.citation_index, c]));

  const handleCitationClick = (citation: WikiCitation, element: HTMLElement) => {
    setActiveCitation(citation);
    setAnchorEl(element);
  };

  const handleClosePopover = () => {
    setActiveCitation(null);
    setAnchorEl(null);
  };

  // Process text to replace [N] patterns with CitationLink components
  const processTextWithCitations = (text: string): ReactNode[] => {
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
      return <Fragment key={`text-${i}`}>{part}</Fragment>;
    });
  };

  // Process children recursively to handle citations in all text nodes
  const processChildren = (children: ReactNode): ReactNode => {
    if (typeof children === 'string') {
      return processTextWithCitations(children);
    }
    if (Array.isArray(children)) {
      return children.map((child, i) => (
        <Fragment key={i}>{processChildren(child)}</Fragment>
      ));
    }
    return children;
  };

  // Custom components for react-markdown
  const components = {
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
  };

  return (
    <>
      <div className="wiki-content prose prose-invert prose-sm max-w-none px-6 py-4 prose-headings:text-[#dcddde] prose-p:text-[#dcddde] prose-a:text-[#7c3aed] prose-strong:text-[#dcddde] prose-code:text-[#a78bfa] prose-code:bg-[#2d2d2d] prose-code:px-1 prose-code:py-0.5 prose-code:rounded prose-pre:bg-[#2d2d2d] prose-pre:border prose-pre:border-[#3d3d3d] prose-blockquote:border-l-[#7c3aed] prose-blockquote:text-[#888888] prose-li:text-[#dcddde] prose-hr:border-[#3d3d3d]">
        <ReactMarkdown remarkPlugins={[remarkGfm]} components={components}>
          {article.content}
        </ReactMarkdown>
      </div>

      {/* Citation popover */}
      {activeCitation && anchorEl && (
        <CitationPopover
          citation={activeCitation}
          anchorEl={anchorEl}
          onClose={handleClosePopover}
          onViewAtom={onViewAtom}
        />
      )}
    </>
  );
}

