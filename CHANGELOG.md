# Changelog

All notable changes to Diachron will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-01-10

### Added

- **Rust Daemon Architecture (diachrond)**
  - Long-running background service with Unix socket IPC
  - ~7ms cold start (vs 2-3s Node.js)
  - Graceful shutdown on SIGTERM
  - Auto-start via launchd (macOS) / systemd (Linux)

- **Semantic Search**
  - all-MiniLM-L6-v2 embeddings (384-dim vectors)
  - usearch HNSW index (~10μs per search)
  - SQLite FTS5 full-text search with triggers
  - Hybrid search combining vector + keyword matching

- **Conversation Memory**
  - JSONL archive parser for Claude Code sessions
  - 282K+ exchanges indexed from conversation history
  - Incremental indexing with checkpoint state
  - UTF-8 safe truncation for multi-byte characters

- **New Skills**
  - `/memory` - Search conversation memory semantically
  - `/search` - Unified search across code + conversations

- **Daemon Lifecycle Management**
  - `diachron daemon start` - Start daemon manually
  - `diachron daemon stop` - Stop daemon
  - `diachron daemon status` - Check daemon health
  - macOS launchd plist template
  - Linux systemd service template

- **Installer Updates**
  - Automatic daemon setup on install
  - Platform detection (macOS/Linux)
  - Daemon verification after install

### Changed

- Hook now routes events through daemon (unified database)
- Events stored in global `~/.diachron/diachron.db`
- Vector indexes saved to `~/.diachron/indexes/`

### Performance

| Metric | v0.1.0 | v0.2.0 | Improvement |
|--------|--------|--------|-------------|
| Cold start | 2-3s | ~7ms | 300x |
| Search latency | N/A | ~30ms | New feature |
| Hook latency | ~12ms | ~16ms | +4ms (IPC) |
| Memory | N/A | ~50MB | Efficient |

### Technical Notes

- Uses `ort` v2.0.0-rc.11 for ONNX runtime
- `twox-hash` for stable hashing across Rust versions
- `Mutex<Connection>` for thread-safe SQLite access
- All 9 tests passing

---

## [0.1.0] - 2026-01-08

### Added

- **Core Functionality**
  - PostToolUse hook captures Write, Edit, and Bash tool events
  - SQLite database storage in `.diachron/events.db`
  - Dual timestamp format (ISO for sorting, human-readable for display)
  - Session grouping with 1-hour persistence window

- **Rust Hook (diachron-hook)**
  - High-performance event capture (~12ms latency)
  - Git branch detection for every event
  - Commit SHA capture for git commit commands
  - Semantic Bash command classification (git, test, build, deploy, file_ops)
  - Bundled SQLite (no external dependencies)
  - Pre-built binary for macOS ARM64

- **Python Fallback**
  - Full feature parity with Rust hook
  - Works on any platform with Python 3.8+
  - Higher latency (~300ms) but more compatible

- **Timeline CLI**
  - `/timeline` command for viewing events
  - Time filtering with `--since` and `--until`
  - File filtering with `--file`
  - Tool filtering with `--tool`
  - Statistics with `--stats`
  - Export to Markdown and JSON
  - AI summaries with `--summarize` (requires ANTHROPIC_API_KEY)

- **AI Summaries**
  - On-demand summarization via Anthropic Claude Haiku
  - Batch processing with configurable limits
  - 10-word concise summaries
  - ~$0.03 per 1000 events

- **Commands**
  - `/diachron init` - Initialize tracking in a project
  - `/diachron status` - Show tracking status
  - `/diachron config` - View/edit configuration
  - `/diachron capture` - Manual event capture

- **Documentation**
  - Comprehensive README with features and examples
  - Step-by-step INSTALL.md
  - TROUBLESHOOTING.md for common issues
  - Plugin marketplace.json metadata

### Performance

| Component | Latency |
|-----------|---------|
| Rust hook | ~12ms |
| Python hook | ~300ms |
| Rust vs Python | **26x faster** |

### Technical Notes

- Rust hook uses `thin` LTO (not full) to avoid hangs on certain systems
- Binary corruption issues on macOS resolved by pointing directly to `target/release/`
- gpt-4o-mini used for summaries (gpt-5-mini is a reasoning model that doesn't work for this use case)

---

## [0.3.0] - 2026-01-11

### Added

- **Hash-Chain Tamper Evidence**
  - SHA256 hash chain linking all events (cryptographic tamper detection)
  - `prev_hash` and `event_hash` columns on every event
  - Genesis hash for chain origin
  - Daily checkpoints with `chain_checkpoints` table
  - `diachron verify` command to validate chain integrity

- **PR Narrative Generation**
  - `diachron export-evidence` - Generate JSON evidence packs
  - `diachron pr-comment` - Post formatted Markdown to PRs via `gh` CLI
  - Event → Commit → PR correlation with 3-tier confidence:
    - HIGH: Direct `git_commit_sha` linkage
    - MEDIUM: Same session as commit event
    - LOW: Time-window correlation (5min before commit)
  - Coverage metrics showing matched vs unmatched events

- **Content Fingerprinting**
  - `content_hash` and `context_hash` for stable blame across refactors
  - SHA256 content hashing with normalized whitespace
  - Context hashing (±5 lines surrounding code)
  - Optional semantic signature (384-dim embeddings)
  - Three-tier matching: ContentHash → ContextHash → SemanticSimilarity

- **Semantic Blame (v0.4 Preview)**
  - `diachron blame <file:line>` - Find the AI session that wrote code
  - `--json` flag for CI/IDE integration
  - `--mode strict|best-effort|inferred` for confidence control
  - Shows intent, session, timestamp, and verification status

- **GitHub Action**
  - `wolfiesch/diachron/github-action@main` for automated PR comments
  - Reads `diachron.evidence.json` and posts formatted narrative
  - Supports `update` mode (edit existing comment) or `new` mode
  - Outputs: `comment-id`, `coverage`, `verified`

- **Schema Migration v4**
  - Hash chain columns: `prev_hash`, `event_hash`
  - Fingerprint columns: `content_hash`, `context_hash`
  - `chain_checkpoints` table for verification anchors
  - Indexes on `event_hash` for fast lookups

### Changed

- Daemon `save_event()` now computes hash chain on every insert
- Evidence pack renders verification checklist: chain, tests, build, human review
- 42 tests passing across all crates

### Performance

| Metric | v0.2.0 | v0.3.0 | Notes |
|--------|--------|--------|-------|
| Hash computation | N/A | ~0.5ms | SHA256 per event |
| Chain verification | N/A | ~100ms/1000 events | Full chain scan |
| PR correlation | N/A | ~50ms | Typical PR size |
| Evidence export | N/A | ~10ms | JSON serialization |

### New Commands

```bash
diachron verify              # Verify hash chain integrity
diachron export-evidence     # Generate evidence pack JSON
diachron pr-comment --pr 142 # Post PR narrative comment
diachron blame src/auth.rs:42 # Semantic blame for line
```

---

## [Unreleased]

### Planned

- Web dashboard visualization
- Team sync (optional cloud feature)
- VS Code extension
- Windows support
- Conversation summarization via Anthropic API
- Log rotation for daemon
