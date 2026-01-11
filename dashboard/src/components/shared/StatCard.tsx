import { cn } from '@/lib/utils';
import { motion } from 'framer-motion';
import type { LucideIcon } from 'lucide-react';

interface StatCardProps {
  title: string;
  value: string | number;
  subtitle?: string;
  icon: LucideIcon;
  iconColor?: string;
  trend?: {
    value: number;
    label: string;
    isPositive?: boolean;
  };
  className?: string;
  delay?: number;
}

export function StatCard({
  title,
  value,
  subtitle,
  icon: Icon,
  iconColor = 'text-accent-primary',
  trend,
  className,
  delay = 0,
}: StatCardProps) {
  return (
    <motion.div
      initial={{ opacity: 0, y: 20 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.4, delay }}
      className={cn('stat-card group', className)}
    >
      <div className="flex items-start justify-between">
        <div className="space-y-1">
          <p className="text-xs font-medium text-noir-500 uppercase tracking-wider">
            {title}
          </p>
          <p className="text-3xl font-display font-semibold text-noir-100 tabular-nums">
            {value}
          </p>
          {subtitle && (
            <p className="text-xs text-noir-500">{subtitle}</p>
          )}
          {trend && (
            <p className={cn(
              'text-xs font-medium',
              trend.isPositive ? 'text-confidence-high' : 'text-confidence-medium'
            )}>
              {trend.isPositive ? '+' : ''}{trend.value}% {trend.label}
            </p>
          )}
        </div>
        <div className={cn(
          'p-3 rounded-lg bg-noir-800/50 transition-all duration-300',
          'group-hover:bg-accent-primary/10 group-hover:shadow-glow-sm',
          iconColor
        )}>
          <Icon size={24} />
        </div>
      </div>
    </motion.div>
  );
}

// Skeleton version for loading state
export function StatCardSkeleton() {
  return (
    <div className="stat-card">
      <div className="flex items-start justify-between">
        <div className="space-y-2">
          <div className="skeleton h-3 w-16" />
          <div className="skeleton h-8 w-24" />
          <div className="skeleton h-3 w-20" />
        </div>
        <div className="skeleton h-12 w-12 rounded-lg" />
      </div>
    </div>
  );
}
