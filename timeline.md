---
name: timeline
description: View the timeline of AI-assisted changes in your project
allowed-tools:
  - Bash(python3 *)
  - Read(*)
user-invocable: true
---

# Diachron Timeline Viewer

Display the timeline of AI-assisted code changes in the current project.

## Usage

```
/timeline                           # Show last 20 events
/timeline --since "1 hour ago"      # Events from the last hour
/timeline --since "yesterday"       # Events since yesterday
/timeline --file src/               # Events affecting files in src/
/timeline --tool Edit               # Only Edit tool events
/timeline --verbose                 # Show full details
/timeline --stats                   # Show statistics
/timeline --export markdown         # Export to TIMELINE.md
```

## Instructions

When the user invokes `/timeline`, follow these steps:

### Step 1: Check Initialization

First verify Diachron is set up:

```bash
python3 -c "from pathlib import Path; print('yes' if Path('.diachron/events.db').exists() else 'no')"
```

If "no", inform the user:
> Diachron is not initialized for this project. Run `/diachron init` to set it up.

### Step 2: Parse Arguments

Parse the user's arguments to determine:
- `--since` / `-s`: Time filter (e.g., "1 hour ago", "yesterday", "2024-01-01")
- `--until` / `-u`: Upper time bound
- `--file` / `-f`: File path filter (prefix match)
- `--tool` / `-t`: Tool name filter (Write, Edit, Bash)
- `--session`: Filter by session ID
- `--limit` / `-n`: Number of results (default: 20)
- `--verbose` / `-v`: Show full details including raw input
- `--stats`: Show database statistics instead of events
- `--export`: Export format (markdown, json)

### Step 3: Query Events

Use the Python query helper:

```bash
python3 << 'EOF'
import sys
import json
sys.path.insert(0, str(__import__('pathlib').Path.home() / ".claude/skills/diachron/lib"))
from db import DiachronDB

db = DiachronDB()
events = db.query_events(
    since="<SINCE_VALUE>",      # or None
    file_path="<FILE_PATH>",    # or None
    tool_name="<TOOL_NAME>",    # or None
    limit=<LIMIT>
)

for e in events:
    print(json.dumps(e))

db.close()
EOF
```

### Step 4: Format Output

Format the results as a clean timeline:

**Standard Format:**
```
📍 Timeline for [project-name]
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

🕐 01/08/2026 03:45 AM PST
   ├─ Tool: Edit
   ├─ File: src/components/Button.tsx
   ├─ Operation: modify
   └─ Change: Updated onClick handler to use async/await

🕐 01/08/2026 03:42 AM PST
   ├─ Tool: Write
   ├─ File: src/utils/api.ts
   ├─ Operation: create
   └─ Change: +78 lines

🕐 01/08/2026 03:40 AM PST
   ├─ Tool: Bash
   ├─ Operation: commit
   └─ Change: feat: add authentication module

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Showing 3 of 42 events • Session: a1b2c3d4
```

**Verbose Format (with --verbose):**
Include the raw input/output for each event.

**Stats Format (with --stats):**
```
📊 Diachron Statistics
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Total Events:     142
Total Sessions:   8
Unique Files:     34
First Event:      01/05/2026 10:30 AM PST
Last Event:       01/08/2026 03:45 AM PST

By Tool:
  • Edit:   67 events (47%)
  • Write:  52 events (37%)
  • Bash:   23 events (16%)

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

### Step 5: Handle Export

If `--export markdown` is specified, create a TIMELINE.md file:

```markdown
# Project Timeline

Generated: 01/08/2026 03:50 AM PST

## Recent Changes

### 01/08/2026

#### 03:45 AM - Modified src/components/Button.tsx
- **Tool:** Edit
- **Change:** Updated onClick handler to use async/await

#### 03:42 AM - Created src/utils/api.ts
- **Tool:** Write
- **Change:** +78 lines, API client for authentication

...
```

If `--export json` is specified, output raw JSON.

## Examples

**Show today's changes:**
```
/timeline --since today
```

**Show changes to a specific file:**
```
/timeline --file src/components/Auth.tsx
```

**Show all commits:**
```
/timeline --tool Bash --since "1 week ago"
```

**Export for documentation:**
```
/timeline --export markdown --since "1 week ago"
```

## Error Handling

- If no events match the filters, show: "No events found matching your criteria."
- If database is corrupted, suggest: "Database may be corrupted. Try `/diachron repair`."
- Always handle errors gracefully without disrupting the user's workflow.
