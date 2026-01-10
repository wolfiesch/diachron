# Diachron

[![Version](https://img.shields.io/badge/version-0.1.0-blue.svg)](https://github.com/wolfiesch/diachron)
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
- **Semantic Bash Parsing** - Categories: git, test, build, deploy, file_ops
- **AI Summaries** - On-demand summaries via OpenAI (optional)
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
| `/timeline --summarize` | Generate AI summaries (requires OpenAI API key) |
| `/timeline --export markdown` | Export to TIMELINE.md |

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
5. **AI Summaries** - Optional on-demand summaries via OpenAI gpt-4o-mini

## Requirements

| Requirement | Version | Notes |
|-------------|---------|-------|
| Claude Code | 2.1+ | Required for PostToolUse hook support |
| Python | 3.8+ | For timeline CLI and database operations |
| macOS/Linux | Any | Windows is untested |

**Optional:**
- OpenAI API key (for AI-powered summaries via `/timeline --summarize`)
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

## Roadmap

- [x] ~~AI-powered change summaries~~
- [x] ~~Git branch/commit correlation~~
- [x] ~~Semantic Bash command parsing~~
- [ ] Web dashboard visualization
- [ ] Team sync (cloud option)
- [ ] VS Code extension

## License

MIT License - do what you want with it.

---

Built with ❤️ for vibe coders everywhere.
