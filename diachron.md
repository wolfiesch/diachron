---
name: diachron
description: Manage Diachron provenance tracking for your project
allowed-tools:
  - Bash(python3 *)
  - Bash(mkdir *)
  - Write(*)
  - Read(*)
user-invocable: true
---

# Diachron - Agentic Provenance System

Diachron automatically tracks AI-assisted code changes in your project, creating a queryable timeline of modifications.

## Commands

```
/diachron init          # Initialize Diachron for this project
/diachron status        # Show current tracking status
/diachron config        # Show/edit configuration
/diachron repair        # Repair corrupted database
/diachron export        # Export timeline (alias for /timeline --export)
/diachron clean         # Clean old events based on retention policy
```

## /diachron init

Initialize Diachron for the current project. This will:
1. Create the `.diachron/` directory
2. Initialize the SQLite database
3. Create default configuration

**Steps:**

1. Check if already initialized:
```bash
test -d .diachron && echo "exists" || echo "new"
```

2. If "exists", ask user if they want to reinitialize (this will NOT delete existing data).

3. Create the directory structure:
```bash
mkdir -p .diachron/overflow
```

4. Create default config file at `.diachron/config.json`:
```json
{
  "version": 1,
  "ai_summaries": false,
  "capture_tools": ["Write", "Edit", "Bash"],
  "bash_capture_mode": "file_modifying_only",
  "max_inline_diff_lines": 50,
  "retention_days": 90,
  "exclude_patterns": [
    "node_modules/**",
    "*.lock",
    ".git/**",
    "dist/**",
    "build/**",
    "__pycache__/**",
    "*.pyc"
  ]
}
```

5. Initialize the database by running:
```bash
python3 << 'EOF'
import sys
from pathlib import Path
sys.path.insert(0, str(Path.home() / ".claude/skills/diachron/lib"))
from db import DiachronDB
db = DiachronDB()
db._get_connection()  # This initializes the schema
db.close()
print("Database initialized successfully")
EOF
```

6. Add `.diachron/` to `.gitignore` if it exists and doesn't already include it:
```bash
if [ -f .gitignore ]; then
    grep -q "^\.diachron" .gitignore || echo -e "\n# Diachron provenance data\n.diachron/" >> .gitignore
fi
```

7. Print success message:
```
✅ Diachron initialized!

Your project is now tracking AI-assisted changes.

• Timeline: /timeline
• Stats: /timeline --stats
• Export: /timeline --export markdown

Configuration: .diachron/config.json
Database: .diachron/events.db
```

## /diachron status

Show current Diachron status for this project.

```bash
python3 << 'EOF'
import sys
import json
from pathlib import Path

if not Path(".diachron").exists():
    print("❌ Diachron is not initialized in this project.")
    print("   Run /diachron init to set it up.")
    sys.exit(0)

sys.path.insert(0, str(Path.home() / ".claude/skills/diachron/lib"))
from db import DiachronDB

db = DiachronDB()
stats = db.get_stats()
db.close()

config_path = Path(".diachron/config.json")
config = json.loads(config_path.read_text()) if config_path.exists() else {}

print("📊 Diachron Status")
print("━" * 40)
print(f"  Initialized: ✅ Yes")
print(f"  Total Events: {stats['total_events']}")
print(f"  Sessions: {stats['total_sessions']}")
print(f"  Files Tracked: {stats['unique_files']}")
print(f"  AI Summaries: {'✅ On' if config.get('ai_summaries') else '❌ Off'}")
print(f"  Retention: {config.get('retention_days', 90)} days")
print("━" * 40)
EOF
```

## /diachron config

Show or edit Diachron configuration.

**To show current config:**
```bash
cat .diachron/config.json | python3 -m json.tool
```

**To enable AI summaries:**
Update `.diachron/config.json` and set `"ai_summaries": true`

**To change retention period:**
Update `"retention_days"` in the config (e.g., 30 for 30 days, 365 for a year)

**To exclude additional patterns:**
Add to the `"exclude_patterns"` array (uses glob syntax)

## /diachron repair

Attempt to repair a corrupted database.

```bash
python3 << 'EOF'
import sqlite3
from pathlib import Path

db_path = Path(".diachron/events.db")
if not db_path.exists():
    print("No database found. Run /diachron init first.")
else:
    try:
        conn = sqlite3.connect(str(db_path))
        conn.execute("PRAGMA integrity_check")
        result = conn.fetchone()
        if result and result[0] == "ok":
            print("✅ Database integrity check passed.")
        else:
            print("⚠️ Database may have issues. Consider backing up and reinitializing.")
        conn.close()
    except Exception as e:
        print(f"❌ Error: {e}")
EOF
```

## /diachron clean

Clean old events based on retention policy.

```bash
python3 << 'EOF'
import sys
import json
from pathlib import Path
from datetime import datetime, timedelta

sys.path.insert(0, str(Path.home() / ".claude/skills/diachron/lib"))
from db import DiachronDB

config = json.loads(Path(".diachron/config.json").read_text())
retention_days = config.get("retention_days", 90)
cutoff = (datetime.now() - timedelta(days=retention_days)).isoformat()

db = DiachronDB()
conn = db._get_connection()
cursor = conn.cursor()

cursor.execute("SELECT COUNT(*) FROM events WHERE timestamp < ?", (cutoff,))
old_count = cursor.fetchone()[0]

if old_count > 0:
    print(f"Found {old_count} events older than {retention_days} days.")
    cursor.execute("DELETE FROM events WHERE timestamp < ?", (cutoff,))
    conn.commit()
    print(f"✅ Deleted {old_count} old events.")
else:
    print("No events to clean.")

db.close()
EOF
```

## Configuration Reference

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `ai_summaries` | boolean | false | Generate AI summaries for changes |
| `capture_tools` | array | ["Write", "Edit", "Bash"] | Tools to capture events from |
| `bash_capture_mode` | string | "file_modifying_only" | "all" or "file_modifying_only" |
| `max_inline_diff_lines` | number | 50 | Max diff lines stored inline |
| `retention_days` | number | 90 | Days to keep events |
| `exclude_patterns` | array | [...] | Glob patterns to ignore |

## How It Works

Diachron uses Claude Code 2.1's **PostToolUse hooks** to automatically capture file modifications:

1. When you (or Claude) use Write, Edit, or Bash tools, the `diachron-capture` hook fires
2. The hook extracts file path, operation type, and diff information
3. Events are stored in a local SQLite database
4. Use `/timeline` to view your project's change history

No manual logging required - it's completely transparent!
