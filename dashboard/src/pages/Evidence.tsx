import { useState, useEffect } from 'react';
import { useParams, Link } from 'react-router-dom';
import { motion } from 'framer-motion';
import {
  FileStack,
  GitCommit,
  CheckCircle,
  XCircle,
  Hash,
  Download,
  Copy,
  ChevronLeft,
  Shield,
  AlertTriangle,
} from 'lucide-react';
import { useEvidence } from '@/api/hooks';
import { ConfidenceBadge } from '@/components/shared/ConfidenceBadge';
import { ToolBadge } from '@/components/shared/ToolBadge';
import { OperationBadge } from '@/components/shared/OperationIcon';
import { formatRelativeTime, normalizeOperation, type EvidencePackResult } from '@/types/diachron';
import { cn } from '@/lib/utils';

function EvidenceInput() {
  const [prId, setPrId] = useState('');
  const [branch, setBranch] = useState('');
  const [timeRange, setTimeRange] = useState('24h');
  const [result, setResult] = useState<EvidencePackResult | null>(null);
  const [copied, setCopied] = useState(false);

  const { mutate: generateEvidence, isPending } = useEvidence();

  const handleGenerate = () => {
    if (!prId) return;

    generateEvidence(
      {
        pr_id: parseInt(prId, 10),
        branch: branch || undefined,
        time_range: timeRange,
      },
      {
        onSuccess: (data) => {
          setResult(data);
        },
      }
    );
  };

  const handleCopyMarkdown = () => {
    if (!result) return;
    // Generate markdown from result
    const markdown = generateMarkdown(result);
    navigator.clipboard.writeText(markdown);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const handleDownloadJson = () => {
    if (!result) return;
    const blob = new Blob([JSON.stringify(result, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `diachron-evidence-pr-${prId}.json`;
    a.click();
    URL.revokeObjectURL(url);
  };

  return (
    <div className="space-y-6">
      {/* Header */}
      <motion.div
        initial={{ opacity: 0, y: -20 }}
        animate={{ opacity: 1, y: 0 }}
      >
        <h1 className="text-3xl font-display font-bold text-noir-100">
          Evidence Packs
        </h1>
        <p className="text-noir-400 mt-1">
          Generate AI provenance evidence for pull requests
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
          {/* PR ID */}
          <div>
            <label className="block text-xs font-medium text-noir-500 uppercase tracking-wider mb-2">
              PR Number <span className="text-op-delete">*</span>
            </label>
            <div className="relative">
              <Hash
                size={16}
                className="absolute left-3 top-1/2 -translate-y-1/2 text-noir-500"
              />
              <input
                type="number"
                placeholder="142"
                value={prId}
                onChange={(e) => setPrId(e.target.value)}
                className="input-noir pl-10"
                min="1"
              />
            </div>
          </div>

          {/* Branch */}
          <div>
            <label className="block text-xs font-medium text-noir-500 uppercase tracking-wider mb-2">
              Branch <span className="text-noir-600">(Optional)</span>
            </label>
            <div className="relative">
              <GitCommit
                size={16}
                className="absolute left-3 top-1/2 -translate-y-1/2 text-noir-500"
              />
              <input
                type="text"
                placeholder="feature/auth"
                value={branch}
                onChange={(e) => setBranch(e.target.value)}
                className="input-noir pl-10"
              />
            </div>
          </div>

          {/* Time Range */}
          <div>
            <label className="block text-xs font-medium text-noir-500 uppercase tracking-wider mb-2">
              Time Range
            </label>
            <select
              value={timeRange}
              onChange={(e) => setTimeRange(e.target.value)}
              className="input-noir"
            >
              <option value="24h">Last 24 hours</option>
              <option value="7d">Last 7 days</option>
              <option value="30d">Last 30 days</option>
              <option value="all">All time</option>
            </select>
          </div>
        </div>

        <div className="flex justify-end">
          <button
            onClick={handleGenerate}
            disabled={!prId || isPending}
            className="btn btn-primary"
          >
            {isPending ? (
              <span className="flex items-center gap-2">
                <span className="w-4 h-4 border-2 border-white/30 border-t-white rounded-full animate-spin" />
                Generating...
              </span>
            ) : (
              <span className="flex items-center gap-2">
                <FileStack size={16} />
                Generate Evidence Pack
              </span>
            )}
          </button>
        </div>
      </motion.div>

      {/* Result */}
      {result && (
        <EvidencePackView
          result={result}
          onCopyMarkdown={handleCopyMarkdown}
          onDownloadJson={handleDownloadJson}
          copied={copied}
        />
      )}

      {/* Help Section */}
      {!result && (
        <motion.div
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ delay: 0.2 }}
          className="card-noir p-6"
        >
          <div className="flex items-start gap-4">
            <div className="p-3 rounded-lg bg-accent-secondary/10">
              <Shield size={24} className="text-accent-secondary" />
            </div>
            <div>
              <h3 className="font-semibold text-noir-100 mb-2">
                What's in an Evidence Pack?
              </h3>
              <ul className="space-y-2 text-sm text-noir-400">
                <li className="flex items-start gap-2">
                  <span className="text-confidence-high">•</span>
                  <span>
                    <strong>PR Summary</strong> — Files changed, lines added/removed, AI coverage percentage
                  </span>
                </li>
                <li className="flex items-start gap-2">
                  <span className="text-confidence-medium">•</span>
                  <span>
                    <strong>Commit Evidence</strong> — Each commit linked to AI events with confidence scores
                  </span>
                </li>
                <li className="flex items-start gap-2">
                  <span className="text-accent-primary">•</span>
                  <span>
                    <strong>Intent Trail</strong> — User prompts that motivated each change
                  </span>
                </li>
                <li className="flex items-start gap-2">
                  <span className="text-accent-secondary">•</span>
                  <span>
                    <strong>Verification Status</strong> — Hash chain integrity and test results
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

interface EvidencePackViewProps {
  result: EvidencePackResult;
  onCopyMarkdown: () => void;
  onDownloadJson: () => void;
  copied: boolean;
}

function EvidencePackView({ result, onCopyMarkdown, onDownloadJson, copied }: EvidencePackViewProps) {
  return (
    <motion.div
      initial={{ opacity: 0, y: 20 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ delay: 0.2 }}
      className="space-y-6"
    >
      {/* Header Card */}
      <div className="card-noir p-6">
        <div className="flex items-start justify-between mb-6">
          <div>
            <div className="flex items-center gap-3 mb-2">
              <h2 className="text-2xl font-display font-bold text-noir-100">
                PR #{result.pr_id}
              </h2>
              <span
                className={cn(
                  'badge',
                  result.coverage_pct >= 80
                    ? 'bg-confidence-high/20 text-confidence-high'
                    : result.coverage_pct >= 50
                    ? 'bg-confidence-medium/20 text-confidence-medium'
                    : 'bg-confidence-low/20 text-confidence-low'
                )}
              >
                {result.coverage_pct.toFixed(0)}% AI Coverage
              </span>
            </div>
            <p className="text-sm text-noir-500">
              Generated {formatRelativeTime(result.generated_at)}
            </p>
          </div>
          <div className="flex items-center gap-2">
            <button
              onClick={onCopyMarkdown}
              className="btn btn-secondary"
            >
              {copied ? (
                <span className="flex items-center gap-2 text-confidence-high">
                  <CheckCircle size={16} />
                  Copied!
                </span>
              ) : (
                <span className="flex items-center gap-2">
                  <Copy size={16} />
                  Copy Markdown
                </span>
              )}
            </button>
            <button
              onClick={onDownloadJson}
              className="btn btn-secondary"
            >
              <Download size={16} />
              JSON
            </button>
          </div>
        </div>

        {/* Intent */}
        {result.intent && (
          <div className="bg-noir-800/50 rounded-lg p-4 border-l-2 border-accent-primary mb-6">
            <p className="text-xs font-medium text-noir-500 uppercase tracking-wider mb-2">
              Intent
            </p>
            <p className="text-noir-200 italic">"{result.intent}"</p>
          </div>
        )}

        {/* Summary Stats */}
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
          <div className="bg-noir-800/30 rounded-lg p-4">
            <p className="text-xs text-noir-500 uppercase tracking-wider">Files Changed</p>
            <p className="text-2xl font-display font-semibold text-noir-100 mt-1 tabular-nums">
              {result.summary.files_changed}
            </p>
          </div>
          <div className="bg-noir-800/30 rounded-lg p-4">
            <p className="text-xs text-noir-500 uppercase tracking-wider">Lines Added</p>
            <p className="text-2xl font-display font-semibold text-confidence-high mt-1 tabular-nums">
              +{result.summary.lines_added}
            </p>
          </div>
          <div className="bg-noir-800/30 rounded-lg p-4">
            <p className="text-xs text-noir-500 uppercase tracking-wider">Lines Removed</p>
            <p className="text-2xl font-display font-semibold text-op-delete mt-1 tabular-nums">
              -{result.summary.lines_removed}
            </p>
          </div>
          <div className="bg-noir-800/30 rounded-lg p-4">
            <p className="text-xs text-noir-500 uppercase tracking-wider">AI Events</p>
            <p className="text-2xl font-display font-semibold text-accent-primary mt-1 tabular-nums">
              {result.total_events}
            </p>
          </div>
        </div>
      </div>

      {/* Verification Status */}
      <div className="card-noir p-6">
        <div className="flex items-center gap-3 mb-5">
          <div className="p-2 rounded-lg bg-accent-secondary/10">
            <Shield size={18} className="text-accent-secondary" />
          </div>
          <h3 className="font-semibold text-noir-100">Verification Status</h3>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
          <div className="flex items-center gap-3">
            {result.verification.chain_verified ? (
              <CheckCircle size={20} className="text-confidence-high" />
            ) : (
              <XCircle size={20} className="text-op-delete" />
            )}
            <div>
              <p className="text-sm font-medium text-noir-200">Hash Chain</p>
              <p className="text-xs text-noir-500">
                {result.verification.chain_verified ? 'Verified' : 'Invalid'}
              </p>
            </div>
          </div>
          <div className="flex items-center gap-3">
            {result.verification.tests_passed ? (
              <CheckCircle size={20} className="text-confidence-high" />
            ) : result.verification.tests_passed === false ? (
              <XCircle size={20} className="text-op-delete" />
            ) : (
              <AlertTriangle size={20} className="text-confidence-medium" />
            )}
            <div>
              <p className="text-sm font-medium text-noir-200">Tests</p>
              <p className="text-xs text-noir-500">
                {result.verification.tests_passed === true
                  ? 'Passed'
                  : result.verification.tests_passed === false
                  ? 'Failed'
                  : 'Not run'}
              </p>
            </div>
          </div>
          <div className="flex items-center gap-3">
            {result.verification.build_succeeded ? (
              <CheckCircle size={20} className="text-confidence-high" />
            ) : result.verification.build_succeeded === false ? (
              <XCircle size={20} className="text-op-delete" />
            ) : (
              <AlertTriangle size={20} className="text-confidence-medium" />
            )}
            <div>
              <p className="text-sm font-medium text-noir-200">Build</p>
              <p className="text-xs text-noir-500">
                {result.verification.build_succeeded === true
                  ? 'Passed'
                  : result.verification.build_succeeded === false
                  ? 'Failed'
                  : 'Not run'}
              </p>
            </div>
          </div>
        </div>
      </div>

      {/* Commit Evidence */}
      <div className="card-noir">
        <div className="p-5 border-b border-noir-800">
          <div className="flex items-center gap-3">
            <div className="p-2 rounded-lg bg-op-commit/10">
              <GitCommit size={18} className="text-op-commit" />
            </div>
            <h3 className="font-semibold text-noir-100">Commit Evidence</h3>
            <span className="badge bg-noir-700 text-noir-300">
              {result.commits.length} commits
            </span>
          </div>
        </div>

        <div className="divide-y divide-noir-800/50">
          {result.commits.map((commit, index) => (
            <motion.div
              key={commit.sha}
              initial={{ opacity: 0, x: -10 }}
              animate={{ opacity: 1, x: 0 }}
              transition={{ delay: 0.05 * index }}
              className="p-5"
            >
              <div className="flex items-start justify-between mb-3">
                <div>
                  <div className="flex items-center gap-2 mb-1">
                    <span className="mono-value">{commit.sha.slice(0, 8)}</span>
                    <ConfidenceBadge confidence={commit.confidence} />
                  </div>
                  <p className="text-noir-200">{commit.message}</p>
                </div>
              </div>

              {commit.events.length > 0 && (
                <div className="mt-3 pl-4 border-l-2 border-noir-700 space-y-2">
                  {commit.events.slice(0, 5).map((event) => (
                    <div
                      key={event.id}
                      className="flex items-center gap-3 text-sm"
                    >
                      <ToolBadge tool={event.tool_name} showIcon={false} />
                      {event.operation && (
                        <OperationBadge operation={normalizeOperation(event.operation)} />
                      )}
                      {event.file_path && (
                        <span className="file-path truncate">{event.file_path}</span>
                      )}
                    </div>
                  ))}
                  {commit.events.length > 5 && (
                    <p className="text-xs text-noir-500">
                      +{commit.events.length - 5} more events
                    </p>
                  )}
                </div>
              )}
            </motion.div>
          ))}
        </div>
      </div>
    </motion.div>
  );
}

function EvidenceDetail() {
  const { prId } = useParams<{ prId: string }>();
  const { mutate: generateEvidence, isPending, data: result } = useEvidence();

  // Auto-generate on mount
  useEffect(() => {
    if (prId) {
      generateEvidence({ pr_id: parseInt(prId, 10) });
    }
  }, [prId, generateEvidence]);

  if (isPending) {
    return (
      <div className="space-y-6">
        <div className="flex items-center gap-4">
          <Link
            to="/evidence"
            className="btn btn-ghost"
          >
            <ChevronLeft size={16} />
            Back
          </Link>
          <div className="skeleton h-8 w-48" />
        </div>
        <div className="card-noir p-6 space-y-4">
          {Array.from({ length: 4 }).map((_, i) => (
            <div key={i} className="skeleton h-16 w-full" />
          ))}
        </div>
      </div>
    );
  }

  if (!result) {
    return (
      <div className="space-y-6">
        <Link
          to="/evidence"
          className="inline-flex items-center gap-1 text-sm text-noir-400 hover:text-noir-100"
        >
          <ChevronLeft size={16} />
          Back to Evidence Packs
        </Link>
        <div className="card-noir p-12 text-center">
          <FileStack size={48} className="mx-auto mb-4 text-noir-700" />
          <p className="text-noir-400 mb-2">No evidence found for PR #{prId}</p>
          <p className="text-xs text-noir-600">
            Make sure the PR exists and has AI-generated changes
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <Link
        to="/evidence"
        className="inline-flex items-center gap-1 text-sm text-noir-400 hover:text-noir-100"
      >
        <ChevronLeft size={16} />
        Back to Evidence Packs
      </Link>
      <EvidencePackView
        result={result}
        onCopyMarkdown={() => {
          const md = generateMarkdown(result);
          navigator.clipboard.writeText(md);
        }}
        onDownloadJson={() => {
          const blob = new Blob([JSON.stringify(result, null, 2)], { type: 'application/json' });
          const url = URL.createObjectURL(blob);
          const a = document.createElement('a');
          a.href = url;
          a.download = `diachron-evidence-pr-${prId}.json`;
          a.click();
          URL.revokeObjectURL(url);
        }}
        copied={false}
      />
    </div>
  );
}

function generateMarkdown(result: EvidencePackResult): string {
  const lines: string[] = [
    `# AI Provenance Evidence - PR #${result.pr_id}`,
    '',
    `> Generated by Diachron v${result.diachron_version} on ${result.generated_at}`,
    '',
  ];

  if (result.intent) {
    lines.push(`## Intent`, '', `> ${result.intent}`, '');
  }

  lines.push(
    `## Summary`,
    '',
    `| Metric | Value |`,
    `|--------|-------|`,
    `| Files Changed | ${result.summary.files_changed} |`,
    `| Lines Added | +${result.summary.lines_added} |`,
    `| Lines Removed | -${result.summary.lines_removed} |`,
    `| AI Events | ${result.total_events} |`,
    `| Coverage | ${result.coverage_pct.toFixed(1)}% |`,
    ''
  );

  lines.push(
    `## Verification`,
    '',
    `- Hash Chain: ${result.verification.chain_verified ? '✅ Valid' : '❌ Invalid'}`,
    `- Tests: ${result.verification.tests_passed === true ? '✅ Passed' : result.verification.tests_passed === false ? '❌ Failed' : '⚠️ Not run'}`,
    `- Build: ${result.verification.build_succeeded === true ? '✅ Passed' : result.verification.build_succeeded === false ? '❌ Failed' : '⚠️ Not run'}`,
    ''
  );

  lines.push(`## Commits`, '');

  for (const commit of result.commits) {
    lines.push(
      `### \`${commit.sha.slice(0, 8)}\` - ${commit.message}`,
      '',
      `**Confidence:** ${commit.confidence}`,
      ''
    );

    if (commit.events.length > 0) {
      lines.push('| Tool | Operation | File |', '|------|-----------|------|');
      for (const event of commit.events.slice(0, 10)) {
        lines.push(
          `| ${event.tool_name} | ${event.operation || '-'} | ${event.file_path || '-'} |`
        );
      }
      if (commit.events.length > 10) {
        lines.push(``, `*+${commit.events.length - 10} more events*`);
      }
      lines.push('');
    }
  }

  lines.push('---', '', '*Generated by [Diachron](https://github.com/wolfgangschoenberger/diachron)*');

  return lines.join('\n');
}

export function EvidencePage() {
  const { prId } = useParams<{ prId: string }>();
  return prId ? <EvidenceDetail /> : <EvidenceInput />;
}
