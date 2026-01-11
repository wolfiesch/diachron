import express, { Request, Response, NextFunction } from 'express';
import cors from 'cors';
import { WebSocketServer, WebSocket } from 'ws';
import http from 'http';
import path from 'path';
import { fileURLToPath } from 'url';

import {
  pingDaemon,
  getDiagnostics,
  queryTimeline,
  search,
  blameByFingerprint,
  correlateEvidence,
  runMaintenance,
  StoredEvent,
} from './socket-client.js';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const PORT = process.env.PORT || 3947;

const app = express();
app.use(cors());
app.use(express.json());

// Serve static files in production
app.use(express.static(path.join(__dirname, '../dist')));

// Error wrapper for async handlers
const asyncHandler = (fn: (req: Request, res: Response, next: NextFunction) => Promise<void>) =>
  (req: Request, res: Response, next: NextFunction) => {
    Promise.resolve(fn(req, res, next)).catch(next);
  };

// Health check
app.get('/api/health', asyncHandler(async (_req, res) => {
  try {
    const pong = await pingDaemon();
    res.json({
      status: 'ok',
      daemon: 'connected',
      uptime_secs: pong.uptime_secs,
      events_count: pong.events_count,
    });
  } catch (err) {
    res.status(503).json({
      status: 'error',
      daemon: 'disconnected',
      message: (err as Error).message,
    });
  }
}));

// Diagnostics
app.get('/api/diagnostics', asyncHandler(async (_req, res) => {
  const diagnostics = await getDiagnostics();
  res.json(diagnostics);
}));

// Timeline events
app.get('/api/events', asyncHandler(async (req, res) => {
  const { since, file, limit } = req.query;
  const events = await queryTimeline({
    since: since as string | undefined,
    file_filter: file as string | undefined,
    limit: limit ? parseInt(limit as string, 10) : 100,
  });
  res.json(events);
}));

// Get single event by ID
app.get('/api/events/:id', asyncHandler(async (req, res) => {
  const eventId = parseInt(req.params.id as string, 10);
  const events = await queryTimeline({ limit: 1000 });
  const event = events.find((e) => e.id === eventId);

  if (event) {
    res.json(event);
  } else {
    res.status(404).json({ error: 'Event not found' });
  }
}));

// Sessions (aggregated from events)
app.get('/api/sessions', asyncHandler(async (_req, res) => {
  const events = await queryTimeline({ limit: 1000 });

  // Group events by session_id
  const sessionMap = new Map<string, StoredEvent[]>();
  for (const event of events) {
    const sid = event.session_id || 'unknown';
    if (!sessionMap.has(sid)) {
      sessionMap.set(sid, []);
    }
    sessionMap.get(sid)!.push(event);
  }

  // Convert to session summaries
  const sessions = Array.from(sessionMap.entries()).map(([session_id, sessionEvents]) => {
    const sorted = sessionEvents.sort((a, b) =>
      new Date(a.timestamp).getTime() - new Date(b.timestamp).getTime()
    );
    return {
      session_id,
      event_count: sessionEvents.length,
      first_event: sorted[0]?.timestamp,
      last_event: sorted[sorted.length - 1]?.timestamp,
      tools: [...new Set(sessionEvents.map((e) => e.tool_name))],
      files: [...new Set(sessionEvents.filter((e) => e.file_path).map((e) => e.file_path))],
    };
  });

  // Sort by last event (most recent first)
  sessions.sort((a, b) =>
    new Date(b.last_event || 0).getTime() - new Date(a.last_event || 0).getTime()
  );

  res.json(sessions);
}));

// Get single session details
app.get('/api/sessions/:id', asyncHandler(async (req, res) => {
  const sessionId = req.params.id;
  const events = await queryTimeline({ limit: 1000 });

  const sessionEvents = events.filter((e) =>
    (e.session_id || 'unknown') === sessionId
  );

  if (sessionEvents.length === 0) {
    res.status(404).json({ error: 'Session not found' });
    return;
  }

  const sorted = sessionEvents.sort((a, b) =>
    new Date(a.timestamp).getTime() - new Date(b.timestamp).getTime()
  );

  res.json({
    session_id: sessionId,
    event_count: sessionEvents.length,
    first_event: sorted[0]?.timestamp,
    last_event: sorted[sorted.length - 1]?.timestamp,
    tools: [...new Set(sessionEvents.map((e) => e.tool_name))],
    files: [...new Set(sessionEvents.filter((e) => e.file_path).map((e) => e.file_path))],
    events: sorted,
  });
}));

// Search
app.post('/api/search', asyncHandler(async (req, res) => {
  const { query, limit, source_filter, since, project } = req.body;

  if (!query) {
    res.status(400).json({ error: 'Query is required' });
    return;
  }

  const results = await search({
    query,
    limit: limit || 20,
    source_filter: source_filter || null,
    since: since || null,
    project: project || null,
  });

  res.json(results);
}));

// Blame
app.post('/api/blame', asyncHandler(async (req, res) => {
  const { file_path, line_number, content, context, mode } = req.body;

  if (!file_path || line_number === undefined) {
    res.status(400).json({ error: 'file_path and line_number are required' });
    return;
  }

  const result = await blameByFingerprint({
    file_path,
    line_number,
    content: content || '',
    context: context || '',
    mode: mode || 'best-effort',
  });

  if (result) {
    res.json(result);
  } else {
    res.status(404).json({ error: 'No blame match found' });
  }
}));

// Evidence pack generation
app.post('/api/evidence/:prId/generate', asyncHandler(async (req, res) => {
  const prId = parseInt(req.params.prId as string, 10);
  const { commits, branch, start_time, end_time, intent } = req.body;

  if (!commits || !branch || !start_time || !end_time) {
    res.status(400).json({ error: 'commits, branch, start_time, and end_time are required' });
    return;
  }

  const evidence = await correlateEvidence({
    pr_id: prId,
    commits,
    branch,
    start_time,
    end_time,
    intent: intent || undefined,
  });

  res.json(evidence);
}));

// Maintenance
app.post('/api/maintenance', asyncHandler(async (req, res) => {
  const { retention_days } = req.body;
  const stats = await runMaintenance(retention_days || 0);
  res.json(stats);
}));

// Error handler
app.use((err: Error, _req: Request, res: Response, _next: NextFunction) => {
  console.error('API Error:', err.message);
  res.status(500).json({
    error: 'Internal server error',
    message: err.message,
  });
});

// Fallback to index.html for SPA routing
app.get('*', (_req, res) => {
  res.sendFile(path.join(__dirname, '../dist/index.html'));
});

// Create HTTP server
const server = http.createServer(app);

// WebSocket server for real-time updates
const wss = new WebSocketServer({ server, path: '/ws/events' });

// Store connected clients
const clients = new Set<WebSocket>();

wss.on('connection', (ws) => {
  clients.add(ws);
  console.log('WebSocket client connected');

  ws.on('close', () => {
    clients.delete(ws);
    console.log('WebSocket client disconnected');
  });

  ws.on('error', (err) => {
    console.error('WebSocket error:', err);
    clients.delete(ws);
  });
});

// Poll for new events and broadcast to WebSocket clients
let lastEventId = 0;

async function pollForEvents() {
  try {
    const events = await queryTimeline({ limit: 10 });
    const newEvents = events.filter((e) => e.id > lastEventId);

    if (newEvents.length > 0) {
      lastEventId = Math.max(...newEvents.map((e) => e.id));

      const message = JSON.stringify({
        type: 'new_events',
        events: newEvents,
      });

      for (const client of clients) {
        if (client.readyState === WebSocket.OPEN) {
          client.send(message);
        }
      }
    }
  } catch {
    // Daemon might be unavailable, silently continue
  }
}

// Poll every 2 seconds
setInterval(pollForEvents, 2000);

// Start server
server.listen(PORT, () => {
  console.log(`
╔═══════════════════════════════════════════════════════════╗
║                                                           ║
║   ▄▀▀▀▄  ▀▀█▀▀  ▄▀▀▀▄  ▄▀▀▀▀▄  █   █  █▀▀▀▄  ▄▀▀▀▄  █▄  █ ║
║   █   █    █    █▀▀▀█  █       █▀▀▀█  █▀▀▀▄  █   █  █ ▀▄█ ║
║   ▀▄▄▄▀  ▄▄█▄▄  █   █  ▀▄▄▄▄▀  █   █  █   █  ▀▄▄▄▀  █   █ ║
║                                                           ║
║   Dashboard running at http://localhost:${PORT}             ║
║   WebSocket events at ws://localhost:${PORT}/ws/events      ║
║                                                           ║
╚═══════════════════════════════════════════════════════════╝
  `);
});
