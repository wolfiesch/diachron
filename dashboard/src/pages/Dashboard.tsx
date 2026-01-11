import { motion } from 'framer-motion';
import { Activity, Clock, FolderGit2, Users, ArrowRight, Zap } from 'lucide-react';
import { Link } from 'react-router-dom';
import { useEvents, useSessions, useDiagnostics } from '@/api/hooks';
import { StatCard, StatCardSkeleton } from '@/components/shared';
import { ToolBadge } from '@/components/shared/ToolBadge';
import { OperationIcon } from '@/components/shared/OperationIcon';
import { formatBytes, formatRelativeTime, normalizeOperation } from '@/types/diachron';
import { cn } from '@/lib/utils';

export function DashboardPage() {
  // Fetch 100 events for stats, but only display first 10 in Recent Activity
  const { data: allEvents, isLoading: eventsLoading } = useEvents({ limit: 100 });
  const { data: sessions, isLoading: sessionsLoading } = useSessions();
  const { data: diagnostics, isLoading: diagnosticsLoading } = useDiagnostics();

  // Events for display (first 10)
  const recentEvents = allEvents?.slice(0, 10);

  // Calculate stats from larger sample
  const totalEvents = diagnostics?.events_count || 0;
  const totalSessions = sessions?.length || 0;
  const uniqueFiles = allEvents
    ? new Set(allEvents.filter((e) => e.file_path).map((e) => e.file_path)).size
    : 0;
  const dbSize = diagnostics ? formatBytes(diagnostics.database_size_bytes) : '—';

  return (
    <div className="space-y-8">
      {/* Header */}
      <motion.div
        initial={{ opacity: 0, y: -20 }}
        animate={{ opacity: 1, y: 0 }}
        className="space-y-2"
      >
        <h1 className="text-3xl font-display font-bold text-noir-100">
          Dashboard
        </h1>
        <p className="text-noir-400">
          AI provenance tracking overview
        </p>
      </motion.div>

      {/* Stat Cards */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
        {diagnosticsLoading ? (
          <>
            <StatCardSkeleton />
            <StatCardSkeleton />
            <StatCardSkeleton />
            <StatCardSkeleton />
          </>
        ) : (
          <>
            <StatCard
              title="Total Events"
              value={totalEvents.toLocaleString()}
              subtitle="All tracked changes"
              icon={Activity}
              iconColor="text-accent-primary"
              delay={0}
            />
            <StatCard
              title="Sessions"
              value={totalSessions.toLocaleString()}
              subtitle="Unique work sessions"
              icon={Users}
              iconColor="text-accent-secondary"
              delay={0.1}
            />
            <StatCard
              title="Files Tracked"
              value={uniqueFiles.toLocaleString()}
              subtitle="Recently modified"
              icon={FolderGit2}
              iconColor="text-confidence-high"
              delay={0.2}
            />
            <StatCard
              title="Database"
              value={dbSize}
              subtitle={diagnostics?.model_loaded ? 'Embeddings loaded' : 'No embeddings'}
              icon={Zap}
              iconColor="text-confidence-medium"
              delay={0.3}
            />
          </>
        )}
      </div>

      {/* Main Content Grid */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Recent Activity */}
        <div className="lg:col-span-2">
          <motion.div
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ delay: 0.2 }}
            className="card-noir"
          >
            <div className="flex items-center justify-between p-5 border-b border-noir-800">
              <div className="flex items-center gap-3">
                <div className="p-2 rounded-lg bg-accent-primary/10">
                  <Clock size={18} className="text-accent-primary" />
                </div>
                <div>
                  <h2 className="font-semibold text-noir-100">Recent Activity</h2>
                  <p className="text-xs text-noir-500">Latest AI-assisted changes</p>
                </div>
              </div>
              <Link
                to="/timeline"
                className="btn btn-ghost text-xs"
              >
                View all
                <ArrowRight size={14} />
              </Link>
            </div>

            <div className="divide-y divide-noir-800/50">
              {eventsLoading ? (
                Array.from({ length: 5 }).map((_, i) => (
                  <div key={i} className="p-4 flex items-center gap-4">
                    <div className="skeleton w-8 h-8 rounded" />
                    <div className="flex-1 space-y-2">
                      <div className="skeleton h-4 w-3/4" />
                      <div className="skeleton h-3 w-1/2" />
                    </div>
                  </div>
                ))
              ) : recentEvents && recentEvents.length > 0 ? (
                recentEvents.map((event, index) => (
                  <motion.div
                    key={event.id}
                    initial={{ opacity: 0, x: -20 }}
                    animate={{ opacity: 1, x: 0 }}
                    transition={{ delay: 0.1 + index * 0.05 }}
                    className="p-4 flex items-center gap-4 hover:bg-noir-800/30 transition-colors group"
                  >
                    <div className="p-2 rounded-lg bg-noir-800/50 group-hover:bg-noir-700/50 transition-colors">
                      <OperationIcon
                        operation={normalizeOperation(event.operation)}
                        size={18}
                      />
                    </div>
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2 mb-1">
                        <ToolBadge tool={event.tool_name} />
                        {event.file_path && (
                          <span className="file-path truncate max-w-[300px]">
                            {event.file_path.split('/').pop()}
                          </span>
                        )}
                      </div>
                      <div className="flex items-center gap-2 text-xs text-noir-500">
                        {event.diff_summary && (
                          <span className="font-mono">
                            <span className="diff-plus">+{event.diff_summary.match(/\+(\d+)/)?.[1] || 0}</span>
                            {' / '}
                            <span className="diff-minus">-{event.diff_summary.match(/-(\d+)/)?.[1] || 0}</span>
                          </span>
                        )}
                        <span>•</span>
                        <span>{formatRelativeTime(event.timestamp)}</span>
                      </div>
                    </div>
                    <div className="opacity-0 group-hover:opacity-100 transition-opacity">
                      <ArrowRight size={16} className="text-noir-500" />
                    </div>
                  </motion.div>
                ))
              ) : (
                <div className="p-8 text-center text-noir-500">
                  <Activity size={32} className="mx-auto mb-3 opacity-50" />
                  <p>No events tracked yet</p>
                  <p className="text-xs mt-1">
                    Events will appear here as you use AI coding assistants
                  </p>
                </div>
              )}
            </div>
          </motion.div>
        </div>

        {/* Quick Actions & Sessions */}
        <div className="space-y-6">
          {/* Quick Actions */}
          <motion.div
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ delay: 0.3 }}
            className="card-noir p-5 space-y-4"
          >
            <h2 className="font-semibold text-noir-100">Quick Actions</h2>
            <div className="space-y-2">
              <Link to="/search" className="btn btn-secondary w-full justify-start">
                <Activity size={16} />
                Search Events
              </Link>
              <Link to="/blame" className="btn btn-secondary w-full justify-start">
                <Activity size={16} />
                Blame Lookup
              </Link>
              <Link to="/doctor" className="btn btn-secondary w-full justify-start">
                <Activity size={16} />
                Run Diagnostics
              </Link>
            </div>
          </motion.div>

          {/* Recent Sessions */}
          <motion.div
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ delay: 0.4 }}
            className="card-noir"
          >
            <div className="p-5 border-b border-noir-800">
              <h2 className="font-semibold text-noir-100">Recent Sessions</h2>
              <p className="text-xs text-noir-500 mt-1">Active work sessions</p>
            </div>
            <div className="divide-y divide-noir-800/50">
              {sessionsLoading ? (
                Array.from({ length: 3 }).map((_, i) => (
                  <div key={i} className="p-4">
                    <div className="skeleton h-4 w-24 mb-2" />
                    <div className="skeleton h-3 w-16" />
                  </div>
                ))
              ) : sessions && sessions.length > 0 ? (
                sessions.slice(0, 5).map((session, index) => (
                  <Link
                    key={session.session_id}
                    to={`/sessions/${session.session_id}`}
                    className={cn(
                      'block p-4 hover:bg-noir-800/30 transition-colors',
                      'animate-fade-in',
                      index === 0 && 'stagger-1',
                      index === 1 && 'stagger-2',
                      index === 2 && 'stagger-3',
                    )}
                  >
                    <div className="flex items-center justify-between">
                      <span className="mono-value truncate max-w-[120px]">
                        {session.session_id.slice(0, 8)}...
                      </span>
                      <span className="text-xs text-noir-500">
                        {session.event_count} events
                      </span>
                    </div>
                    <div className="text-xs text-noir-500 mt-1">
                      {formatRelativeTime(session.last_event)}
                    </div>
                  </Link>
                ))
              ) : (
                <div className="p-4 text-center text-noir-500 text-sm">
                  No sessions yet
                </div>
              )}
            </div>
            {sessions && sessions.length > 5 && (
              <div className="p-4 border-t border-noir-800">
                <Link to="/sessions" className="text-xs text-accent-primary hover:underline">
                  View all {sessions.length} sessions →
                </Link>
              </div>
            )}
          </motion.div>
        </div>
      </div>
    </div>
  );
}
