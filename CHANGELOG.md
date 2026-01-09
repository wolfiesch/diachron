# Changelog

All notable changes to Diachron will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
  - AI summaries with `--summarize` (requires OpenAI API key)

- **AI Summaries**
  - On-demand summarization via OpenAI gpt-4o-mini
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

## [Unreleased]

### Planned

- Web dashboard visualization
- Team sync (optional cloud feature)
- VS Code extension
- Windows support
