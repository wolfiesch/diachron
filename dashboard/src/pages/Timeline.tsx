import { useState, useMemo } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { useVirtualizer } from '@tanstack/react-virtual';
import { useRef } from 'react';
import {
  Clock,
  Filter,
  X,
  ChevronRight,
  FileEdit,
  Terminal,
  Sparkles,
  Bot,
} from 'lucide-react';
import { useEvents } from '@/api/hooks';
import { ToolBadge } from '@/components/shared/ToolBadge';
import { OperationIcon, OperationBadge } from '@/components/shared/OperationIcon';
import { formatRelativeTime, normalizeOperation, type StoredEvent } from '@/types/diachron';
import { cn } from '@/lib/utils';

type TimeFilter = '1h' | '24h' | '7d' | '30d' | 'all';
type ToolFilter = 'all' | 'claude' | 'codex' | 'bash' | 'other';

const timeFilterOptions: { value: TimeFilter; label: string }[] = [
  { value: '1h', label: 'Last hour' },
  { value: '24h', label: 'Last 24 hours' },
  { value: '7d', label: 'Last 7 days' },
  { value: '30d', label: 'Last 30 days' },
  { value: 'all', label: 'All time' },
];

const toolFilterOptions: { value: ToolFilter; label: string; icon: typeof FileEdit }[] = [
  { value: 'all', label: 'All tools', icon: Bot },
  { value: 'claude', label: 'Claude', icon: FileEdit },
  { value: 'codex', label: 'Codex', icon: Sparkles },
  { value: 'bash', label: 'Bash', icon: Terminal },
];

function getSinceFromFilter(filter: TimeFilter): string | undefined {
  // Daemon expects shorthand format: "1h", "24h", "7d", "30d"
  switch (filter) {
    case '1h':
      return '1h';
    case '24h':
      return '24h';
    case '7d':
      return '7d';
    case '30d':
      return '30d';
    case 'all':
    default:
      return undefined;
  }
}

function filterEventsByTool(events: StoredEvent[], toolFilter: ToolFilter): StoredEvent[] {
  if (toolFilter === 'all') return events;

  return events.filter((event) => {
    const tool = event.tool_name.toLowerCase();
    switch (toolFilter) {
      case 'claude':
        return tool.includes('claude') || tool === 'write' || tool === 'edit';
      case 'codex':
        return tool.includes('codex');
      case 'bash':
        return tool === 'bash';
      case 'other':
        return (
          !tool.includes('claude') &&
          tool !== 'write' &&
          tool !== 'edit' &&
          !tool.includes('codex') &&
          tool !== 'bash'
        );
      default:
        return true;
    }
  });
}

export function TimelinePage() {
  const [timeFilter, setTimeFilter] = useState<TimeFilter>('24h');
  const [toolFilter, setToolFilter] = useState<ToolFilter>('all');
  const [fileFilter, setFileFilter] = useState('');
  const [selectedEvent, setSelectedEvent] = useState<StoredEvent | null>(null);

  const parentRef = useRef<HTMLDivElement>(null);

  const { data: events, isLoading } = useEvents({
    since: getSinceFromFilter(timeFilter),
    file: fileFilter || undefined,
    limit: 500,
  });

  const filteredEvents = useMemo(() => {
    if (!events) return [];
    return filterEventsByTool(events, toolFilter);
  }, [events, toolFilter]);

  const virtualizer = useVirtualizer({
    count: filteredEvents.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 72,
    overscan: 10,
  });

  return (
    <div className="space-y-6">
      {/* Header */}
      <motion.div
        initial={{ opacity: 0, y: -20 }}
        animate={{ opacity: 1, y: 0 }}
        className="flex items-center justify-between"
      >
        <div>
          <h1 className="text-3xl font-display font-bold text-noir-100">
            Timeline
          </h1>
          <p className="text-noir-400 mt-1">
            {filteredEvents.length} events tracked
          </p>
        </div>
        <div className="flex items-center gap-2">
          <div className="status-indicator">
            <span className="status-dot status-dot-connected" />
            <span className="text-xs text-noir-400">Live updates</span>
          </div>
        </div>
      </motion.div>

      {/* Filters */}
      <motion.div
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.1 }}
        className="card-noir p-4 flex flex-wrap items-center gap-4"
      >
        <div className="flex items-center gap-2">
          <Filter size={16} className="text-noir-500" />
          <span className="text-sm text-noir-400">Filter:</span>
        </div>

        {/* Time Filter */}
        <div className="flex items-center gap-1 bg-noir-800/50 rounded-lg p-1">
          {timeFilterOptions.map((option) => (
            <button
              key={option.value}
              onClick={() => setTimeFilter(option.value)}
              className={cn(
                'px-3 py-1.5 text-xs font-medium rounded-md transition-all',
                timeFilter === option.value
                  ? 'bg-accent-primary text-white'
                  : 'text-noir-400 hover:text-noir-100 hover:bg-noir-700/50'
              )}
            >
              {option.label}
            </button>
          ))}
        </div>

        {/* Tool Filter */}
        <div className="flex items-center gap-1 bg-noir-800/50 rounded-lg p-1">
          {toolFilterOptions.map((option) => (
            <button
              key={option.value}
              onClick={() => setToolFilter(option.value)}
              className={cn(
                'px-3 py-1.5 text-xs font-medium rounded-md transition-all flex items-center gap-1.5',
                toolFilter === option.value
                  ? 'bg-accent-primary text-white'
                  : 'text-noir-400 hover:text-noir-100 hover:bg-noir-700/50'
              )}
            >
              <option.icon size={12} />
              {option.label}
            </button>
          ))}
        </div>

        {/* File Filter */}
        <div className="flex-1 min-w-[200px]">
          <input
            type="text"
            placeholder="Filter by file path..."
            value={fileFilter}
            onChange={(e) => setFileFilter(e.target.value)}
            className="input-noir text-sm h-9"
          />
        </div>

        {(toolFilter !== 'all' || fileFilter) && (
          <button
            onClick={() => {
              setToolFilter('all');
              setFileFilter('');
            }}
            className="btn btn-ghost text-xs"
          >
            <X size={14} />
            Clear filters
          </button>
        )}
      </motion.div>

      {/* Event List */}
      <div className="flex gap-6">
        <motion.div
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ delay: 0.2 }}
          className={cn(
            'flex-1 card-noir overflow-hidden',
            selectedEvent && 'max-w-[calc(100%-480px)]'
          )}
        >
          {isLoading ? (
            <div className="divide-y divide-noir-800/50">
              {Array.from({ length: 10 }).map((_, i) => (
                <div key={i} className="p-4 flex items-center gap-4">
                  <div className="skeleton w-10 h-10 rounded-lg" />
                  <div className="flex-1 space-y-2">
                    <div className="skeleton h-4 w-3/4" />
                    <div className="skeleton h-3 w-1/2" />
                  </div>
                </div>
              ))}
            </div>
          ) : filteredEvents.length > 0 ? (
            <div
              ref={parentRef}
              className="h-[calc(100vh-320px)] overflow-auto"
            >
              <div
                style={{
                  height: `${virtualizer.getTotalSize()}px`,
                  width: '100%',
                  position: 'relative',
                }}
              >
                {virtualizer.getVirtualItems().map((virtualItem) => {
                  const event = filteredEvents[virtualItem.index];
                  const isSelected = selectedEvent?.id === event.id;

                  return (
                    <div
                      key={virtualItem.key}
                      data-index={virtualItem.index}
                      ref={virtualizer.measureElement}
                      style={{
                        position: 'absolute',
                        top: 0,
                        left: 0,
                        width: '100%',
                        transform: `translateY(${virtualItem.start}px)`,
                      }}
                    >
                      <button
                        onClick={() => setSelectedEvent(isSelected ? null : event)}
                        className={cn(
                          'w-full p-4 flex items-center gap-4 text-left transition-colors',
                          'border-b border-noir-800/50',
                          isSelected
                            ? 'bg-accent-primary/10 border-l-2 border-l-accent-primary'
                            : 'hover:bg-noir-800/30'
                        )}
                      >
                        <div className="p-2.5 rounded-lg bg-noir-800/50">
                          <OperationIcon
                            operation={normalizeOperation(event.operation)}
                            size={18}
                          />
                        </div>
                        <div className="flex-1 min-w-0">
                          <div className="flex items-center gap-2 mb-1">
                            <ToolBadge tool={event.tool_name} />
                            {event.file_path && (
                              <span className="file-path truncate">
                                {event.file_path}
                              </span>
                            )}
                          </div>
                          <div className="flex items-center gap-2 text-xs text-noir-500">
                            {event.operation && (
                              <span className="capitalize">{event.operation}</span>
                            )}
                            {event.diff_summary && (
                              <>
                                <span>•</span>
                                <span className="font-mono">
                                  <span className="diff-plus">
                                    +{event.diff_summary.match(/\+(\d+)/)?.[1] || 0}
                                  </span>
                                  {' / '}
                                  <span className="diff-minus">
                                    -{event.diff_summary.match(/-(\d+)/)?.[1] || 0}
                                  </span>
                                </span>
                              </>
                            )}
                            <span>•</span>
                            <span>{formatRelativeTime(event.timestamp)}</span>
                          </div>
                        </div>
                        <ChevronRight
                          size={16}
                          className={cn(
                            'text-noir-600 transition-transform',
                            isSelected && 'rotate-90 text-accent-primary'
                          )}
                        />
                      </button>
                    </div>
                  );
                })}
              </div>
            </div>
          ) : (
            <div className="p-12 text-center">
              <Clock size={48} className="mx-auto mb-4 text-noir-700" />
              <p className="text-noir-400 mb-2">No events found</p>
              <p className="text-xs text-noir-600">
                Try adjusting your filters or time range
              </p>
            </div>
          )}
        </motion.div>

        {/* Event Detail Drawer */}
        <AnimatePresence>
          {selectedEvent && (
            <motion.div
              initial={{ opacity: 0, x: 20 }}
              animate={{ opacity: 1, x: 0 }}
              exit={{ opacity: 0, x: 20 }}
              className="w-[460px] card-noir overflow-hidden flex-shrink-0"
            >
              <div className="p-5 border-b border-noir-800 flex items-center justify-between">
                <h3 className="font-semibold text-noir-100">Event Details</h3>
                <button
                  onClick={() => setSelectedEvent(null)}
                  className="btn btn-ghost p-1.5"
                >
                  <X size={16} />
                </button>
              </div>

              <div className="p-5 space-y-5 max-h-[calc(100vh-400px)] overflow-y-auto">
                {/* Event ID */}
                <div>
                  <label className="text-xs font-medium text-noir-500 uppercase tracking-wider">
                    Event ID
                  </label>
                  <p className="mono-value mt-1">#{selectedEvent.id}</p>
                </div>

                {/* Tool & Operation */}
                <div className="flex gap-4">
                  <div>
                    <label className="text-xs font-medium text-noir-500 uppercase tracking-wider">
                      Tool
                    </label>
                    <div className="mt-1">
                      <ToolBadge tool={selectedEvent.tool_name} />
                    </div>
                  </div>
                  {selectedEvent.operation && (
                    <div>
                      <label className="text-xs font-medium text-noir-500 uppercase tracking-wider">
                        Operation
                      </label>
                      <div className="mt-1">
                        <OperationBadge operation={normalizeOperation(selectedEvent.operation)} />
                      </div>
                    </div>
                  )}
                </div>

                {/* File Path */}
                {selectedEvent.file_path && (
                  <div>
                    <label className="text-xs font-medium text-noir-500 uppercase tracking-wider">
                      File
                    </label>
                    <p className="file-path mt-1 break-all">{selectedEvent.file_path}</p>
                  </div>
                )}

                {/* Timestamp */}
                <div>
                  <label className="text-xs font-medium text-noir-500 uppercase tracking-wider">
                    Timestamp
                  </label>
                  <p className="text-noir-300 mt-1">
                    {selectedEvent.timestamp_display || selectedEvent.timestamp}
                  </p>
                  <p className="text-xs text-noir-500 mt-0.5">
                    {formatRelativeTime(selectedEvent.timestamp)}
                  </p>
                </div>

                {/* Session */}
                {selectedEvent.session_id && (
                  <div>
                    <label className="text-xs font-medium text-noir-500 uppercase tracking-wider">
                      Session
                    </label>
                    <p className="mono-value mt-1">{selectedEvent.session_id}</p>
                  </div>
                )}

                {/* Diff Summary */}
                {selectedEvent.diff_summary && (
                  <div>
                    <label className="text-xs font-medium text-noir-500 uppercase tracking-wider">
                      Changes
                    </label>
                    <div className="mt-2 code-block">
                      <p className="font-mono text-sm">
                        <span className="diff-plus">
                          +{selectedEvent.diff_summary.match(/\+(\d+)/)?.[1] || 0} lines added
                        </span>
                      </p>
                      <p className="font-mono text-sm">
                        <span className="diff-minus">
                          -{selectedEvent.diff_summary.match(/-(\d+)/)?.[1] || 0} lines removed
                        </span>
                      </p>
                    </div>
                  </div>
                )}

                {/* Git Commit */}
                {selectedEvent.git_commit_sha && (
                  <div>
                    <label className="text-xs font-medium text-noir-500 uppercase tracking-wider">
                      Git Commit
                    </label>
                    <p className="mono-value mt-1">
                      {selectedEvent.git_commit_sha.slice(0, 8)}
                    </p>
                  </div>
                )}

                {/* AI Summary */}
                {selectedEvent.ai_summary && (
                  <div>
                    <label className="text-xs font-medium text-noir-500 uppercase tracking-wider">
                      AI Summary
                    </label>
                    <p className="text-noir-300 mt-1 text-sm leading-relaxed">
                      {selectedEvent.ai_summary}
                    </p>
                  </div>
                )}
              </div>
            </motion.div>
          )}
        </AnimatePresence>
      </div>
    </div>
  );
}
