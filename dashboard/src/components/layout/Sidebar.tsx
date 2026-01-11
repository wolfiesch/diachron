import { NavLink } from 'react-router-dom';
import { cn } from '@/lib/utils';
import { useHealth, useDiagnostics } from '@/api/hooks';
import { formatBytes, formatDuration } from '@/types/diachron';
import {
  LayoutDashboard,
  Clock,
  Users,
  Search,
  Target,
  FileCheck,
  Stethoscope,
  Activity,
  Database,
  Cpu,
  ExternalLink,
} from 'lucide-react';

interface NavItem {
  to: string;
  icon: typeof LayoutDashboard;
  label: string;
}

const mainNavItems: NavItem[] = [
  { to: '/', icon: LayoutDashboard, label: 'Dashboard' },
  { to: '/timeline', icon: Clock, label: 'Timeline' },
  { to: '/sessions', icon: Users, label: 'Sessions' },
  { to: '/search', icon: Search, label: 'Search' },
  { to: '/evidence', icon: FileCheck, label: 'Evidence Packs' },
];

const toolsNavItems: NavItem[] = [
  { to: '/blame', icon: Target, label: 'Blame Lookup' },
  { to: '/doctor', icon: Stethoscope, label: 'Diagnostics' },
];

export function Sidebar() {
  const { data: health } = useHealth();
  const { data: diagnostics } = useDiagnostics();

  const isConnected = health?.status === 'ok';

  return (
    <aside className="fixed inset-y-0 left-0 w-64 bg-noir-900/50 border-r border-noir-800 flex flex-col">
      {/* Logo */}
      <div className="h-16 flex items-center px-5 border-b border-noir-800">
        <div className="flex items-center gap-3">
          <div className="w-8 h-8 rounded-lg bg-gradient-to-br from-accent-primary to-accent-secondary flex items-center justify-center">
            <Activity size={18} className="text-white" />
          </div>
          <div>
            <h1 className="font-display font-semibold text-lg text-noir-100">
              Diachron
            </h1>
            <p className="text-2xs text-noir-500 -mt-0.5">AI Provenance</p>
          </div>
        </div>
      </div>

      {/* Navigation */}
      <nav className="flex-1 overflow-y-auto py-4 px-3 space-y-6">
        {/* Main Navigation */}
        <div className="space-y-1">
          {mainNavItems.map((item) => (
            <NavLink
              key={item.to}
              to={item.to}
              end={item.to === '/'}
              className={({ isActive }) =>
                cn('nav-item', isActive && 'active')
              }
            >
              <item.icon size={18} />
              <span>{item.label}</span>
            </NavLink>
          ))}
        </div>

        {/* Tools */}
        <div>
          <p className="px-3 mb-2 text-xs font-medium text-noir-600 uppercase tracking-wider">
            Tools
          </p>
          <div className="space-y-1">
            {toolsNavItems.map((item) => (
              <NavLink
                key={item.to}
                to={item.to}
                className={({ isActive }) =>
                  cn('nav-item', isActive && 'active')
                }
              >
                <item.icon size={18} />
                <span>{item.label}</span>
              </NavLink>
            ))}
          </div>
        </div>
      </nav>

      {/* System Status */}
      <div className="p-4 border-t border-noir-800 space-y-3">
        <div className="flex items-center justify-between">
          <span className="text-xs font-medium text-noir-500 uppercase tracking-wider">
            System Status
          </span>
          {isConnected && (
            <span className="flex items-center gap-1 text-2xs text-confidence-high">
              <span className="w-1.5 h-1.5 rounded-full bg-confidence-high animate-pulse" />
              Live
            </span>
          )}
        </div>

        <div className="space-y-2.5">
          {/* Daemon Status */}
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2 text-sm">
              <Activity size={14} className={isConnected ? 'text-confidence-high' : 'text-op-delete'} />
              <span className="text-noir-400">Daemon</span>
            </div>
            <span className={cn(
              'text-xs font-mono',
              isConnected ? 'text-noir-300' : 'text-op-delete'
            )}>
              {isConnected
                ? health?.uptime_secs
                  ? formatDuration(health.uptime_secs)
                  : 'Connected'
                : 'Disconnected'}
            </span>
          </div>

          {/* Events Count */}
          {diagnostics && (
            <>
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-2 text-sm">
                  <Database size={14} className="text-accent-primary" />
                  <span className="text-noir-400">Events</span>
                </div>
                <span className="text-xs font-mono text-noir-300">
                  {diagnostics.events_count.toLocaleString()}
                </span>
              </div>

              {/* Memory */}
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-2 text-sm">
                  <Cpu size={14} className="text-accent-secondary" />
                  <span className="text-noir-400">Memory</span>
                </div>
                <span className="text-xs font-mono text-noir-300">
                  {formatBytes(diagnostics.memory_rss_bytes)}
                </span>
              </div>
            </>
          )}
        </div>
      </div>

      {/* Footer */}
      <div className="p-4 border-t border-noir-800">
        <a
          href="https://github.com/anthropics/diachron"
          target="_blank"
          rel="noopener noreferrer"
          className="flex items-center gap-2 text-xs text-noir-500 hover:text-noir-300 transition-colors"
        >
          <ExternalLink size={12} />
          <span>Documentation</span>
        </a>
      </div>
    </aside>
  );
}
