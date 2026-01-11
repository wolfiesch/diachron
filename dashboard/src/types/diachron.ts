// Core Diachron types matching the Rust daemon
// These mirror the types in rust/core/src/types.rs

export interface StoredEvent {
  id: number;
  timestamp: string;
  timestamp_display?: string;
  session_id?: string;
  tool_name: string;
  file_path?: string;
  operation?: string;
  diff_summary?: string;
  raw_input?: string;
  ai_summary?: string;
  git_commit_sha?: string;
  metadata?: string;
}

export interface SearchResult {
  id: string;
  score: number;
  source: 'event' | 'exchange';
  snippet: string;
  timestamp: string;
  project?: string;
}

export type ConfidenceLevel = 'HIGH' | 'MEDIUM' | 'LOW' | 'INFERRED';

export interface BlameMatch {
  event: StoredEvent;
  confidence: ConfidenceLevel;
  match_type: string;
  similarity: number;
  intent?: string;
}

export interface DiagnosticInfo {
  uptime_secs: number;
  events_count: number;
  exchanges_count: number;
  events_index_count: number;
  exchanges_index_count: number;
  database_size_bytes: number;
  events_index_size_bytes: number;
  exchanges_index_size_bytes: number;
  model_loaded: boolean;
  model_size_bytes: number;
  memory_rss_bytes: number;
}

export interface EvidencePackResult {
  pr_id: number;
  generated_at: string;
  diachron_version: string;
  branch: string;
  summary: EvidenceSummary;
  commits: CommitEvidence[];
  verification: VerificationStatus;
  intent?: string;
  coverage_pct: number;
  unmatched_count: number;
  total_events: number;
}

export interface EvidenceSummary {
  files_changed: number;
  lines_added: number;
  lines_removed: number;
  tool_operations: number;
  sessions: number;
}

export interface CommitEvidence {
  sha: string;
  message: string;
  events: StoredEvent[];
  confidence: ConfidenceLevel;
}

export interface VerificationStatus {
  chain_verified: boolean;
  tests_passed: boolean | null;
  build_succeeded: boolean | null;
  human_reviewed: boolean;
}

export interface MaintenanceStats {
  size_before: number;
  size_after: number;
  events_pruned: number;
  exchanges_pruned: number;
  duration_ms: number;
}

export interface Session {
  session_id: string;
  event_count: number;
  first_event: string;
  last_event: string;
  tools: string[];
  files: (string | undefined)[];
}

export interface SessionDetail extends Session {
  events: StoredEvent[];
}

export interface HealthStatus {
  status: 'ok' | 'error';
  daemon: 'connected' | 'disconnected';
  uptime_secs?: number;
  events_count?: number;
  message?: string;
}

// Operation type for styling
export type Operation = 'create' | 'modify' | 'delete' | 'commit' | 'execute' | 'move' | 'copy' | 'unknown';

// Tool type for styling
export type ToolName = 'Claude' | 'Codex' | 'Aider' | 'Cursor' | 'Write' | 'Edit' | 'Bash' | string;

// Helper to normalize operation strings
export function normalizeOperation(op?: string): Operation {
  if (!op) return 'unknown';
  const lower = op.toLowerCase();
  if (['create', 'modify', 'delete', 'commit', 'execute', 'move', 'copy'].includes(lower)) {
    return lower as Operation;
  }
  return 'unknown';
}

// Helper to get tool color class
export function getToolColorClass(tool: string): string {
  const lower = tool.toLowerCase();
  if (lower.includes('claude') || lower === 'write' || lower === 'edit') {
    return 'text-tool-claude';
  }
  if (lower.includes('codex')) {
    return 'text-tool-codex';
  }
  if (lower.includes('aider')) {
    return 'text-tool-aider';
  }
  if (lower.includes('cursor')) {
    return 'text-tool-cursor';
  }
  if (lower === 'bash') {
    return 'text-op-execute';
  }
  return 'text-noir-400';
}

// Helper to get confidence color class
export function getConfidenceColorClass(confidence: ConfidenceLevel): string {
  switch (confidence) {
    case 'HIGH':
      return 'text-confidence-high';
    case 'MEDIUM':
      return 'text-confidence-medium';
    case 'LOW':
      return 'text-confidence-low';
    case 'INFERRED':
      return 'text-confidence-inferred';
    default:
      return 'text-noir-400';
  }
}

// Helper to get operation color class
export function getOperationColorClass(op: Operation): string {
  switch (op) {
    case 'create':
      return 'text-op-create';
    case 'modify':
      return 'text-op-modify';
    case 'delete':
      return 'text-op-delete';
    case 'commit':
      return 'text-op-commit';
    case 'execute':
      return 'text-op-execute';
    default:
      return 'text-noir-400';
  }
}

// Format bytes to human readable
export function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`;
}

// Format duration in seconds to human readable
export function formatDuration(secs: number): string {
  if (secs < 60) return `${secs}s`;
  if (secs < 3600) return `${Math.floor(secs / 60)}m ${secs % 60}s`;
  const hours = Math.floor(secs / 3600);
  const mins = Math.floor((secs % 3600) / 60);
  return `${hours}h ${mins}m`;
}

// Format relative time
export function formatRelativeTime(timestamp: string): string {
  const date = new Date(timestamp);
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffSecs = Math.floor(diffMs / 1000);
  const diffMins = Math.floor(diffSecs / 60);
  const diffHours = Math.floor(diffMins / 60);
  const diffDays = Math.floor(diffHours / 24);

  if (diffSecs < 60) return 'just now';
  if (diffMins < 60) return `${diffMins}m ago`;
  if (diffHours < 24) return `${diffHours}h ago`;
  if (diffDays < 7) return `${diffDays}d ago`;

  return date.toLocaleDateString();
}
