import { useState } from 'react';
import { motion } from 'framer-motion';
import { Target, FileCode, Hash, Sparkles } from 'lucide-react';
import { useBlame } from '@/api/hooks';
import { ConfidenceBadge } from '@/components/shared/ConfidenceBadge';
import { ToolBadge } from '@/components/shared/ToolBadge';
import { OperationBadge } from '@/components/shared/OperationIcon';
import { formatRelativeTime, normalizeOperation, type BlameMatch } from '@/types/diachron';
import { cn } from '@/lib/utils';

type BlameMode = 'strict' | 'best-effort' | 'inferred';

export function BlamePage() {
  const [filePath, setFilePath] = useState('');
  const [lineNumber, setLineNumber] = useState('');
  const [content, setContent] = useState('');
  const [mode, setMode] = useState<BlameMode>('best-effort');
  const [result, setResult] = useState<BlameMatch | null | undefined>(undefined);

  const { mutate: blame, isPending } = useBlame();

  const handleBlame = () => {
    if (!filePath || !lineNumber) return;

    blame(
      {
        file_path: filePath,
        line_number: parseInt(lineNumber, 10),
        content: content || '',
        context: '',
        mode,
      },
      {
        onSuccess: (data) => {
          setResult(data);
        },
      }
    );
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleBlame();
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
          Blame Lookup
        </h1>
        <p className="text-noir-400 mt-1">
          Find AI provenance for specific lines of code
        </p>
      </motion.div>

      {/* Input Form */}
      <motion.div
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.1 }}
        className="card-noir p-6 space-y-4"
      >
        <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
          {/* File Path */}
          <div className="md:col-span-2">
            <label className="block text-xs font-medium text-noir-500 uppercase tracking-wider mb-2">
              File Path
            </label>
            <div className="relative">
              <FileCode
                size={16}
                className="absolute left-3 top-1/2 -translate-y-1/2 text-noir-500"
              />
              <input
                type="text"
                placeholder="src/components/Button.tsx"
                value={filePath}
                onChange={(e) => setFilePath(e.target.value)}
                onKeyDown={handleKeyDown}
                className="input-noir pl-10"
              />
            </div>
          </div>

          {/* Line Number */}
          <div>
            <label className="block text-xs font-medium text-noir-500 uppercase tracking-wider mb-2">
              Line Number
            </label>
            <div className="relative">
              <Hash
                size={16}
                className="absolute left-3 top-1/2 -translate-y-1/2 text-noir-500"
              />
              <input
                type="number"
                placeholder="42"
                value={lineNumber}
                onChange={(e) => setLineNumber(e.target.value)}
                onKeyDown={handleKeyDown}
                className="input-noir pl-10"
                min="1"
              />
            </div>
          </div>
        </div>

        {/* Line Content (Optional) */}
        <div>
          <label className="block text-xs font-medium text-noir-500 uppercase tracking-wider mb-2">
            Line Content <span className="text-noir-600">(Optional - improves accuracy)</span>
          </label>
          <input
            type="text"
            placeholder="const handleClick = () => { ... }"
            value={content}
            onChange={(e) => setContent(e.target.value)}
            onKeyDown={handleKeyDown}
            className="input-noir font-mono text-sm"
          />
        </div>

        {/* Mode Selection */}
        <div className="flex items-center gap-4">
          <span className="text-xs text-noir-500">Mode:</span>
          <div className="flex items-center gap-1 bg-noir-800/50 rounded-lg p-1">
            {[
              { value: 'strict', label: 'Strict', desc: 'High confidence only' },
              { value: 'best-effort', label: 'Best Effort', desc: 'Recommended' },
              { value: 'inferred', label: 'Inferred', desc: 'Include heuristics' },
            ].map((option) => (
              <button
                key={option.value}
                onClick={() => setMode(option.value as BlameMode)}
                className={cn(
                  'px-3 py-1.5 text-xs font-medium rounded-md transition-all',
                  mode === option.value
                    ? 'bg-accent-primary text-white'
                    : 'text-noir-400 hover:text-noir-100 hover:bg-noir-700/50'
                )}
                title={option.desc}
              >
                {option.label}
              </button>
            ))}
          </div>

          <button
            onClick={handleBlame}
            disabled={!filePath || !lineNumber || isPending}
            className="btn btn-primary ml-auto"
          >
            {isPending ? (
              <span className="flex items-center gap-2">
                <span className="w-4 h-4 border-2 border-white/30 border-t-white rounded-full animate-spin" />
                Looking up...
              </span>
            ) : (
              <span className="flex items-center gap-2">
                <Target size={16} />
                Blame
              </span>
            )}
          </button>
        </div>
      </motion.div>

      {/* Result */}
      {result !== undefined && (
        <motion.div
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ delay: 0.2 }}
        >
          {result ? (
            <div className="card-noir p-6 space-y-6">
              {/* Header */}
              <div className="flex items-start justify-between">
                <div>
                  <h3 className="text-lg font-semibold text-noir-100 mb-2">
                    AI Provenance Found
                  </h3>
                  <div className="flex items-center gap-3">
                    <ConfidenceBadge confidence={result.confidence} />
                    <span className="text-xs text-noir-500">
                      Similarity: {(result.similarity * 100).toFixed(1)}%
                    </span>
                  </div>
                </div>
                <div className="p-3 rounded-lg bg-confidence-high/10">
                  <Target size={24} className="text-confidence-high" />
                </div>
              </div>

              {/* Intent */}
              {result.intent && (
                <div className="bg-noir-800/50 rounded-lg p-4 border-l-2 border-accent-primary">
                  <p className="text-xs font-medium text-noir-500 uppercase tracking-wider mb-2">
                    Intent
                  </p>
                  <p className="text-noir-200 italic">"{result.intent}"</p>
                </div>
              )}

              {/* Event Details */}
              <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
                <div className="space-y-4">
                  <div>
                    <p className="text-xs font-medium text-noir-500 uppercase tracking-wider">
                      Tool
                    </p>
                    <div className="mt-1">
                      <ToolBadge tool={result.event.tool_name} />
                    </div>
                  </div>

                  {result.event.operation && (
                    <div>
                      <p className="text-xs font-medium text-noir-500 uppercase tracking-wider">
                        Operation
                      </p>
                      <div className="mt-1">
                        <OperationBadge operation={normalizeOperation(result.event.operation)} />
                      </div>
                    </div>
                  )}

                  <div>
                    <p className="text-xs font-medium text-noir-500 uppercase tracking-wider">
                      Match Type
                    </p>
                    <p className="text-noir-300 mt-1">{result.match_type}</p>
                  </div>
                </div>

                <div className="space-y-4">
                  <div>
                    <p className="text-xs font-medium text-noir-500 uppercase tracking-wider">
                      Timestamp
                    </p>
                    <p className="text-noir-300 mt-1">
                      {result.event.timestamp_display || result.event.timestamp}
                    </p>
                    <p className="text-xs text-noir-500">
                      {formatRelativeTime(result.event.timestamp)}
                    </p>
                  </div>

                  {result.event.session_id && (
                    <div>
                      <p className="text-xs font-medium text-noir-500 uppercase tracking-wider">
                        Session
                      </p>
                      <p className="mono-value mt-1">{result.event.session_id}</p>
                    </div>
                  )}

                  {result.event.git_commit_sha && (
                    <div>
                      <p className="text-xs font-medium text-noir-500 uppercase tracking-wider">
                        Git Commit
                      </p>
                      <p className="mono-value mt-1">
                        {result.event.git_commit_sha.slice(0, 8)}
                      </p>
                    </div>
                  )}
                </div>
              </div>

              {/* Diff Summary */}
              {result.event.diff_summary && (
                <div>
                  <p className="text-xs font-medium text-noir-500 uppercase tracking-wider mb-2">
                    Change Summary
                  </p>
                  <div className="code-block">
                    <span className="diff-plus">
                      +{result.event.diff_summary.match(/\+(\d+)/)?.[1] || 0} lines added
                    </span>
                    <br />
                    <span className="diff-minus">
                      -{result.event.diff_summary.match(/-(\d+)/)?.[1] || 0} lines removed
                    </span>
                  </div>
                </div>
              )}
            </div>
          ) : (
            <div className="card-noir p-12 text-center">
              <Target size={48} className="mx-auto mb-4 text-noir-700" />
              <p className="text-noir-400 mb-2">No provenance found</p>
              <p className="text-xs text-noir-600 max-w-md mx-auto">
                No AI-generated changes were found for this line.
                Try adjusting the mode to "Inferred" for heuristic matching,
                or provide the line content for better accuracy.
              </p>
            </div>
          )}
        </motion.div>
      )}

      {/* Help Section */}
      {result === undefined && (
        <motion.div
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ delay: 0.2 }}
          className="card-noir p-6"
        >
          <div className="flex items-start gap-4">
            <div className="p-3 rounded-lg bg-accent-secondary/10">
              <Sparkles size={24} className="text-accent-secondary" />
            </div>
            <div>
              <h3 className="font-semibold text-noir-100 mb-2">
                How Blame Works
              </h3>
              <ul className="space-y-2 text-sm text-noir-400">
                <li className="flex items-start gap-2">
                  <span className="text-confidence-high">•</span>
                  <span>
                    <strong>Strict mode</strong> only returns high-confidence matches
                    with exact content hash verification.
                  </span>
                </li>
                <li className="flex items-start gap-2">
                  <span className="text-confidence-medium">•</span>
                  <span>
                    <strong>Best effort mode</strong> includes medium-confidence matches
                    using context and session correlation.
                  </span>
                </li>
                <li className="flex items-start gap-2">
                  <span className="text-confidence-low">•</span>
                  <span>
                    <strong>Inferred mode</strong> uses semantic similarity and file-path
                    heuristics when no direct match exists.
                  </span>
                </li>
              </ul>
            </div>
          </div>
        </motion.div>
      )}
    </div>
  );
}
