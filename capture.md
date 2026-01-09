---
name: diachron-capture
description: Captures file modification events for timeline provenance (PostToolUse hook)
hooks:
  - type: PostToolUse
    matcher: Write|Edit|Bash
allowed-tools:
  - Bash(python3 *)
  - Bash(git diff *)
  - Bash(git log *)
  - Read(*)
user-invocable: false
---

# Diachron Capture Hook

You are a PostToolUse hook that captures file modification events for the Diachron provenance system.

## When This Hook Fires

This hook fires AFTER any of these tools complete:
- **Write** - File creation/overwrite
- **Edit** - File modification
- **Bash** - Only for file-modifying commands (git commit, rm, mv, cp, touch, mkdir)

## Your Task

When this hook fires, you must:

1. **Extract the file path** from the tool result
2. **Determine the operation type** (create, modify, delete, execute)
3. **Generate a brief diff summary** (lines added/removed)
4. **Insert the event** into the Diachron database

## Step-by-Step Instructions

### Step 1: Check if .diachron exists

First, check if Diachron is initialized for this project:

```bash
python3 -c "from pathlib import Path; p = Path('.diachron'); print('yes' if p.exists() else 'no')"
```

If "no", do nothing and exit silently. Diachron is not enabled for this project.

### Step 2: Extract Event Data

Based on the tool that fired:

**For Write tool:**
- `file_path`: The file that was written
- `operation`: "create" if file was new, "modify" if it existed
- `diff_summary`: Line count of the new content

**For Edit tool:**
- `file_path`: The file that was edited
- `operation`: "modify"
- `diff_summary`: Brief description of the change (from old_string → new_string)

**For Bash tool:**
- Only capture if the command is file-modifying:
  - `git commit` → operation: "commit", extract commit message
  - `rm` → operation: "delete", file_path from command
  - `mv` → operation: "move", both source and dest
  - `cp` → operation: "copy"
  - `touch` → operation: "create"
  - `mkdir` → operation: "create_dir"
- Skip non-file-modifying commands (ls, grep, cat, etc.)

### Step 3: Insert into Database

Run the following Python command to insert the event:

```bash
python3 ~/.claude/skills/diachron/lib/db.py insert
```

Or use inline Python:

```python
import sys
sys.path.insert(0, str(Path.home() / ".claude/skills/diachron/lib"))
from db import DiachronDB

db = DiachronDB()
db.insert_event(
    tool_name="<TOOL_NAME>",
    file_path="<FILE_PATH>",
    operation="<OPERATION>",
    diff_summary="<DIFF_SUMMARY>",
    raw_input="<TRUNCATED_INPUT>"
)
db.close()
```

### Step 4: Exit Silently

After inserting the event, exit without any output. This hook should be invisible to the user.

## File-Modifying Bash Commands to Capture

Only capture these bash commands:
- `git commit` - Capture the commit message
- `git merge` - Capture the merge info
- `rm`, `rm -rf` - Capture deleted files
- `mv` - Capture source → destination
- `cp` - Capture source → destination
- `touch` - Capture created file
- `mkdir` - Capture created directory
- `chmod`, `chown` - Capture permission changes

## Commands to SKIP (read-only, not file-modifying)

Do NOT capture these:
- `ls`, `cat`, `head`, `tail`
- `grep`, `rg`, `find`, `fd`
- `git status`, `git log`, `git diff` (without commit)
- `pwd`, `cd`, `echo`, `printf`
- `npm install`, `pip install` (handled separately if needed)

## Error Handling

- If the database insert fails, log the error but don't interrupt the workflow
- Never throw errors that would block the user's work
- This hook should be completely transparent

## Example Captures

**Write tool creates src/utils.ts:**
```json
{
  "tool_name": "Write",
  "file_path": "src/utils.ts",
  "operation": "create",
  "diff_summary": "+45 lines"
}
```

**Edit tool modifies package.json:**
```json
{
  "tool_name": "Edit",
  "file_path": "package.json",
  "operation": "modify",
  "diff_summary": "Changed version: 1.0.0 → 1.1.0"
}
```

**Bash runs git commit:**
```json
{
  "tool_name": "Bash",
  "file_path": null,
  "operation": "commit",
  "diff_summary": "feat: add user authentication"
}
```
