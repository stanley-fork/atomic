import { ButtonHTMLAttributes } from 'react';

interface FABProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  icon?: React.ReactNode;
}

export function FAB({ icon, className = '', ...props }: FABProps) {
  return (
    <button
      className={`fixed bottom-6 right-6 w-14 h-14 bg-[#7c3aed] hover:bg-[#6d28d9] text-white rounded-full shadow-lg flex items-center justify-center transition-all duration-200 hover:scale-105 focus:outline-none focus:ring-2 focus:ring-[#7c3aed] focus:ring-offset-2 focus:ring-offset-[#1e1e1e] ${className}`}
      {...props}
    >
      {icon || (
        <svg className="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
        </svg>
      )}
    </button>
  );
}

