import { ButtonHTMLAttributes } from 'react';
import { Plus } from 'lucide-react';

interface FABProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  icon?: React.ReactNode;
}

export function FAB({ icon, className = '', ...props }: FABProps) {
  return (
    <button
      className={`fixed bottom-6 right-6 mb-[env(safe-area-inset-bottom)] mr-[env(safe-area-inset-right)] w-14 h-14 bg-[var(--color-accent)] hover:bg-[var(--color-accent-hover)] text-white rounded-full shadow-lg flex items-center justify-center transition-all duration-200 hover:scale-105 focus:outline-none focus:ring-2 focus:ring-[var(--color-accent)] focus:ring-offset-2 focus:ring-offset-[var(--color-bg-main)] ${className}`}
      {...props}
    >
      {icon || (
        <Plus className="w-6 h-6" strokeWidth={2} />
      )}
    </button>
  );
}

