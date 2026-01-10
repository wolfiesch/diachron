---
name: search
description: Unified semantic search across code changes and conversations
triggers:
  - /search
---

# Unified Search

Search across both code provenance (events) and conversation memory (exchanges) using Diachron's hybrid semantic search.

## Overview

Diachron provides unified search across:
- **Code events**: File changes, edits, bash commands (from hooks)
- **Conversation exchanges**: Past Claude Code discussions (280K+ indexed)

Search is powered by:
- **Vector search**: all-MiniLM-L6-v2 embeddings for semantic understanding
- **Full-text search**: SQLite FTS5 for keyword matching
- **Hybrid ranking**: Combines both for best results

## Usage

```bash
# Search everything
/search "authentication"
/search "React component patterns"
/search "database optimization"

# Filter by source type
/search "fix" --type event           # Code changes only
/search "how to" --type exchange     # Conversations only

# Limit results
/search "API design" --limit 20
```

## Commands

When the user runs `/search <query>`:

```bash
~/.claude/skills/diachron/rust/target/release/diachron search "<query>" --limit 10
```

With type filter:
```bash
~/.claude/skills/diachron/rust/target/release/diachron search "<query>" --type event --limit 10
```

## Output Format

Results show:
- **Score**: Relevance score (0-1, higher is better)
- **Source**: `Event` (code change) or `Exchange` (conversation)
- **Timestamp**: When it occurred
- **Snippet**: Preview of the content

Example:
```
[0.95] Event 2026-01-10T08:15:00 - Edit src/auth/middleware.ts (+45 lines)
[0.89] Exchange 2026-01-09T14:23:00 - "How do I implement OAuth2?"
[0.82] Event 2026-01-09T11:30:00 - Write src/lib/jwt.ts (new file)
[0.78] Exchange 2026-01-08T10:15:00 - "Best practices for JWT tokens"
```

## Options

| Option | Description |
|--------|-------------|
| `--limit N` | Maximum results to return (default: 10) |
| `--type event` | Only search code changes |
| `--type exchange` | Only search conversations |

## How It Works

1. **Query embedding**: Your search query is converted to a 384-dim vector
2. **Vector search**: usearch HNSW index finds semantically similar items (~10μs)
3. **FTS search**: SQLite FTS5 finds keyword matches
4. **Hybrid merge**: Results are deduplicated and ranked by combined score
5. **Response**: Top results returned with source, timestamp, and snippet

## Performance

| Metric | Value |
|--------|-------|
| Vector search | ~10μs per query |
| FTS search | ~5ms per query |
| Total latency | ~30ms end-to-end |
| Index size | ~455MB for 280K exchanges |

## Requirements

- Diachron daemon must be running: `diachron daemon status`
- At least one source must be indexed

## See Also

- `/memory` - Search only conversations
- `/timeline` - View code change history
- `/diachron status` - Check system status
