import React from 'react';

interface BadgeProps {
  children: React.ReactNode;
  variant?: 'default' | 'success' | 'warning' | 'info' | 'outline';
  className?: string;
}

export const Badge: React.FC<BadgeProps> = ({
  children,
  variant = 'default',
  className = '',
}) => {
  const variantStyles = {
    default: 'bg-zinc-800 text-zinc-300 border-zinc-700',
    success: 'bg-emerald-950/60 text-emerald-400 border-emerald-800/60',
    warning: 'bg-amber-950/60 text-amber-400 border-amber-800/60',
    info: 'bg-cyan-950/60 text-cyan-400 border-cyan-800/60',
    outline: 'bg-transparent text-zinc-400 border-zinc-700',
  };

  return (
    <span
      className={`inline-flex items-center px-2 py-0.5 rounded-full text-xs font-mono border ${variantStyles[variant]} ${className}`}
    >
      {children}
    </span>
  );
};
