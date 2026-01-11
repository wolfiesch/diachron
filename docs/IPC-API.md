# Diachron IPC API

This document describes the inter-process communication (IPC) API between clients and the Diachron daemon (`diachrond`). Use this API to build custom integrations, hooks for other AI assistants (Cursor, Codex, etc.), or tooling for the Diachron ecosystem.

## Overview

The daemon listens on a Unix domain socket and communicates via newline-delimited JSON messages.

### Socket Location

```
~/.diachron/diachron.sock
```

### Protocol

1. Connect to the Unix socket
2. Send a JSON message followed by a newline (`\n`)
3. Read the JSON response (also newline-terminated)
4. Disconnect or send another message

### Message Format

All messages use a tagged enum pattern:

```json
{"type": "MessageType", "payload": { ... }}
```

Responses follow the same pattern:

```json
{"type": "Ok|Error|...", "payload": { ... }}
```

---

## Quick Start

### Python Example

```python
import socket
import json

SOCKET_PATH = "~/.diachron/diachron.sock"

def send_message(msg):
    """Send a message to the Diachron daemon and return the response."""
    sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    sock.connect(os.path.expanduser(SOCKET_PATH))

    # Send message
    sock.sendall((json.dumps(msg) + "\n").encode())

    # Read response
    response = b""
    while not response.endswith(b"\n"):
        response += sock.recv(4096)

    sock.close()
    return json.loads(response.decode())

# Health check
result = send_message({"type": "Ping", "payload": None})
print(f"Daemon uptime: {result['payload']['uptime_secs']}s")
```

### Bash Example (with netcat)

```bash
echo '{"type":"Ping","payload":null}' | nc -U ~/.diachron/diachron.sock
```

---

## Message Types

### Capture (Record Events)

Record a code change event. This is the core function used by hooks.

**Request:**
```json
{
  "type": "Capture",
  "payload": {
    "tool_name": "Cursor",
    "file_path": "/path/to/file.ts",
    "operation": "modify",
    "diff_summary": "+15 lines, -3 lines",
    "raw_input": "original tool input or command",
    "metadata": "{\"branch\": \"feature-x\"}",
    "git_commit_sha": null,
    "command_category": null
  }
}
```

**Fields:**
| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `tool_name` | string | ✓ | Source of the event (e.g., "Claude", "Cursor", "Codex", "VSCode") |
| `file_path` | string | - | Absolute path to the affected file |
| `operation` | string | ✓ | One of: "create", "modify", "delete", "move", "copy", "commit", "execute" |
| `diff_summary` | string | - | Human-readable summary of changes |
| `raw_input` | string | - | Raw tool input for forensics |
| `metadata` | string | - | JSON string with extra context (branch, session_id, etc.) |
| `git_commit_sha` | string | - | If this was a commit operation |
| `command_category` | string | - | For Bash: "git", "test", "build", "deploy", "file_ops", "package" |

**Response:**
```json
{"type": "Ok", "payload": null}
```

---

### Ping (Health Check)

Check if the daemon is running and get uptime.

**Request:**
```json
{"type": "Ping", "payload": null}
```

**Response:**
```json
{
  "type": "Pong",
  "payload": {
    "uptime_secs": 3600,
    "events_count": 1250
  }
}
```

---

### Timeline (Query Events)

Retrieve recent events with optional filtering.

**Request:**
```json
{
  "type": "Timeline",
  "payload": {
    "since": "1h",
    "file_filter": "src/",
    "limit": 50
  }
}
```

**Fields:**
| Field | Type | Description |
|-------|------|-------------|
| `since` | string | Time filter: "1h", "7d", "2026-01-01", ISO timestamp |
| `file_filter` | string | Path prefix filter |
| `limit` | number | Max events to return |

**Response:**
```json
{
  "type": "Events",
  "payload": [
    {
      "id": 1234,
      "timestamp": "2026-01-11T07:30:00Z",
      "timestamp_display": "7:30 AM",
      "session_id": "abc123",
      "tool_name": "Claude",
      "file_path": "/path/to/file.ts",
      "operation": "modify",
      "diff_summary": "+12 lines",
      "raw_input": null,
      "ai_summary": "Added error handling for auth flow",
      "git_commit_sha": null,
      "metadata": null
    }
  ]
}
```

---

### Search (Semantic Search)

Search events and conversations using vector similarity + full-text search.

**Request:**
```json
{
  "type": "Search",
  "payload": {
    "query": "authentication error handling",
    "limit": 10,
    "source_filter": "event",
    "since": "7d",
    "project": "my-project"
  }
}
```

**Fields:**
| Field | Type | Description |
|-------|------|-------------|
| `query` | string | Search query (semantic + keyword) |
| `limit` | number | Max results |
| `source_filter` | string | "event" or "exchange" (null for both) |
| `since` | string | Time filter |
| `project` | string | Project name filter |

**Response:**
```json
{
  "type": "SearchResults",
  "payload": [
    {
      "id": "event:1234",
      "score": 0.92,
      "source": "event",
      "snippet": "Added JWT refresh token handling",
      "timestamp": "2026-01-11T07:30:00Z",
      "project": "my-project"
    }
  ]
}
```

---

### BlameByFingerprint (Semantic Blame)

Find which AI session created a specific line of code.

**Request:**
```json
{
  "type": "BlameByFingerprint",
  "payload": {
    "file_path": "/path/to/file.ts",
    "line_number": 42,
    "content": "const token = await refreshToken(user.id);",
    "context": "// lines 37-47 of the file",
    "mode": "best-effort"
  }
}
```

**Fields:**
| Field | Type | Description |
|-------|------|-------------|
| `file_path` | string | File being blamed |
| `line_number` | number | Line number |
| `content` | string | Current line content |
| `context` | string | Surrounding ±5 lines |
| `mode` | string | "strict", "best-effort", or "inferred" |

**Response (found):**
```json
{
  "type": "BlameResult",
  "payload": {
    "event": { /* StoredEvent object */ },
    "confidence": "HIGH",
    "match_type": "ContentHash",
    "similarity": 0.98,
    "intent": "Fix the 401 errors on page refresh"
  }
}
```

**Response (not found):**
```json
{
  "type": "BlameNotFound",
  "payload": {
    "reason": "No matching fingerprint in database"
  }
}
```

---

### CorrelateEvidence (PR Evidence Pack)

Generate an evidence pack linking events to PR commits.

**Request:**
```json
{
  "type": "CorrelateEvidence",
  "payload": {
    "pr_id": 142,
    "commits": ["abc123", "def456"],
    "branch": "feature-auth",
    "start_time": "2026-01-10T00:00:00Z",
    "end_time": "2026-01-11T23:59:59Z",
    "intent": "Implement OAuth2 authentication"
  }
}
```

**Response:**
```json
{
  "type": "EvidenceResult",
  "payload": {
    "pr_id": 142,
    "generated_at": "2026-01-11T08:00:00Z",
    "diachron_version": "0.6.0",
    "branch": "feature-auth",
    "summary": {
      "files_changed": 8,
      "lines_added": 245,
      "lines_removed": 32,
      "tool_operations": 15,
      "sessions": 2
    },
    "commits": [
      {
        "sha": "abc123",
        "message": "Add OAuth2 login flow",
        "events": [ /* array of StoredEvent */ ],
        "confidence": "HIGH"
      }
    ],
    "verification": {
      "chain_verified": true,
      "tests_executed": true,
      "build_succeeded": true,
      "human_reviewed": false
    },
    "intent": "Implement OAuth2 authentication",
    "coverage_pct": 87.5,
    "unmatched_count": 2,
    "total_events": 15
  }
}
```

---

### DoctorInfo (Diagnostics)

Get comprehensive daemon diagnostics.

**Request:**
```json
{"type": "DoctorInfo", "payload": null}
```

**Response:**
```json
{
  "type": "Doctor",
  "payload": {
    "uptime_secs": 3600,
    "events_count": 1250,
    "exchanges_count": 8500,
    "events_index_count": 1250,
    "exchanges_index_count": 8500,
    "database_size_bytes": 52428800,
    "events_index_size_bytes": 1048576,
    "exchanges_index_size_bytes": 4194304,
    "model_loaded": true,
    "model_size_bytes": 45000000,
    "memory_rss_bytes": 134217728
  }
}
```

---

### IndexConversations (Index Archives)

Trigger indexing of Claude Code conversation archives.

**Request:**
```json
{"type": "IndexConversations", "payload": null}
```

**Response:**
```json
{
  "type": "IndexStats",
  "payload": {
    "exchanges_indexed": 150,
    "archives_processed": 3,
    "errors": 0
  }
}
```

---

### SummarizeExchanges (Generate AI Summaries)

Summarize exchanges that don't have summaries yet.

**Request:**
```json
{
  "type": "SummarizeExchanges",
  "payload": {
    "limit": 100
  }
}
```

**Response:**
```json
{
  "type": "SummarizeStats",
  "payload": {
    "summarized": 85,
    "skipped": 10,
    "errors": 5
  }
}
```

---

### Maintenance (Database Cleanup)

Run database maintenance (VACUUM, ANALYZE, pruning).

**Request:**
```json
{
  "type": "Maintenance",
  "payload": {
    "retention_days": 90
  }
}
```

**Fields:**
| Field | Type | Description |
|-------|------|-------------|
| `retention_days` | number | Prune data older than N days (0 = no pruning) |

**Response:**
```json
{
  "type": "MaintenanceStats",
  "payload": {
    "size_before": 1073741824,
    "size_after": 805306368,
    "events_pruned": 5000,
    "exchanges_pruned": 2500,
    "duration_ms": 4200
  }
}
```

---

### Shutdown

Gracefully stop the daemon.

**Request:**
```json
{"type": "Shutdown", "payload": null}
```

**Response:**
```json
{"type": "Ok", "payload": null}
```

---

## Error Handling

All operations may return an error response:

```json
{
  "type": "Error",
  "payload": "Description of what went wrong"
}
```

Common errors:
- `"Database error: ..."` - SQLite operation failed
- `"Invalid message: ..."` - Malformed JSON
- `"Embedding model not loaded"` - Semantic search unavailable

---

## Building Custom Hooks

### For Other AI Assistants

To add Diachron support for Cursor, Codex, or other tools:

1. **Capture events** when files are modified
2. **Set `tool_name`** to identify the source (e.g., "Cursor", "Codex")
3. **Include metadata** like session ID, branch, user intent

Example Cursor hook:

```typescript
async function captureEvent(change: FileChange) {
  const sock = await connectUnixSocket("~/.diachron/diachron.sock");

  await sock.write(JSON.stringify({
    type: "Capture",
    payload: {
      tool_name: "Cursor",
      file_path: change.absolutePath,
      operation: change.type,  // "create" | "modify" | "delete"
      diff_summary: change.summary,
      metadata: JSON.stringify({
        cursor_session: process.env.CURSOR_SESSION_ID,
        branch: await getGitBranch()
      })
    }
  }) + "\n");

  await sock.read();  // Wait for response
  sock.close();
}
```

### For CI/CD Pipelines

Use the IPC API to query provenance in GitHub Actions:

```yaml
- name: Generate Evidence Pack
  run: |
    echo '{"type":"CorrelateEvidence","payload":{
      "pr_id": ${{ github.event.pull_request.number }},
      "commits": ${{ toJson(github.event.pull_request.commits) }},
      "branch": "${{ github.head_ref }}",
      "start_time": "2026-01-01T00:00:00Z",
      "end_time": "2026-01-11T23:59:59Z"
    }}' | nc -U ~/.diachron/diachron.sock > evidence.json
```

---

## Version Compatibility

| API Version | Diachron Version | Notes |
|-------------|------------------|-------|
| 1.0 | v0.3.0+ | Core IPC, Capture, Timeline, Search |
| 1.1 | v0.4.0+ | BlameByFingerprint, CorrelateEvidence |
| 1.2 | v0.5.0+ | Intent extraction in BlameResult |
| 1.3 | v0.6.0+ | Maintenance command |

---

## Support

- **Issues**: https://github.com/wolfiesch/diachron/issues
- **Discussions**: https://github.com/wolfiesch/diachron/discussions
