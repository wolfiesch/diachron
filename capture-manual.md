---
name: diachron-capture
description: Manually capture a file event to the Diachron timeline
user-invocable: true
allowed-tools:
  - Bash(python3 *)
  - Read(*)
---

# /diachron capture - Manual Event Capture

Use this command to manually capture a file event to the Diachron timeline. This is useful when:
- The automatic hook missed an event
- You want to capture a Read operation (normally not captured)
- You want to add a custom annotation to the timeline

## Usage

The user will specify:
- A file path (required)
- An operation type: create, modify, delete, commit, or custom
- An optional description

## Instructions

1. **Parse the user's request** to extract:
   - `file_path`: The file being captured
   - `operation`: One of create, modify, delete, commit, execute, or a custom operation
   - `description`: Optional details about the change

2. **Check if Diachron is enabled**:

```bash
python3 -c "from pathlib import Path; print('enabled' if Path('.diachron').exists() else 'disabled')"
```

If disabled, inform the user to run `/diachron init` first.

3. **Capture the event** using the CLI:

```bash
python3 ~/.claude/skills/diachron/lib/hook_capture.py \
  --tool Manual \
  --path "<FILE_PATH>" \
  --operation "<OPERATION>" \
  --summary "<DESCRIPTION>"
```

4. **Confirm success** to the user with a brief message like:
   "✓ Captured: <operation> on <file_path>"

## Examples

**User:** "/diachron capture src/app.py modified - added error handling"

**Response:**
```bash
python3 ~/.claude/skills/diachron/lib/hook_capture.py \
  --tool Manual \
  --path "src/app.py" \
  --operation "modify" \
  --summary "added error handling"
```
✓ Captured: modify on src/app.py

**User:** "/diachron capture README.md"

**Response:**
```bash
python3 ~/.claude/skills/diachron/lib/hook_capture.py \
  --tool Manual \
  --path "README.md" \
  --operation "modify" \
  --summary ""
```
✓ Captured: modify on README.md
