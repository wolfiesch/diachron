import { motion } from 'framer-motion';
import { Link, useParams } from 'react-router-dom';
import { Users, Clock, ArrowRight, ChevronLeft } from 'lucide-react';
import { useSessions, useSession } from '@/api/hooks';
import { ToolBadge } from '@/components/shared/ToolBadge';
import { OperationIcon } from '@/components/shared/OperationIcon';
import { formatRelativeTime, normalizeOperation } from '@/types/diachron';

function SessionList() {
  const { data: sessions, isLoading } = useSessions();

  return (
    <div className="space-y-6">
      {/* Header */}
      <motion.div
        initial={{ opacity: 0, y: -20 }}
        animate={{ opacity: 1, y: 0 }}
      >
        <h1 className="text-3xl font-display font-bold text-noir-100">
          Sessions
        </h1>
        <p className="text-noir-400 mt-1">
          {sessions?.length || 0} work sessions
        </p>
      </motion.div>

      {/* Sessions Grid */}
      <motion.div
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.1 }}
        className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4"
      >
        {isLoading ? (
          Array.from({ length: 6 }).map((_, i) => (
            <div key={i} className="card-noir p-5 space-y-3">
              <div className="skeleton h-5 w-32" />
              <div className="skeleton h-4 w-20" />
              <div className="skeleton h-3 w-24" />
            </div>
          ))
        ) : sessions && sessions.length > 0 ? (
          sessions.map((session, index) => (
            <motion.div
              key={session.session_id}
              initial={{ opacity: 0, y: 20 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ delay: 0.05 * index }}
            >
              <Link
                to={`/sessions/${session.session_id}`}
                className="card-noir p-5 block group hover:border-accent-primary/30"
              >
                <div className="flex items-start justify-between mb-3">
                  <div className="p-2 rounded-lg bg-accent-primary/10">
                    <Users size={18} className="text-accent-primary" />
                  </div>
                  <ArrowRight
                    size={16}
                    className="text-noir-600 group-hover:text-accent-primary transition-colors"
                  />
                </div>

                <p className="mono-value mb-2">
                  {session.session_id.slice(0, 12)}...
                </p>

                <div className="flex items-center gap-2 mb-3">
                  <span className="text-sm font-medium text-noir-100">
                    {session.event_count} events
                  </span>
                  <span className="text-noir-600">•</span>
                  <span className="text-xs text-noir-500">
                    {session.files?.filter(Boolean).length || 0} files
                  </span>
                </div>

                <div className="flex flex-wrap gap-1 mb-3">
                  {session.tools.slice(0, 3).map((tool) => (
                    <ToolBadge key={tool} tool={tool} showIcon={false} />
                  ))}
                  {session.tools.length > 3 && (
                    <span className="badge bg-noir-700 text-noir-400">
                      +{session.tools.length - 3}
                    </span>
                  )}
                </div>

                <div className="flex items-center gap-1.5 text-xs text-noir-500">
                  <Clock size={12} />
                  <span>{formatRelativeTime(session.last_event)}</span>
                </div>
              </Link>
            </motion.div>
          ))
        ) : (
          <div className="col-span-full card-noir p-12 text-center">
            <Users size={48} className="mx-auto mb-4 text-noir-700" />
            <p className="text-noir-400 mb-2">No sessions yet</p>
            <p className="text-xs text-noir-600">
              Sessions are created automatically when you use AI coding assistants
            </p>
          </div>
        )}
      </motion.div>
    </div>
  );
}

function SessionDetail() {
  const { id } = useParams<{ id: string }>();
  const { data: session, isLoading } = useSession(id || '');

  if (isLoading) {
    return (
      <div className="space-y-6">
        <div className="skeleton h-8 w-48" />
        <div className="skeleton h-4 w-32" />
        <div className="card-noir p-6 space-y-4">
          {Array.from({ length: 5 }).map((_, i) => (
            <div key={i} className="skeleton h-16 w-full" />
          ))}
        </div>
      </div>
    );
  }

  if (!session) {
    return (
      <div className="card-noir p-12 text-center">
        <Users size={48} className="mx-auto mb-4 text-noir-700" />
        <p className="text-noir-400 mb-2">Session not found</p>
        <Link to="/sessions" className="btn btn-secondary mt-4">
          <ChevronLeft size={16} />
          Back to sessions
        </Link>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Header */}
      <motion.div
        initial={{ opacity: 0, y: -20 }}
        animate={{ opacity: 1, y: 0 }}
      >
        <Link
          to="/sessions"
          className="inline-flex items-center gap-1 text-sm text-noir-400 hover:text-noir-100 mb-4"
        >
          <ChevronLeft size={16} />
          Back to sessions
        </Link>

        <h1 className="text-3xl font-display font-bold text-noir-100">
          Session Details
        </h1>
        <p className="mono-value mt-2">{session.session_id}</p>
      </motion.div>

      {/* Stats */}
      <motion.div
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.1 }}
        className="grid grid-cols-1 md:grid-cols-4 gap-4"
      >
        <div className="card-noir p-4">
          <p className="text-xs text-noir-500 uppercase tracking-wider">Events</p>
          <p className="text-2xl font-display font-semibold text-noir-100 mt-1">
            {session.event_count}
          </p>
        </div>
        <div className="card-noir p-4">
          <p className="text-xs text-noir-500 uppercase tracking-wider">Files</p>
          <p className="text-2xl font-display font-semibold text-noir-100 mt-1">
            {session.files?.filter(Boolean).length || 0}
          </p>
        </div>
        <div className="card-noir p-4">
          <p className="text-xs text-noir-500 uppercase tracking-wider">Started</p>
          <p className="text-sm text-noir-300 mt-1">
            {formatRelativeTime(session.first_event)}
          </p>
        </div>
        <div className="card-noir p-4">
          <p className="text-xs text-noir-500 uppercase tracking-wider">Last Activity</p>
          <p className="text-sm text-noir-300 mt-1">
            {formatRelativeTime(session.last_event)}
          </p>
        </div>
      </motion.div>

      {/* Tools Used */}
      <motion.div
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.15 }}
        className="card-noir p-5"
      >
        <h3 className="text-sm font-medium text-noir-400 mb-3">Tools Used</h3>
        <div className="flex flex-wrap gap-2">
          {session.tools.map((tool) => (
            <ToolBadge key={tool} tool={tool} />
          ))}
        </div>
      </motion.div>

      {/* Events Timeline */}
      <motion.div
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.2 }}
        className="card-noir"
      >
        <div className="p-5 border-b border-noir-800">
          <h3 className="font-semibold text-noir-100">Events Timeline</h3>
        </div>
        <div className="divide-y divide-noir-800/50 max-h-[500px] overflow-y-auto">
          {session.events.map((event, index) => (
            <motion.div
              key={event.id}
              initial={{ opacity: 0, x: -10 }}
              animate={{ opacity: 1, x: 0 }}
              transition={{ delay: 0.02 * index }}
              className="p-4 flex items-center gap-4 hover:bg-noir-800/30 transition-colors"
            >
              <div className="p-2 rounded-lg bg-noir-800/50">
                <OperationIcon
                  operation={normalizeOperation(event.operation)}
                  size={16}
                />
              </div>
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2 mb-1">
                  <ToolBadge tool={event.tool_name} />
                  {event.file_path && (
                    <span className="file-path truncate text-sm">
                      {event.file_path}
                    </span>
                  )}
                </div>
                <div className="text-xs text-noir-500">
                  {event.timestamp_display || formatRelativeTime(event.timestamp)}
                </div>
              </div>
              {event.diff_summary && (
                <span className="font-mono text-xs">
                  <span className="diff-plus">
                    +{event.diff_summary.match(/\+(\d+)/)?.[1] || 0}
                  </span>
                  {' / '}
                  <span className="diff-minus">
                    -{event.diff_summary.match(/-(\d+)/)?.[1] || 0}
                  </span>
                </span>
              )}
            </motion.div>
          ))}
        </div>
      </motion.div>
    </div>
  );
}

export function SessionsPage() {
  const { id } = useParams<{ id: string }>();
  return id ? <SessionDetail /> : <SessionList />;
}
