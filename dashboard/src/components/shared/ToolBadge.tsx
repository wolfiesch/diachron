import { cn } from '@/lib/utils';
import { Bot, Sparkles, Terminal, FileEdit, Cpu } from 'lucide-react';

interface ToolBadgeProps {
  tool: string;
  showIcon?: boolean;
  className?: string;
}

function getToolConfig(tool: string): {
  icon: typeof Bot;
  className: string;
  label: string;
} {
  const lower = tool.toLowerCase();

  if (lower.includes('claude') || lower === 'write' || lower === 'edit') {
    return {
      icon: FileEdit,
      className: 'badge-claude',
      label: lower === 'write' || lower === 'edit' ? 'Claude' : tool,
    };
  }

  if (lower.includes('codex')) {
    return {
      icon: Sparkles,
      className: 'badge-codex',
      label: 'Codex',
    };
  }

  if (lower.includes('aider')) {
    return {
      icon: Cpu,
      className: 'badge-aider',
      label: 'Aider',
    };
  }

  if (lower.includes('cursor')) {
    return {
      icon: Bot,
      className: 'badge-cursor',
      label: 'Cursor',
    };
  }

  if (lower === 'bash') {
    return {
      icon: Terminal,
      className: 'badge-execute',
      label: 'Bash',
    };
  }

  return {
    icon: Bot,
    className: 'bg-noir-700/50 text-noir-300',
    label: tool,
  };
}

export function ToolBadge({ tool, showIcon = true, className }: ToolBadgeProps) {
  const config = getToolConfig(tool);
  const Icon = config.icon;

  return (
    <span className={cn('badge inline-flex items-center gap-1.5', config.className, className)}>
      {showIcon && <Icon size={12} />}
      <span>{config.label}</span>
    </span>
  );
}
