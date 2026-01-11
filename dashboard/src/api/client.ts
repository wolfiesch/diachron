// API client for Diachron dashboard
// Makes requests to the Node.js proxy server

import type {
  StoredEvent,
  SearchResult,
  BlameMatch,
  DiagnosticInfo,
  EvidencePackResult,
  MaintenanceStats,
  Session,
  SessionDetail,
  HealthStatus,
} from '@/types/diachron';

const API_BASE = '/api';

class ApiError extends Error {
  constructor(
    public status: number,
    message: string
  ) {
    super(message);
    this.name = 'ApiError';
  }
}

async function fetchApi<T>(path: string, options?: RequestInit): Promise<T> {
  const response = await fetch(`${API_BASE}${path}`, {
    headers: {
      'Content-Type': 'application/json',
    },
    ...options,
  });

  if (!response.ok) {
    const error = await response.json().catch(() => ({ message: 'Unknown error' }));
    throw new ApiError(response.status, error.message || error.error || 'Request failed');
  }

  return response.json();
}

// Health check
export async function getHealth(): Promise<HealthStatus> {
  return fetchApi<HealthStatus>('/health');
}

// Diagnostics
export async function getDiagnostics(): Promise<DiagnosticInfo> {
  return fetchApi<DiagnosticInfo>('/diagnostics');
}

// Timeline events
export interface TimelineParams {
  since?: string;
  file?: string;
  limit?: number;
}

export async function getEvents(params?: TimelineParams): Promise<StoredEvent[]> {
  const searchParams = new URLSearchParams();
  if (params?.since) searchParams.set('since', params.since);
  if (params?.file) searchParams.set('file', params.file);
  if (params?.limit) searchParams.set('limit', params.limit.toString());

  const query = searchParams.toString();
  return fetchApi<StoredEvent[]>(`/events${query ? `?${query}` : ''}`);
}

// Get single event
export async function getEvent(id: number): Promise<StoredEvent> {
  return fetchApi<StoredEvent>(`/events/${id}`);
}

// Sessions
export async function getSessions(): Promise<Session[]> {
  return fetchApi<Session[]>('/sessions');
}

export async function getSession(id: string): Promise<SessionDetail> {
  return fetchApi<SessionDetail>(`/sessions/${id}`);
}

// Search
export interface SearchParams {
  query: string;
  limit?: number;
  source_filter?: 'event' | 'exchange' | null;
  since?: string;
  project?: string;
}

export async function search(params: SearchParams): Promise<SearchResult[]> {
  return fetchApi<SearchResult[]>('/search', {
    method: 'POST',
    body: JSON.stringify(params),
  });
}

// Blame
export interface BlameParams {
  file_path: string;
  line_number: number;
  content?: string;
  context?: string;
  mode?: 'strict' | 'best-effort' | 'inferred';
}

export async function blame(params: BlameParams): Promise<BlameMatch | null> {
  try {
    return await fetchApi<BlameMatch>('/blame', {
      method: 'POST',
      body: JSON.stringify(params),
    });
  } catch (err) {
    if (err instanceof ApiError && err.status === 404) {
      return null;
    }
    throw err;
  }
}

// Evidence
export interface EvidenceParams {
  pr_id: number;
  branch?: string;
  time_range?: string;
  commits?: string[];
  start_time?: string;
  end_time?: string;
  intent?: string;
}

export async function generateEvidence(params: EvidenceParams): Promise<EvidencePackResult> {
  return fetchApi<EvidencePackResult>(`/evidence/${params.pr_id}/generate`, {
    method: 'POST',
    body: JSON.stringify({
      branch: params.branch,
      time_range: params.time_range,
      commits: params.commits,
      start_time: params.start_time,
      end_time: params.end_time,
      intent: params.intent,
    }),
  });
}

// Maintenance
export async function runMaintenance(retentionDays?: number): Promise<MaintenanceStats> {
  return fetchApi<MaintenanceStats>('/maintenance', {
    method: 'POST',
    body: JSON.stringify({ retention_days: retentionDays || 0 }),
  });
}

// WebSocket connection for real-time events
export function connectEventStream(
  onEvent: (events: StoredEvent[]) => void,
  onError?: (error: Event) => void
): WebSocket {
  const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
  const ws = new WebSocket(`${protocol}//${window.location.host}/ws/events`);

  ws.onmessage = (event) => {
    try {
      const data = JSON.parse(event.data);
      if (data.type === 'new_events' && data.events) {
        onEvent(data.events);
      }
    } catch {
      console.error('Failed to parse WebSocket message');
    }
  };

  ws.onerror = (error) => {
    console.error('WebSocket error:', error);
    onError?.(error);
  };

  return ws;
}
