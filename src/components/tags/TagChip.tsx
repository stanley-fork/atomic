import { memo } from 'react';

interface TagChipProps {
  name: string;
  onClick?: (e: React.MouseEvent) => void;
  onRemove?: () => void;
  size?: 'sm' | 'md';
  className?: string;
}

export const TagChip = memo(function TagChip({ name, onClick, onRemove, size = 'sm', className = '' }: TagChipProps) {
  const sizeStyles = {
    sm: 'px-2 py-0.5 text-xs',
    md: 'px-2.5 py-1 text-sm',
  };

  return (
    <span
      className={`inline-flex items-center gap-1 bg-[var(--color-accent)]/20 text-[var(--color-accent-light)] rounded-full shrink-0 max-w-[120px] ${sizeStyles[size]} ${
        onClick ? 'cursor-pointer hover:bg-[var(--color-accent)]/30 transition-colors' : ''
      } ${className}`}
      onClick={onClick}
      title={name}
    >
      <span className="truncate">{name}</span>
      {onRemove && (
        <button
          onClick={(e) => {
            e.stopPropagation();
            onRemove();
          }}
          className="ml-0.5 hover:text-white transition-colors shrink-0"
        >
          <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      )}
    </span>
  );
});

