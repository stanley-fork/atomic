interface WikiLinkInlineProps {
  tagName: string;
  hasArticle: boolean;
  onClick: () => void;
}

export function WikiLinkInline({ tagName, hasArticle, onClick }: WikiLinkInlineProps) {
  if (hasArticle) {
    return (
      <button
        onClick={onClick}
        className="text-[var(--color-accent)] hover:text-[var(--color-accent-light)] underline decoration-dotted underline-offset-2 cursor-pointer bg-transparent border-none p-0 font-inherit text-inherit"
        title={`Go to article: ${tagName}`}
      >
        {tagName}
      </button>
    );
  }

  return (
    <span
      className="text-[var(--color-text-tertiary)] cursor-default"
      title="Article not yet generated"
    >
      {tagName}
    </span>
  );
}
