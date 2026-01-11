import { useState } from 'react';
import { motion } from 'framer-motion';
import { Search, Sparkles, FileText, Clock, Zap } from 'lucide-react';
import { useSearch } from '@/api/hooks';
import { formatRelativeTime, type SearchResult } from '@/types/diachron';
import { cn } from '@/lib/utils';

type SearchMode = 'hybrid' | 'semantic' | 'keyword';
type SourceFilter = 'all' | 'event' | 'exchange';

export function SearchPage() {
  const [query, setQuery] = useState('');
  const [mode, setMode] = useState<SearchMode>('hybrid');
  const [sourceFilter, setSourceFilter] = useState<SourceFilter>('all');
  const [results, setResults] = useState<SearchResult[]>([]);

  const { mutate: search, isPending } = useSearch();

  const handleSearch = () => {
    if (!query.trim()) return;

    search(
      {
        query: query.trim(),
        limit: 50,
        source_filter: sourceFilter === 'all' ? null : sourceFilter,
      },
      {
        onSuccess: (data) => {
          setResults(data);
        },
      }
    );
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSearch();
    }
  };

  return (
    <div className="space-y-6">
      {/* Header */}
      <motion.div
        initial={{ opacity: 0, y: -20 }}
        animate={{ opacity: 1, y: 0 }}
      >
        <h1 className="text-3xl font-display font-bold text-noir-100">
          Search
        </h1>
        <p className="text-noir-400 mt-1">
          Search across events and conversations
        </p>
      </motion.div>

      {/* Search Box */}
      <motion.div
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.1 }}
        className="card-noir p-6 space-y-4"
      >
        {/* Query Input */}
        <div className="relative">
          <Search
            size={20}
            className="absolute left-4 top-1/2 -translate-y-1/2 text-noir-500"
          />
          <input
            type="text"
            placeholder="Search for events, files, code, or conversations..."
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={handleKeyDown}
            className="input-noir pl-12 pr-24 h-14 text-lg"
          />
          <button
            onClick={handleSearch}
            disabled={!query.trim() || isPending}
            className={cn(
              'absolute right-2 top-1/2 -translate-y-1/2',
              'btn btn-primary h-10'
            )}
          >
            {isPending ? (
              <span className="flex items-center gap-2">
                <span className="w-4 h-4 border-2 border-white/30 border-t-white rounded-full animate-spin" />
                Searching
              </span>
            ) : (
              <span className="flex items-center gap-2">
                <Zap size={16} />
                Search
              </span>
            )}
          </button>
        </div>

        {/* Filters */}
        <div className="flex items-center gap-6">
          {/* Search Mode */}
          <div className="flex items-center gap-2">
            <span className="text-xs text-noir-500">Mode:</span>
            <div className="flex items-center gap-1 bg-noir-800/50 rounded-lg p-1">
              {[
                { value: 'hybrid', label: 'Hybrid', icon: Zap },
                { value: 'semantic', label: 'Semantic', icon: Sparkles },
                { value: 'keyword', label: 'Keyword', icon: FileText },
              ].map((option) => (
                <button
                  key={option.value}
                  onClick={() => setMode(option.value as SearchMode)}
                  className={cn(
                    'px-3 py-1.5 text-xs font-medium rounded-md transition-all flex items-center gap-1.5',
                    mode === option.value
                      ? 'bg-accent-primary text-white'
                      : 'text-noir-400 hover:text-noir-100 hover:bg-noir-700/50'
                  )}
                >
                  <option.icon size={12} />
                  {option.label}
                </button>
              ))}
            </div>
          </div>

          {/* Source Filter */}
          <div className="flex items-center gap-2">
            <span className="text-xs text-noir-500">Source:</span>
            <div className="flex items-center gap-1 bg-noir-800/50 rounded-lg p-1">
              {[
                { value: 'all', label: 'All' },
                { value: 'event', label: 'Events' },
                { value: 'exchange', label: 'Conversations' },
              ].map((option) => (
                <button
                  key={option.value}
                  onClick={() => setSourceFilter(option.value as SourceFilter)}
                  className={cn(
                    'px-3 py-1.5 text-xs font-medium rounded-md transition-all',
                    sourceFilter === option.value
                      ? 'bg-accent-primary text-white'
                      : 'text-noir-400 hover:text-noir-100 hover:bg-noir-700/50'
                  )}
                >
                  {option.label}
                </button>
              ))}
            </div>
          </div>
        </div>
      </motion.div>

      {/* Results */}
      <motion.div
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.2 }}
        className="space-y-4"
      >
        {results.length > 0 && (
          <div className="flex items-center justify-between">
            <p className="text-sm text-noir-400">
              Found {results.length} results
            </p>
          </div>
        )}

        {results.length > 0 ? (
          <div className="space-y-3">
            {results.map((result, index) => (
              <motion.div
                key={result.id}
                initial={{ opacity: 0, y: 10 }}
                animate={{ opacity: 1, y: 0 }}
                transition={{ delay: index * 0.03 }}
                className="card-noir p-4 hover:border-accent-primary/30 transition-colors"
              >
                <div className="flex items-start gap-4">
                  <div
                    className={cn(
                      'p-2 rounded-lg',
                      result.source === 'event'
                        ? 'bg-accent-primary/10'
                        : 'bg-accent-secondary/10'
                    )}
                  >
                    {result.source === 'event' ? (
                      <FileText size={18} className="text-accent-primary" />
                    ) : (
                      <Sparkles size={18} className="text-accent-secondary" />
                    )}
                  </div>

                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2 mb-2">
                      <span
                        className={cn(
                          'badge text-2xs',
                          result.source === 'event'
                            ? 'bg-accent-primary/20 text-accent-primary'
                            : 'bg-accent-secondary/20 text-accent-secondary'
                        )}
                      >
                        {result.source === 'event' ? 'Event' : 'Conversation'}
                      </span>
                      <span className="text-xs text-noir-500">
                        Score: {(result.score * 100).toFixed(1)}%
                      </span>
                      {result.project && (
                        <span className="mono-value text-2xs">{result.project}</span>
                      )}
                    </div>

                    <p className="text-sm text-noir-200 leading-relaxed">
                      {result.snippet}
                    </p>

                    <div className="flex items-center gap-2 mt-2 text-xs text-noir-500">
                      <Clock size={12} />
                      <span>{formatRelativeTime(result.timestamp)}</span>
                    </div>
                  </div>
                </div>
              </motion.div>
            ))}
          </div>
        ) : query && !isPending ? (
          <div className="card-noir p-12 text-center">
            <Search size={48} className="mx-auto mb-4 text-noir-700" />
            <p className="text-noir-400 mb-2">No results found</p>
            <p className="text-xs text-noir-600">
              Try different keywords or search mode
            </p>
          </div>
        ) : !query ? (
          <div className="card-noir p-12 text-center">
            <Sparkles size={48} className="mx-auto mb-4 text-noir-700" />
            <p className="text-noir-400 mb-2">Semantic + Keyword Search</p>
            <p className="text-xs text-noir-600 max-w-md mx-auto">
              Search across all tracked events and indexed conversations.
              Use semantic mode for concept-based search or keyword mode for exact matches.
            </p>
          </div>
        ) : null}
      </motion.div>
    </div>
  );
}
