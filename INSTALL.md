# Installing Diachron

Complete guide to installing and configuring Diachron for Claude Code.

## Prerequisites

| Requirement | Version | Notes |
|-------------|---------|-------|
| Claude Code | 2.1+ | Required for PostToolUse hook support |
| Python | 3.8+ | For timeline CLI and database operations |
| macOS/Linux | Any | Windows is untested |

**Optional:**
- OpenAI API key (for AI-powered summaries via `/timeline --summarize`)

## Installation Methods

### Method 1: Git Clone (Recommended)

```bash
# Clone directly into Claude Code skills directory
git clone https://github.com/wolfiesch/diachron ~/.claude/skills/diachron

# Verify installation
ls ~/.claude/skills/diachron
# Should show: README.md, diachron.md, timeline.md, lib/, rust/, etc.
```

### Method 2: Manual Download

1. Download the latest release from GitHub
2. Extract to `~/.claude/skills/diachron/`
3. Ensure the directory structure matches:

```
~/.claude/skills/diachron/
├── diachron.md          # /diachron command
├── timeline.md          # /timeline command
├── capture.md           # PostToolUse hook (optional)
├── README.md
├── lib/
│   ├── db.py
│   ├── timeline_cli.py
│   ├── hook_capture.py
│   └── summarize.py
└── rust/
    └── target/release/diachron-hook  # Pre-built binary (macOS ARM64)
```

## Configure the PostToolUse Hook (Required)

Diachron captures events via a PostToolUse hook. You must add this to your Claude Code settings.

### Step 1: Locate Settings File

```bash
# Your settings file is at:
~/.claude/settings.json
```

### Step 2: Add the Hook Configuration

Open `~/.claude/settings.json` and add the `PostToolUse` hook inside the `"hooks"` section:

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

**Important:**
- If `"hooks"` doesn't exist, create it
- If `"PostToolUse"` already exists, merge the configuration
- The Rust hook is ~26x faster than Python - use it if available

### Step 3: Restart Claude Code

Close and reopen Claude Code to load the new hook configuration.

## Alternative: Python Hook (Fallback)

If the pre-built Rust binary doesn't work (e.g., on x86 Mac or Linux), use the Python hook:

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Write|Edit|Bash",
        "hooks": [
          {
            "type": "command",
            "command": "python3 ~/.claude/skills/diachron/lib/hook_capture.py",
            "timeout": 5
          }
        ]
      }
    ]
  }
}
```

Note: Python hook adds ~300ms latency per tool call vs ~12ms for Rust.

## Verify Installation

### 1. Check Commands Are Available

Start Claude Code in any project and try:

```
/diachron status
```

You should see either "Not initialized" or status info.

### 2. Initialize Diachron

```
/diachron init
```

This creates the `.diachron/` directory with `events.db` and `config.json`.

### 3. Test Event Capture

Make a change (write a file, edit something) and then:

```
/timeline
```

If the hook is working, you'll see the captured event.

### 4. Check Hook is Firing

After any Write/Edit/Bash operation, you should NOT see errors like:
- "PostToolUse:Bash hook error"
- "Failed with non-blocking status code"

If you see these, check [TROUBLESHOOTING.md](./TROUBLESHOOTING.md).

## Building from Source (Advanced)

### Build Rust Hook

If you need to build the Rust hook yourself:

```bash
cd ~/.claude/skills/diachron/rust

# Install Rust if needed
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build release binary
cargo build --release

# Binary will be at:
# ./target/release/diachron-hook
```

### Requirements for Building

- Rust 1.70+ (for edition 2021)
- No additional system dependencies (SQLite is bundled)

## Updating Diachron

### Via Git

```bash
cd ~/.claude/skills/diachron
git pull origin main
```

### Manual Update

1. Download the new release
2. Replace files in `~/.claude/skills/diachron/`
3. Restart Claude Code

**Note:** Your `.diachron/events.db` data is stored per-project and won't be affected by updates.

## Uninstalling

### Remove Skill

```bash
rm -rf ~/.claude/skills/diachron
```

### Remove Hook from Settings

Edit `~/.claude/settings.json` and remove the `PostToolUse` configuration.

### Remove Project Data (Optional)

In each project where Diachron was initialized:

```bash
rm -rf .diachron/
```

## Directory Locations

| Path | Purpose |
|------|---------|
| `~/.claude/skills/diachron/` | Skill installation |
| `~/.claude/settings.json` | Hook configuration |
| `<project>/.diachron/` | Per-project events database |
| `<project>/.diachron/events.db` | SQLite database |
| `<project>/.diachron/config.json` | Project configuration |

## Next Steps

After installation:

1. Run `/diachron init` in your project
2. Work normally - events are captured automatically
3. Run `/timeline` to see your history
4. Check `/timeline --stats` for statistics
5. Try `/timeline --summarize` for AI-powered summaries (requires OpenAI API key)

See [README.md](./README.md) for full usage documentation.

---

Having trouble? Check [TROUBLESHOOTING.md](./TROUBLESHOOTING.md) for common issues and solutions.
