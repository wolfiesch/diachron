import { cn } from '@/lib/utils';
import {
  FilePlus,
  FileEdit,
  FileX,
  GitCommit,
  Terminal,
  Move,
  Copy,
  HelpCircle,
} from 'lucide-react';
import type { Operation } from '@/types/diachron';

interface OperationIconProps {
  operation: Operation;
  size?: number;
  className?: string;
}

const operationConfig: Record<
  Operation,
  { icon: typeof FilePlus; colorClass: string; label: string }
> = {
  create: {
    icon: FilePlus,
    colorClass: 'text-op-create',
    label: 'Create',
  },
  modify: {
    icon: FileEdit,
    colorClass: 'text-op-modify',
    label: 'Modify',
  },
  delete: {
    icon: FileX,
    colorClass: 'text-op-delete',
    label: 'Delete',
  },
  commit: {
    icon: GitCommit,
    colorClass: 'text-op-commit',
    label: 'Commit',
  },
  execute: {
    icon: Terminal,
    colorClass: 'text-op-execute',
    label: 'Execute',
  },
  move: {
    icon: Move,
    colorClass: 'text-noir-400',
    label: 'Move',
  },
  copy: {
    icon: Copy,
    colorClass: 'text-noir-400',
    label: 'Copy',
  },
  unknown: {
    icon: HelpCircle,
    colorClass: 'text-noir-500',
    label: 'Unknown',
  },
};

export function OperationIcon({ operation, size = 16, className }: OperationIconProps) {
  const config = operationConfig[operation];
  const Icon = config.icon;

  return (
    <Icon
      size={size}
      className={cn(config.colorClass, className)}
      aria-label={config.label}
    />
  );
}

export function OperationBadge({
  operation,
  className,
}: {
  operation: Operation;
  className?: string;
}) {
  return (
    <span
      className={cn(
        'badge inline-flex items-center gap-1.5',
        `badge-${operation}`,
        className
      )}
    >
      <OperationIcon operation={operation} size={12} />
      <span className="capitalize">{operation}</span>
    </span>
  );
}
