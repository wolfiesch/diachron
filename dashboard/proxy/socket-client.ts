import * as net from 'net';
import * as os from 'os';
import * as path from 'path';

const SOCKET_PATH = path.join(os.homedir(), '.diachron', 'diachron.sock');
const CONNECT_TIMEOUT = 5000;
const REQUEST_TIMEOUT = 30000;

interface IpcRequest {
  type: string;
  payload?: unknown;
}

interface IpcResponse {
  type: string;
  payload?: unknown;
}

/**
 * Send a message to the Diachron daemon via Unix socket
 * and return the response.
 */
export async function sendToDaemon(message: IpcRequest): Promise<IpcResponse> {
  return new Promise((resolve, reject) => {
    const socket = new net.Socket();
    let responseData = '';
    let connected = false;

    const connectTimeout = setTimeout(() => {
      if (!connected) {
        socket.destroy();
        reject(new Error('Connection timeout: daemon not responding'));
      }
    }, CONNECT_TIMEOUT);

    const requestTimeout = setTimeout(() => {
      socket.destroy();
      reject(new Error('Request timeout: daemon took too long to respond'));
    }, REQUEST_TIMEOUT);

    socket.on('connect', () => {
      connected = true;
      clearTimeout(connectTimeout);

      // Send the message as JSON with newline delimiter
      const jsonMessage = JSON.stringify(message) + '\n';
      socket.write(jsonMessage);
    });

    socket.on('data', (data) => {
      responseData += data.toString();

      // Check for complete JSON response (newline-delimited)
      if (responseData.includes('\n')) {
        try {
          const response = JSON.parse(responseData.trim());
          clearTimeout(requestTimeout);
          socket.destroy();
          resolve(response);
        } catch {
          // Incomplete JSON, wait for more data
        }
      }
    });

    socket.on('error', (err) => {
      clearTimeout(connectTimeout);
      clearTimeout(requestTimeout);

      if ((err as NodeJS.ErrnoException).code === 'ENOENT') {
        reject(new Error('Daemon not running: socket not found at ' + SOCKET_PATH));
      } else if ((err as NodeJS.ErrnoException).code === 'ECONNREFUSED') {
        reject(new Error('Daemon not responding: connection refused'));
      } else {
        reject(err);
      }
    });

    socket.on('close', () => {
      clearTimeout(connectTimeout);
      clearTimeout(requestTimeout);
    });

    // Connect to the Unix socket
    socket.connect(SOCKET_PATH);
  });
}

/**
 * Check if daemon is running and responsive
 */
export async function pingDaemon(): Promise<{ uptime_secs: number; events_count: number }> {
  const response = await sendToDaemon({ type: 'Ping' });

  if (response.type === 'Pong' && response.payload) {
    return response.payload as { uptime_secs: number; events_count: number };
  }

  throw new Error('Unexpected response from daemon');
}

/**
 * Get diagnostic info from daemon
 */
export async function getDiagnostics(): Promise<DiagnosticInfo> {
  const response = await sendToDaemon({ type: 'DoctorInfo' });

  if (response.type === 'Doctor' && response.payload) {
    return response.payload as DiagnosticInfo;
  }

  throw new Error('Failed to get diagnostics');
}

/**
 * Query timeline events
 */
export async function queryTimeline(options: {
  since?: string;
  file_filter?: string;
  limit?: number;
}): Promise<StoredEvent[]> {
  const response = await sendToDaemon({
    type: 'Timeline',
    payload: {
      since: options.since || null,
      file_filter: options.file_filter || null,
      limit: options.limit || 100,
    },
  });

  if (response.type === 'Events' && response.payload) {
    return response.payload as StoredEvent[];
  }

  throw new Error('Failed to query timeline');
}

/**
 * Search events and exchanges
 */
export async function search(options: {
  query: string;
  limit?: number;
  source_filter?: 'event' | 'exchange' | null;
  since?: string;
  project?: string;
}): Promise<SearchResult[]> {
  const response = await sendToDaemon({
    type: 'Search',
    payload: {
      query: options.query,
      limit: options.limit || 20,
      source_filter: options.source_filter || null,
      since: options.since || null,
      project: options.project || null,
    },
  });

  if (response.type === 'SearchResults' && response.payload) {
    return response.payload as SearchResult[];
  }

  throw new Error('Failed to search');
}

/**
 * Blame a specific line in a file
 */
export async function blameByFingerprint(options: {
  file_path: string;
  line_number: number;
  content: string;
  context: string;
  mode?: 'strict' | 'best-effort' | 'inferred';
}): Promise<BlameMatch | null> {
  const response = await sendToDaemon({
    type: 'BlameByFingerprint',
    payload: {
      file_path: options.file_path,
      line_number: options.line_number,
      content: options.content,
      context: options.context,
      mode: options.mode || 'best-effort',
    },
  });

  if (response.type === 'BlameResult' && response.payload) {
    return response.payload as BlameMatch;
  }

  if (response.type === 'BlameNotFound') {
    return null;
  }

  throw new Error('Failed to blame');
}

/**
 * Generate evidence pack for a PR
 */
export async function correlateEvidence(options: {
  pr_id: number;
  commits: string[];
  branch: string;
  start_time: string;
  end_time: string;
  intent?: string;
}): Promise<EvidencePackResult> {
  const response = await sendToDaemon({
    type: 'CorrelateEvidence',
    payload: {
      pr_id: options.pr_id,
      commits: options.commits,
      branch: options.branch,
      start_time: options.start_time,
      end_time: options.end_time,
      intent: options.intent || null,
    },
  });

  if (response.type === 'EvidenceResult' && response.payload) {
    return response.payload as EvidencePackResult;
  }

  throw new Error('Failed to correlate evidence');
}

/**
 * Run database maintenance
 */
export async function runMaintenance(retentionDays: number = 0): Promise<MaintenanceStats> {
  const response = await sendToDaemon({
    type: 'Maintenance',
    payload: { retention_days: retentionDays },
  });

  if (response.type === 'MaintenanceStats' && response.payload) {
    return response.payload as MaintenanceStats;
  }

  throw new Error('Failed to run maintenance');
}

// Type definitions matching Rust types
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

export interface BlameMatch {
  event: StoredEvent;
  confidence: 'HIGH' | 'MEDIUM' | 'LOW' | 'INFERRED';
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
  message?: string;
  events: StoredEvent[];
  confidence: string;
}

export interface VerificationStatus {
  chain_verified: boolean;
  tests_executed: boolean;
  build_succeeded: boolean;
  human_reviewed: boolean;
}

export interface MaintenanceStats {
  size_before: number;
  size_after: number;
  events_pruned: number;
  exchanges_pruned: number;
  duration_ms: number;
}
