# Diachron

[![Version](https://img.shields.io/badge/version-1.0.0-blue.svg)](https://github.com/wolfiesch/diachron)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux-lightgrey.svg)]()
[![Claude Code](https://img.shields.io/badge/Claude%20Code-2.1%2B-orange.svg)]()

**Agentic Provenance for AI-Assisted Development**

> *diachron* (from Greek *dia* "through" + *chronos* "time")

Diachron automatically tracks every code change made by AI coding assistants, creating a queryable timeline of your project's evolution.

## The Problem

When AI agents like Claude Code make changes to your codebase:
- Git commits contain technical jargon you may not understand
- The AI's reasoning and intent isn't captured anywhere
- You lose visibility into *when* bugs were introduced or features built
- There's no human-readable narrative of what happened

## The Solution

Diachron uses **Claude Code 2.1's hook architecture** to transparently capture every file modification. No manual logging required.

```
┌─────────────────────────────────────────────────────────────────┐
│                        Claude Code Session                       │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│   ┌──────────────┐    PostToolUse    ┌──────────────────────┐   │
│   │  Write/Edit  │ ────────────────► │  diachron-hook       │   │
│   │  Bash Tools  │       ~12ms       │  (Rust binary)       │   │
│   └──────────────┘                   └──────────┬───────────┘   │
│                                                 │               │
│                                                 ▼               │
│                                      ┌──────────────────────┐   │
│                                      │  .diachron/          │   │
│                                      │  ├── events.db       │   │
│                                      │  └── config.json     │   │
│                                      └──────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

## Features

- **Automatic Capture** - Every Write, Edit, and Bash command logged
- **Git Integration** - Captures branch name and commit SHAs
- **Web Dashboard** - Real-time timeline visualization with filtering (v1.0)
- **VS Code Extension** - Inline blame on hover with gutter icons (v0.8)
- **Hash-Chain Integrity** - SHA256 tamper-evidence for every event (v0.3)
- **PR Narratives** - Generate evidence packs for pull request comments (v0.3)
- **Semantic Blame** - Find which AI session wrote specific code lines (v0.3)
- **Semantic Bash Parsing** - Categories: git, test, build, deploy, file_ops
- **AI Summaries** - On-demand summaries via Anthropic Claude API (optional)
- **Multi-Assistant Support** - Track Codex CLI alongside Claude Code (v0.7)
- **Fast** - Rust hook adds only ~12ms latency per operation
- **Privacy-First** - All data stored locally, never uploaded

## Quick Start

### Installation

```bash
# One-liner install (recommended)
curl -fsSL https://raw.githubusercontent.com/wolfiesch/diachron/main/install.sh | bash
```

The installer automatically:
- Clones to `~/.claude/skills/diachron/`
- Configures the PostToolUse hook in your settings
- Detects your architecture and selects the optimal hook (Rust or Python)
- Verifies the installation

**After install:** Restart Claude Code to activate the hook.

<details>
<summary>Alternative: Manual installation</summary>

```bash
# Clone to Claude Code skills directory
git clone https://github.com/wolfiesch/diachron ~/.claude/skills/diachron

# Run the installer
~/.claude/skills/diachron/install.sh
```

See [INSTALL.md](./INSTALL.md) for complete manual installation instructions.
</details>

### Usage

1. **Initialize in your project:**
   ```
   /diachron init
   ```

2. **Work normally** - changes are captured automatically

3. **View your timeline:**
   ```
   /timeline
   /timeline --since "1 hour ago"
   /timeline --file src/
   /timeline --stats
   /timeline --summarize
   ```

## Commands

| Command | Description |
|---------|-------------|
| `/diachron init` | Initialize Diachron for this project |
| `/diachron status` | Show tracking status and stats |
| `/diachron config` | View/edit configuration |
| `/timeline` | View change timeline |
| `/timeline --stats` | Show database statistics |
| `/timeline --watch` | Watch for new events in real-time (Ctrl+C to stop) |
| `/timeline --summarize` | Generate AI summaries (requires ANTHROPIC_API_KEY) |
| `/timeline --export markdown` | Export to TIMELINE.md |

### CLI Commands

| Command | Description |
|---------|-------------|
| `diachron verify` | Verify hash chain integrity |
| `diachron export-evidence` | Generate JSON evidence pack |
| `diachron pr-comment --pr <N>` | Post PR narrative comment via `gh` CLI |
| `diachron blame <file:line>` | Semantic blame for a code line |
| `diachron maintenance` | Run database VACUUM/ANALYZE, prune old data |
| `diachron daemon start` | Start the background daemon |
| `diachron daemon stop` | Stop the daemon |
| `diachron daemon status` | Check daemon status |
| `diachron dashboard start` | Start web dashboard at localhost:3947 |
| `diachron dashboard stop` | Stop web dashboard |
| `diachron dashboard status` | Check dashboard and daemon status |

## Timeline Output

```
📍 Timeline for my-project
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

🕐 01/08/2026 08:25 AM PST
   ├─ Tool: ✏️ Edit
   ├─ File: src/auth/login.ts
   ├─ Branch: 🌿 feature/oauth
   ├─ Operation: modify (+12/-3 lines)
   └─ Summary: Added OAuth2 refresh token handling

🕐 01/08/2026 08:20 AM PST
   ├─ Tool: 🖥️ Bash [test]
   ├─ Branch: 🌿 feature/oauth
   ├─ Operation: npm test
   └─ Summary: Ran test suite (all passed)

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

## Timeline Examples

**Show recent changes:**
```
/timeline
```

**Filter by time:**
```
/timeline --since "yesterday"
/timeline --since "1 hour ago"
/timeline --since "2026-01-01"
```

**Filter by file:**
```
/timeline --file src/components/
/timeline --file package.json
```

**Filter by tool:**
```
/timeline --tool Edit
/timeline --tool Write
/timeline --tool Bash
```

**Generate AI summaries:**
```
/timeline --summarize           # Summarize unsummarized events
/timeline --summarize --limit 50  # Limit batch size
```

**Export:**
```
/timeline --export markdown
/timeline --export json
```

## v0.3: Trust & Verification

### Hash-Chain Verification

Every event is cryptographically linked to the previous event using SHA256:

```bash
$ diachron verify
✅ Chain integrity verified
   Events: 296 (12 checkpoints)
   First event: 2026-01-01 00:00:00
   Last event: 2026-01-11 00:45:00
   Chain root: 8f3a2b...
```

If tampering is detected:
```bash
$ diachron verify
❌ Chain broken at event #142
   Expected: 8f3a2b...
   Actual: deadbeef...
   Timestamp: 2026-01-10 14:30:00
```

### PR Narrative Generation

Generate evidence packs showing which AI sessions contributed to a PR:

```bash
# Export evidence to JSON
$ diachron export-evidence --output diachron.evidence.json

# Post comment directly to PR (requires gh CLI)
$ diachron pr-comment --pr 142
```

Example PR comment:
```markdown
## PR #142: AI Provenance Evidence

### Intent
> Fix the 401 errors on page refresh

### What Changed
- **Files modified**: 2
- **Lines**: +45 / -10
- **Tool operations**: 3
- **Sessions**: 1

### Evidence Trail
- **Coverage**: 100.0% of events matched to commits

**Commit `abc1234`**: Fix OAuth2 refresh (HIGH)
  - `Write` create → src/auth.rs
  - `Edit` modify → src/auth.rs

### Verification
- [x] Hash chain integrity
- [x] Tests executed after changes
- [x] Build succeeded
- [ ] Human review
```

### Semantic Blame (v0.4 Preview)

Find which AI session wrote specific code:

```bash
$ diachron blame src/auth/login.ts:42

Line 42: const token = await refreshToken(user.id);

📍 Source: Claude Code (Session abc123)
⏰ When: 01/10/2026 10:32 AM PST
💬 Intent: "Fix the 401 errors on page refresh"
📊 Confidence: HIGH (explicit tool call linkage)
```

Use `--json` for CI/IDE integration:
```bash
$ diachron blame src/auth/login.ts:42 --json | jq
```

### GitHub Action

Automatically post evidence to PRs:

```yaml
# .github/workflows/diachron.yml
name: Diachron PR Narrative
on: [pull_request]

jobs:
  post-evidence:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: wolfiesch/diachron/github-action@main
        with:
          github-token: ${{ secrets.GITHUB_TOKEN }}
```

## Web Dashboard (v1.0)

A real-time web interface for exploring your AI provenance timeline.

### Starting the Dashboard

```bash
# Start the dashboard (opens browser automatically)
$ diachron dashboard start
🚀 Starting Diachron dashboard...
   Proxy: http://localhost:3947
   Daemon: Connected (uptime: 6576s, 871 events)
✅ Dashboard running at http://localhost:3947

# Check status
$ diachron dashboard status
Dashboard: Running (http://localhost:3947)
Daemon: Connected (uptime: 1h 49m)
Events: 871

# Stop the dashboard
$ diachron dashboard stop
✅ Dashboard stopped
```

### Dashboard Pages

| Page | URL | Description |
|------|-----|-------------|
| **Dashboard** | `/` | Overview with stat cards, recent activity, quick actions |
| **Timeline** | `/timeline` | Filterable event list with virtual scrolling |
| **Sessions** | `/sessions` | Browse AI sessions and their events |
| **Search** | `/search` | Semantic + keyword search across events |
| **Blame** | `/blame` | Interactive code attribution lookup |
| **Evidence** | `/evidence` | PR evidence pack viewer |
| **Diagnostics** | `/doctor` | Daemon health, database stats |

### Features

- **Real-time updates** via WebSocket - new events appear automatically
- **Virtual scrolling** - handles 10K+ events smoothly
- **Time filters** - Last hour, 24h, 7 days, 30 days, all time
- **Tool filters** - Claude, Codex, Bash, or all
- **File path search** - Filter events by file/directory
- **Event detail drawer** - Slide-in panel with full event details

### Design

The dashboard uses a "Terminal Noir" dark theme optimized for developers:
- Dark background (`#0a0a0b`) with subtle accents
- Confidence colors: Green (HIGH), Amber (MEDIUM), Gray (LOW), Purple (INFERRED)
- JetBrains Mono for code, Inter for UI text
- Framer Motion animations for smooth transitions

## VS Code Extension (v0.8)

Get AI provenance directly in your editor with inline blame on hover.

### Installation

```bash
# From marketplace (coming soon)
code --install-extension wolfiesch.diachron

# Or install local VSIX
code --install-extension ~/.claude/skills/diachron/vscode-extension/diachron-0.8.0.vsix
```

### Features

- **Hover blame** - See AI provenance when hovering over code lines
- **Gutter icons** - Visual indicators for AI-written lines (green/yellow/gray)
- **Timeline sidebar** - Browse recent events in the Explorer panel
- **Session details** - View full session context in dedicated panel

### Hover Card

When you hover over AI-modified code:

```
┌────────────────────────────────────────┐
│ 🤖 Claude Code                         │
│ Session: abc123 • 2 hours ago          │
├────────────────────────────────────────┤
│ 💬 "Fix the 401 errors on page refresh"│
├────────────────────────────────────────┤
│ 📊 HIGH confidence                     │
│ ├─ Content hash match                  │
│ └─ Same session context                │
└────────────────────────────────────────┘
```

## Configuration

Edit `.diachron/config.json`:

```json
{
  "ai_summaries": false,        // AI-generated change descriptions
  "retention_days": 90,         // How long to keep events
  "exclude_patterns": [         // Files to ignore
    "node_modules/**",
    "*.lock"
  ]
}
```

## How It Works

1. **Hook Capture** - A Rust binary hook fires after Write, Edit, or Bash tools (~12ms)
2. **Context Extraction** - Captures file path, operation, git branch, and diff summary
3. **SQLite Storage** - Events stored in `.diachron/events.db` for fast querying
4. **Timeline Generation** - Query by time, file, or tool to see your project's history
5. **AI Summaries** - Optional on-demand summaries via Anthropic Claude Haiku

## Requirements

| Requirement | Version | Notes |
|-------------|---------|-------|
| Claude Code | 2.1+ | Required for PostToolUse hook support |
| Python | 3.8+ | For timeline CLI and database operations |
| macOS/Linux | Any | Windows is untested |

**Optional:**
- Anthropic API key (for AI-powered summaries via `/timeline --summarize`)
- Rust 1.70+ (only if building from source)

## Performance

| Hook Type | Latency | Notes |
|-----------|---------|-------|
| Rust (default) | ~12ms | Pre-built binary for macOS ARM64 |
| Python (fallback) | ~300ms | Use if Rust binary doesn't work |

The Rust hook is **26x faster** than Python, achieved through:
- No interpreter startup overhead
- Bundled SQLite (no external dependencies)
- Optimized release build with LTO

## Privacy

- All data stored **locally** in your project's `.diachron/` directory
- Added to `.gitignore` by default (not committed)
- No cloud sync or external services
- You control your provenance data

## Troubleshooting

See [TROUBLESHOOTING.md](./TROUBLESHOOTING.md) for common issues and solutions.

Quick diagnostics:
```bash
# Run installer diagnostics
~/.claude/skills/diachron/install.sh --doctor

# Check project status
/diachron status

# Verify database
sqlite3 .diachron/events.db "SELECT COUNT(*) FROM events"
```

### Installer Commands

```bash
install.sh              # Install or update
install.sh --update     # Pull latest and rebuild
install.sh --doctor     # Run diagnostics
install.sh --uninstall  # Remove completely
```

## Multi-Assistant Support (v0.7)

Diachron can track file changes from multiple AI assistants, not just Claude Code. Currently supported:

### OpenAI Codex CLI

#### Via `/handoffcodex` (Recommended)

When using Claude Code's `/handoffcodex` skill to delegate work to Codex, provenance is captured automatically after execution completes. Events appear in your timeline with `tool_name: "Codex"`.

#### Standalone Wrapper

For direct Codex usage without Claude Code orchestration:

```bash
# Build the wrapper
cd ~/.claude/skills/diachron/rust
cargo build --release -p diachron-codex

# Use instead of `codex exec`
diachron-codex exec "implement the login feature"
```

This transparently wraps Codex, capturing all file operations to Diachron.

#### Manual Capture

To capture a completed Codex session manually:

```bash
# Capture most recent Codex session
python3 ~/.claude/skills/diachron/lib/codex_capture.py --latest

# With git branch correlation
python3 ~/.claude/skills/diachron/lib/codex_capture.py --latest --git-branch "feature/auth"

# Preview without sending to daemon
python3 ~/.claude/skills/diachron/lib/codex_capture.py --latest --dry-run --verbose
```

### Future Assistants

The IPC API (see `docs/IPC-API.md`) enables community integrations for:
- **Cursor** - Hook into Cursor's file modification events
- **GitHub Copilot** - VS Code extension integration
- **Aider** - Parse session logs similar to Codex

## Roadmap

- [x] ~~AI-powered change summaries~~ (v0.1)
- [x] ~~Git branch/commit correlation~~ (v0.1)
- [x] ~~Semantic Bash command parsing~~ (v0.1)
- [x] ~~Semantic search + conversation memory~~ (v0.2)
- [x] ~~Hash-chain tamper evidence~~ (v0.3)
- [x] ~~PR narrative generation~~ (v0.3)
- [x] ~~Semantic blame~~ (v0.4)
- [x] ~~Intent extraction from conversations~~ (v0.5)
- [x] ~~Log rotation + database maintenance~~ (v0.6)
- [x] ~~Multi-assistant support (Codex)~~ (v0.7)
- [x] ~~VS Code extension with inline blame~~ (v0.8)
- [x] ~~Web dashboard visualization~~ (v1.0)
- [ ] Team sync (cloud option)
- [ ] Multi-project support

## License

MIT License - do what you want with it.

---

Built with ❤️ for vibe coders everywhere.
