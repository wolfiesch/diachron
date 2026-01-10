---
name: memory
description: Search conversation memory across all Claude Code sessions
triggers:
  - /memory
---

# Memory Search

Search your past Claude Code conversations semantically using Diachron's unified memory system.

## Overview

Diachron indexes all your Claude Code conversation history (280K+ exchanges) and enables:
- **Semantic search**: Find conversations by meaning, not just keywords
- **Hybrid search**: Combines vector embeddings + full-text search
- **Cross-project**: Search across all projects and sessions

## Usage

```bash
# Search for topics
/memory "authentication implementation"
/memory "React hooks"
/memory "database schema design"

# With filters
/memory "bug fix" --limit 20
/memory "API design" --type exchanges

# Index new conversations (usually automatic)
/memory index
```

## Commands

When the user runs `/memory <query>`:

1. **If query is "index"**: Trigger conversation indexing
   ```bash
   ~/.claude/skills/diachron/rust/target/release/diachron memory index
   ```

2. **If query is a search term**: Search conversation memory
   ```bash
   ~/.claude/skills/diachron/rust/target/release/diachron search "<query>" --type exchange --limit 10
   ```

## How It Works

1. **Indexing**: Diachron daemon parses `~/.claude/projects/*/*.jsonl` archives
2. **Embeddings**: Uses `all-MiniLM-L6-v2` model (384-dim vectors) for semantic understanding
3. **Storage**: Unified SQLite database at `~/.diachron/diachron.db`
4. **Search**: usearch HNSW index for ~10μs vector search + FTS5 for keyword matching

## Example Output

```
[0.92] Exchange 2026-01-09T14:23:00 - "How do I implement OAuth2 authentication?"
[0.87] Exchange 2026-01-08T10:15:00 - "Best practices for JWT token handling"
[0.81] Exchange 2026-01-05T16:42:00 - "Setting up auth middleware in Express"
```

## Options

- `--limit N` - Return top N results (default: 10)
- `--type exchanges` - Search only conversations (not code events)

## Requirements

- Diachron daemon must be running: `diachron daemon status`
- Conversations must be indexed: `diachron memory status`

## Troubleshooting

If no results are found:
1. Check daemon is running: `diachron daemon status`
2. Check index status: `diachron memory status`
3. Re-index if needed: `/memory index`
