import { WikiArticleSummary } from '../../stores/wiki';
import { formatRelativeDate } from '../../lib/date';

interface WikiArticleCardProps {
  article: WikiArticleSummary;
  onClick: () => void;
}

export function WikiArticleCard({ article, onClick }: WikiArticleCardProps) {
  const updatedAt = formatRelativeDate(article.updated_at);

  return (
    <div
      onClick={onClick}
      className="group px-4 py-3 hover:bg-[var(--color-bg-card)] cursor-pointer transition-colors"
    >
      <div className="flex items-start justify-between gap-3">
        <div className="flex-1 min-w-0">
          {/* Tag name as title */}
          <h3 className="text-[var(--color-text-primary)] font-medium truncate mb-1">
            {article.tag_name}
          </h3>

          {/* Meta info */}
          <div className="flex items-center gap-3 text-xs text-[var(--color-text-tertiary)]">
            <span>{updatedAt}</span>
            <span className="text-[var(--color-text-secondary)]">
              {article.atom_count} {article.atom_count === 1 ? 'source' : 'sources'}
            </span>
            {article.inbound_links > 0 && (
              <span className="text-[var(--color-accent-light)]">
                {article.inbound_links} {article.inbound_links === 1 ? 'link' : 'links'}
              </span>
            )}
          </div>
        </div>

        {/* Wiki icon indicator */}
        <div className="p-1.5 text-[var(--color-text-tertiary)]">
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 6.253v13m0-13C10.832 5.477 9.246 5 7.5 5S4.168 5.477 3 6.253v13C4.168 18.477 5.754 18 7.5 18s3.332.477 4.5 1.253m0-13C13.168 5.477 14.754 5 16.5 5c1.747 0 3.332.477 4.5 1.253v13C19.832 18.477 18.247 18 16.5 18c-1.746 0-3.332.477-4.5 1.253" />
          </svg>
        </div>
      </div>
    </div>
  );
}
