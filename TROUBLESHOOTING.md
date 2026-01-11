# Troubleshooting Diachron

Common issues and solutions for Diachron provenance tracking.

## Quick Diagnostic

Run this command to check Diachron's status:

```bash
/diachron status
```

Or manually:

```bash
# Check if .diachron directory exists
ls -la .diachron/

# Check database has events
sqlite3 .diachron/events.db "SELECT COUNT(*) FROM events"

# Check hook is configured
grep -A 10 "PostToolUse" ~/.claude/settings.json
```

---

## Common Issues

### 1. "Diachron is not initialized for this project"

**Symptom:** `/timeline` or `/diachron status` shows "not initialized" error.

**Cause:** The `.diachron/` directory doesn't exist in the current project.

**Solution:**

```bash
# Initialize Diachron in your project
/diachron init
```

This creates:
- `.diachron/events.db` - SQLite database
- `.diachron/config.json` - Project configuration
- `.diachron/.session_id` - Session tracking file

**Verify:**

```bash
ls -la .diachron/
# Should show events.db, config.json
```

---

### 2. Events Not Being Captured

**Symptom:** You make changes but `/timeline` shows no new events.

**Possible Causes:**

#### A. Hook Not Configured

Check your settings file:

```bash
cat ~/.claude/settings.json | grep -A 15 "PostToolUse"
```

Should show:

```json
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
```

**Fix:** Add the hook configuration. See [INSTALL.md](./INSTALL.md#step-2-add-the-hook-configuration).

#### B. Hook Binary Not Found

**Symptom:** Errors like "command not found" or "No such file or directory".

```bash
# Verify binary exists
ls -la ~/.claude/skills/diachron/rust/target/release/diachron-hook

# Test it manually
echo '{}' | ~/.claude/skills/diachron/rust/target/release/diachron-hook
echo $?  # Should be 0
```

**Fix:** Build the binary or use Python fallback:

```bash
# Option 1: Build Rust binary
cd ~/.claude/skills/diachron/rust
cargo build --release

# Option 2: Use Python hook (slower but always works)
# Edit ~/.claude/settings.json and use:
"command": "python3 ~/.claude/skills/diachron/lib/hook_capture.py"
```

#### C. Claude Code Not Restarted

**Important:** After changing `settings.json`, you must restart Claude Code.

```bash
# Close Claude Code completely, then reopen
# Or use the reload command if available
```

#### D. Diachron Not Initialized in Current Project

The hook only captures events when `.diachron/` exists in the project root.

```bash
# Check if initialized
ls .diachron/events.db

# Initialize if missing
/diachron init
```

---

### 3. Exit Code 137 (SIGKILL)

**Symptom:** Hook errors with exit code 137, no stderr output.

```
PostToolUse:Bash hook error: Failed with non-blocking status code: No stderr output
```

**Cause:** The binary is being killed by the system, often due to:
- Binary corruption during copy
- macOS filesystem caching issues
- Sandbox restrictions

**Solutions:**

#### A. Use Direct Path (Recommended)

Edit `~/.claude/settings.json` to point directly to the build output:

```json
"command": "~/.claude/skills/diachron/rust/target/release/diachron-hook"
```

**Not:** `~/.claude/skills/diachron/bin/diachron-hook` (copies can corrupt)

#### B. Rebuild the Binary

```bash
cd ~/.claude/skills/diachron/rust
cargo clean
cargo build --release
sync  # Ensure filesystem writes complete
```

#### C. Verify Binary Works

```bash
# Test with empty input (should exit 0)
echo '{}' | ~/.claude/skills/diachron/rust/target/release/diachron-hook
echo $?  # Must be 0

# Test with real input
cd /path/to/project/with/.diachron
echo '{"tool_name":"Write","tool_input":{"file_path":"test.txt"},"cwd":"'$(pwd)'"}' | \
  ~/.claude/skills/diachron/rust/target/release/diachron-hook
echo $?  # Must be 0
```

#### D. Fall Back to Python

If Rust continues to fail:

```json
"command": "python3 ~/.claude/skills/diachron/lib/hook_capture.py"
```

Python is ~26x slower but more reliable.

---

### 4. Database Locked Errors

**Symptom:** `sqlite3.OperationalError: database is locked`

**Cause:** Multiple processes trying to write simultaneously.

**Solutions:**

#### A. Wait and Retry

Usually resolves itself. Try again in a few seconds.

#### B. Check for Zombie Processes

```bash
# Find processes using the database
lsof .diachron/events.db

# Kill any stuck processes
kill <pid>
```

#### C. Copy and Replace (Last Resort)

```bash
cd .diachron
cp events.db events.db.backup
sqlite3 events.db.backup "VACUUM"
mv events.db.backup events.db
```

---

### 5. AI Summarization Not Working

**Symptom:** `/timeline --summarize` fails or shows "API error".

#### A. Missing Anthropic API Key

The Rust daemon uses Anthropic's Claude API for summarization:

```bash
# Check if key is set
echo $ANTHROPIC_API_KEY

# Set it if missing
export ANTHROPIC_API_KEY="sk-ant-..."
```

Add to your shell profile (`~/.zshrc` or `~/.bashrc`):

```bash
export ANTHROPIC_API_KEY="sk-ant-your-key-here"
```

Alternatively, add to `~/.diachron/config.toml`:

```toml
[summarization]
anthropic_api_key = "sk-ant-your-key-here"
```

#### B. Daemon Not Running

The Rust daemon handles summarization. Make sure it's running:

```bash
diachron doctor  # Check status
diachron daemon start  # Start if needed
```

#### C. API Rate Limits

If you're hitting rate limits, reduce the batch size:

```bash
/timeline --summarize --limit 10
```

---

### 6. Timeline Shows Wrong Timestamps

**Symptom:** Events show incorrect times or "Unknown" timestamps.

**Cause:** The `pst-timestamp` utility is not available or failing.

**Solution:**

The Rust hook generates timestamps internally (no external dependency). If using Python hook:

```bash
# Check if pst-timestamp exists
which pst-timestamp

# If missing, the hook falls back to system time
# Timestamps will be in UTC instead of PST
```

---

### 7. Hook Slowing Down Claude Code

**Symptom:** Noticeable delay after every Write/Edit/Bash command.

**Cause:** Using Python hook (~300ms) instead of Rust (~12ms).

**Solution:**

1. Verify you're using the Rust binary:

```bash
grep "diachron-hook" ~/.claude/settings.json
# Should show: rust/target/release/diachron-hook
```

2. If using Python, switch to Rust:

```bash
# Build Rust binary
cd ~/.claude/skills/diachron/rust
cargo build --release

# Update settings.json to use Rust path
```

---

## Advanced Debugging

### Enable Debug Logging

For the Python hook, add debug output:

```bash
# Edit ~/.claude/skills/diachron/lib/hook_capture.py
# Add at the top of main():
import sys
print(f"DEBUG: Input received", file=sys.stderr)
```

### Inspect Raw Events

```bash
# View all columns for recent events
sqlite3 .diachron/events.db "SELECT * FROM events ORDER BY id DESC LIMIT 5"

# Check metadata JSON
sqlite3 .diachron/events.db "SELECT id, metadata FROM events WHERE metadata IS NOT NULL LIMIT 5"
```

### Test Hook Manually

```bash
# Simulate a Write event
echo '{
  "tool_name": "Write",
  "tool_input": {
    "file_path": "test.txt",
    "content": "hello world"
  },
  "cwd": "'$(pwd)'"
}' | ~/.claude/skills/diachron/rust/target/release/diachron-hook

# Check if event was captured
sqlite3 .diachron/events.db "SELECT * FROM events ORDER BY id DESC LIMIT 1"
```

### Check Hook Output in Real-Time

```bash
# Watch for new events as you work
watch -n 1 'sqlite3 .diachron/events.db "SELECT id, timestamp_display, tool_name, file_path FROM events ORDER BY id DESC LIMIT 5"'
```

---

## Getting Help

If you're still stuck:

1. **Check the plan file** for known issues: `~/.claude/plans/partitioned-forging-muffin.md`

2. **Report an issue** on GitHub: [github.com/wolfiesch/diachron/issues](https://github.com/wolfiesch/diachron/issues)

3. **Include diagnostic info:**

```bash
# System info
uname -a
python3 --version

# Diachron status
ls -la ~/.claude/skills/diachron/
ls -la .diachron/

# Hook config
grep -A 10 "PostToolUse" ~/.claude/settings.json

# Recent events (if any)
sqlite3 .diachron/events.db "SELECT COUNT(*) FROM events"
```

---

## FAQ

**Q: Can I use Diachron on Windows?**

A: Untested. The Rust binary is built for macOS ARM64. You'd need to build from source.

**Q: Will Diachron capture events in all my projects?**

A: No, only projects where you run `/diachron init`. Each project has its own `.diachron/` directory.

**Q: How much disk space does the database use?**

A: Typically <1MB for thousands of events. Events store diffs and metadata, not full file contents.

**Q: Can I sync events across machines?**

A: Not currently. The `.diachron/` directory is local-only and added to `.gitignore` by default.

**Q: How do I completely reset Diachron?**

A: Delete the `.diachron/` directory and run `/diachron init` again:

```bash
rm -rf .diachron/
/diachron init
```
