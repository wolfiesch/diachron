import { cn } from '@/lib/utils';
import type { ConfidenceLevel } from '@/types/diachron';

interface ConfidenceBadgeProps {
  confidence: ConfidenceLevel;
  showLabel?: boolean;
  size?: 'sm' | 'md';
  className?: string;
}

const confidenceConfig: Record<
  ConfidenceLevel,
  { label: string; className: string; orbClass: string }
> = {
  HIGH: {
    label: 'High',
    className: 'badge-high',
    orbClass: 'glow-orb-high',
  },
  MEDIUM: {
    label: 'Medium',
    className: 'badge-medium',
    orbClass: 'glow-orb-medium',
  },
  LOW: {
    label: 'Low',
    className: 'badge-low',
    orbClass: 'glow-orb-low',
  },
  INFERRED: {
    label: 'Inferred',
    className: 'badge-inferred',
    orbClass: 'glow-orb-inferred',
  },
};

export function ConfidenceBadge({
  confidence,
  showLabel = true,
  size = 'md',
  className,
}: ConfidenceBadgeProps) {
  const config = confidenceConfig[confidence];

  return (
    <span
      className={cn(
        'badge inline-flex items-center gap-1.5',
        config.className,
        size === 'sm' && 'text-2xs px-1.5 py-0.5',
        className
      )}
    >
      <span className={cn('glow-orb', config.orbClass, size === 'sm' && 'w-1.5 h-1.5')} />
      {showLabel && <span>{config.label}</span>}
    </span>
  );
}
