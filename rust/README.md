# Diachron Rust Hook

High-performance event capture hook for Diachron provenance tracking.

## Overview

This Rust binary replaces the Python `hook_capture.py` for **26x faster** event capture. It's invoked by Claude Code's PostToolUse hook mechanism after every Write, Edit, or Bash command.

## Performance

| Metric | Python | Rust | Improvement |
|--------|--------|------|-------------|
| Startup | ~189ms | ~3ms | **55x** |
| Full capture | ~300ms | ~12ms | **26x** |

## Architecture

```
stdin (JSON) → Parse → Extract Event → SQLite Insert → exit 0
     ↓
{tool_name, tool_input, cwd, ...}
```

### Key Components

- **`main.rs`** - Entry point, stdin reading, project detection
- **`HookInput`** struct - Deserialized hook JSON from Claude Code
- **`CaptureEvent`** struct - Normalized event for database storage
- **`Operation`** enum - Event types (Create, Modify, Delete, Commit, etc.)
- **`CommandCategory`** enum - Bash command classification (Git, Test, Build, etc.)

### Features

1. **Fast Skip** - Exits immediately if no `.diachron/` directory found
2. **Tool Filtering** - Only captures Write, Edit, and file-modifying Bash commands
3. **Git Integration** - Captures current branch and commit SHA
4. **Semantic Parsing** - Classifies Bash commands into categories
5. **Bundled SQLite** - No external database dependencies

## Building

### Prerequisites

- Rust 1.70+ (for edition 2021)
- No additional system dependencies

### Build Commands

```bash
# Development build (fast, debug symbols)
cargo build

# Release build (optimized, ~26x faster)
cargo build --release

# Run tests
cargo test
```

### Build Output

```
target/release/diachron-hook    # ~2.2MB binary
```

## Configuration

The hook is configured in `~/.claude/settings.json`:

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Write|Edit|Bash",
        "hooks": [
          {
            "type": "command",
            "command": "~/.claude/skills/diachron/rust/target/release/diachron-hook",
            "timeout": 5
          }
        ]
      }
    ]
  }
}
```

## Input/Output

### Input (stdin)

JSON from Claude Code's PostToolUse hook:

```json
{
  "tool_name": "Write",
  "tool_input": {
    "file_path": "/path/to/file.py",
    "content": "..."
  },
  "tool_result": "success",
  "cwd": "/project/root"
}
```

### Output

- Exit 0: Success (event captured or intentionally skipped)
- Exit 0 with no action: No `.diachron/` directory found
- Exit 0 with no action: Read-only tool (Read, Grep, etc.)

The hook is designed to **never fail loudly** - it exits cleanly even on errors to avoid disrupting the user's workflow.

## Database Schema

Events are stored in `.diachron/events.db`:

```sql
CREATE TABLE events (
    id INTEGER PRIMARY KEY,
    timestamp TEXT NOT NULL,
    timestamp_display TEXT,
    session_id TEXT,
    tool_name TEXT NOT NULL,
    file_path TEXT,
    operation TEXT,
    diff_summary TEXT,
    raw_input TEXT,
    ai_summary TEXT,
    git_commit_sha TEXT,
    parent_event_id INTEGER,
    metadata TEXT  -- JSON: {git_branch, command_category, ...}
);
```

## Command Categories

Bash commands are classified into semantic categories:

| Category | Examples |
|----------|----------|
| Git | `git commit`, `git push`, `git pull` |
| Test | `npm test`, `pytest`, `cargo test` |
| Build | `npm build`, `cargo build`, `make` |
| Deploy | `vercel`, `fly deploy`, `docker push` |
| FileOps | `rm`, `mv`, `cp`, `mkdir`, `touch` |
| Package | `npm install`, `pip install`, `cargo add` |
| Unknown | Everything else |

## Troubleshooting

### Binary Not Found

```bash
# Verify binary exists
ls -la ~/.claude/skills/diachron/rust/target/release/diachron-hook

# Test manually
echo '{}' | ~/.claude/skills/diachron/rust/target/release/diachron-hook
echo $?  # Should be 0
```

### Exit Code 137

Exit code 137 means SIGKILL. This is usually caused by:
1. Binary corruption during copy
2. macOS filesystem caching issues

**Fix:** Use the binary directly from `target/release/` (don't copy).

### Rebuild After Changes

```bash
cd ~/.claude/skills/diachron/rust
cargo build --release
# Restart Claude Code to pick up changes
```

## Development

### Adding a New Command Category

1. Add variant to `CommandCategory` enum
2. Update `classify_command()` function
3. Add patterns to match the new category

### Testing

```bash
# Run unit tests
cargo test

# Test with sample input
echo '{"tool_name":"Write","tool_input":{"file_path":"test.py"},"cwd":"/tmp/test"}' | \
  cargo run --release
```

## Dependencies

From `Cargo.toml`:

```toml
[dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
chrono = { version = "0.4", features = ["serde"] }
rusqlite = { version = "0.31", features = ["bundled"] }
```

## License

MIT License - same as parent project.
