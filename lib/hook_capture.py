#!/usr/bin/env python3
"""
Diachron Hook Capture - Rust-Ready Architecture
================================================
Fast event capture for Claude Code PostToolUse hooks.

Design goals:
- Minimal startup time (<100ms)
- Clean data structures (maps to Rust structs)
- Simple pattern matching (like Rust match)
- Zero external dependencies (stdlib only)

Future Rust port:
- dataclass Event -> struct Event
- classify_bash_command() -> match expression
- JSON parsing -> serde_json
- SQLite -> rusqlite

Usage:
    # From Claude Code hook (JSON on stdin):
    echo '{"tool":"Write","path":"/foo/bar.py",...}' | python3 hook_capture.py

    # Direct CLI:
    python3 hook_capture.py --tool Write --path /foo/bar.py --operation create
"""

from __future__ import annotations

import json
import sys
from dataclasses import dataclass, asdict
from enum import Enum
from pathlib import Path
from typing import Optional

# ============================================================================
# DATA STRUCTURES (Rust: these become structs)
# ============================================================================

class Operation(Enum):
    """File operation types. Rust: enum Operation { Create, Modify, ... }"""
    CREATE = "create"
    MODIFY = "modify"
    DELETE = "delete"
    MOVE = "move"
    COPY = "copy"
    COMMIT = "commit"
    EXECUTE = "execute"
    UNKNOWN = "unknown"


@dataclass
class CaptureEvent:
    """
    Event to capture. Rust equivalent:

    struct CaptureEvent {
        tool_name: String,
        file_path: Option<String>,
        operation: Operation,
        diff_summary: Option<String>,
        raw_input: Option<String>,
    }
    """
    tool_name: str
    file_path: Optional[str] = None
    operation: Operation = Operation.UNKNOWN
    diff_summary: Optional[str] = None
    raw_input: Optional[str] = None

    def to_db_args(self) -> dict:
        """Convert to database insert arguments."""
        return {
            "tool_name": self.tool_name,
            "file_path": self.file_path,
            "operation": self.operation.value,
            "diff_summary": self.diff_summary,
            "raw_input": self.raw_input,
        }


@dataclass
class HookInput:
    """
    Input from Claude Code PostToolUse hook. Rust equivalent:

    struct HookInput {
        session_id: String,
        tool_name: String,
        tool_input: serde_json::Value,
        tool_result: Option<String>,
        timestamp: String,
        cwd: Option<String>,
    }
    """
    tool_name: str
    tool_input: dict
    tool_result: Optional[str] = None
    session_id: Optional[str] = None
    timestamp: Optional[str] = None
    cwd: Optional[str] = None

    @classmethod
    def from_json(cls, data: dict) -> HookInput:
        return cls(
            tool_name=data.get("tool_name", data.get("tool", "")),  # Support both formats
            tool_input=data.get("tool_input", {}),
            tool_result=data.get("tool_result", data.get("tool_output")),
            session_id=data.get("session_id"),
            timestamp=data.get("timestamp"),
            cwd=data.get("cwd"),
        )


# ============================================================================
# COMMAND CLASSIFICATION (Rust: match expressions)
# ============================================================================

# Bash commands that modify files - worth capturing
FILE_MODIFYING_COMMANDS = {
    "git commit": Operation.COMMIT,
    "git merge": Operation.COMMIT,
    "rm": Operation.DELETE,
    "rm -rf": Operation.DELETE,
    "rm -r": Operation.DELETE,
    "mv": Operation.MOVE,
    "cp": Operation.COPY,
    "touch": Operation.CREATE,
    "mkdir": Operation.CREATE,
    "mkdir -p": Operation.CREATE,
    "chmod": Operation.MODIFY,
    "chown": Operation.MODIFY,
}

# Commands to skip - read-only, not worth capturing
SKIP_COMMANDS = frozenset([
    "ls", "cat", "head", "tail", "less", "more",
    "grep", "rg", "find", "fd", "ag",
    "git status", "git log", "git diff", "git branch", "git show",
    "pwd", "cd", "echo", "printf", "which", "whereis",
    "ps", "top", "htop", "df", "du",
    "python3 -c", "node -e",  # One-liners for checking
])


def classify_bash_command(command: str) -> tuple[Operation, Optional[str]]:
    """
    Classify a bash command to determine if it's file-modifying.

    Returns (Operation, optional_detail)

    Rust equivalent:
        fn classify_bash_command(cmd: &str) -> (Operation, Option<String>)
    """
    cmd = command.strip()
    cmd_lower = cmd.lower()

    # Skip read-only commands
    for skip in SKIP_COMMANDS:
        if cmd_lower.startswith(skip):
            return (Operation.UNKNOWN, None)

    # Check file-modifying commands
    for pattern, op in FILE_MODIFYING_COMMANDS.items():
        if cmd_lower.startswith(pattern):
            # Extract detail (e.g., commit message, file path)
            detail = extract_command_detail(cmd, pattern)
            return (op, detail)

    # Default: unknown, might be interesting
    return (Operation.EXECUTE, None)


def extract_command_detail(cmd: str, pattern: str) -> Optional[str]:
    """Extract meaningful detail from a command."""
    if pattern == "git commit":
        # Extract commit message
        if "-m" in cmd:
            parts = cmd.split("-m")
            if len(parts) > 1:
                msg = parts[1].strip().strip('"').strip("'")
                # Handle heredoc style
                if "<<" in msg:
                    msg = msg.split("<<")[0].strip()
                return msg[:200] if msg else None
    elif pattern in ("rm", "rm -rf", "rm -r"):
        # Extract deleted path
        parts = cmd.split()
        for part in parts[1:]:
            if not part.startswith("-"):
                return part
    elif pattern in ("mv", "cp"):
        # Extract source -> dest
        parts = cmd.split()
        non_flags = [p for p in parts[1:] if not p.startswith("-")]
        if len(non_flags) >= 2:
            return f"{non_flags[0]} → {non_flags[-1]}"

    return None


# ============================================================================
# EVENT PARSING (Rust: impl From<HookInput> for CaptureEvent)
# ============================================================================

def parse_write_event(hook: HookInput) -> CaptureEvent:
    """Parse a Write tool event."""
    file_path = hook.tool_input.get("file_path", "")
    content = hook.tool_input.get("content", "")

    # Determine if create or modify based on result
    operation = Operation.CREATE
    if hook.tool_result and "overwritten" in str(hook.tool_result).lower():
        operation = Operation.MODIFY

    # Diff summary: line count
    line_count = content.count("\n") + 1 if content else 0
    diff_summary = f"+{line_count} lines"

    return CaptureEvent(
        tool_name="Write",
        file_path=file_path,
        operation=operation,
        diff_summary=diff_summary,
        raw_input=content[:500] if content else None,
    )


def parse_edit_event(hook: HookInput) -> CaptureEvent:
    """Parse an Edit tool event."""
    file_path = hook.tool_input.get("file_path", "")
    old_string = hook.tool_input.get("old_string", "")
    new_string = hook.tool_input.get("new_string", "")

    # Diff summary
    old_lines = old_string.count("\n") + 1 if old_string else 0
    new_lines = new_string.count("\n") + 1 if new_string else 0
    diff = new_lines - old_lines

    if diff > 0:
        diff_summary = f"+{diff} lines"
    elif diff < 0:
        diff_summary = f"{diff} lines"
    else:
        diff_summary = "modified (same line count)"

    return CaptureEvent(
        tool_name="Edit",
        file_path=file_path,
        operation=Operation.MODIFY,
        diff_summary=diff_summary,
        raw_input=f"old: {old_string[:100]}... → new: {new_string[:100]}..." if old_string else None,
    )


def parse_bash_event(hook: HookInput) -> Optional[CaptureEvent]:
    """
    Parse a Bash tool event.
    Returns None if the command should be skipped.
    """
    command = hook.tool_input.get("command", "")

    operation, detail = classify_bash_command(command)

    # Skip uninteresting commands
    if operation == Operation.UNKNOWN:
        return None

    return CaptureEvent(
        tool_name="Bash",
        file_path=None,  # Bash commands don't always have a single file
        operation=operation,
        diff_summary=detail,
        raw_input=command[:500],
    )


def parse_hook_input(hook: HookInput) -> Optional[CaptureEvent]:
    """
    Parse hook input into a capture event.
    Returns None if the event should not be captured.

    Rust equivalent:
        fn parse_hook_input(hook: HookInput) -> Option<CaptureEvent>
    """
    tool = hook.tool_name

    # Pattern matching (Rust: match tool.as_str() { ... })
    if tool == "Write":
        return parse_write_event(hook)
    elif tool == "Edit":
        return parse_edit_event(hook)
    elif tool == "Bash":
        return parse_bash_event(hook)
    else:
        # Unknown tool - skip
        return None


# ============================================================================
# DATABASE INTEGRATION
# ============================================================================

def save_event(event: CaptureEvent, project_root: Optional[Path] = None) -> int:
    """
    Save event to database.
    Returns event ID or -1 on failure.
    """
    try:
        # Import db module (lazy load for speed when skipping events)
        from db import DiachronDB

        db = DiachronDB(project_root)
        event_id = db.insert_event(**event.to_db_args())
        db.close()
        return event_id
    except Exception as e:
        # Log error but don't crash - hooks should be silent
        print(f"Error saving event: {e}", file=sys.stderr)
        return -1


def find_project_root(start_path: Optional[Path] = None) -> Optional[Path]:
    """
    Find project root by walking up from start_path.
    Returns None if no .diachron directory found.
    """
    current = start_path or Path.cwd()
    while current != current.parent:
        if (current / ".diachron").exists():
            return current
        if (current / ".git").exists():
            # Found git root - check if .diachron exists here
            if (current / ".diachron").exists():
                return current
            return None  # Git repo without Diachron
        current = current.parent
    return None


def is_diachron_enabled(project_root: Optional[Path] = None) -> bool:
    """Check if Diachron is enabled for the current project."""
    if project_root:
        return (project_root / ".diachron").exists()
    return find_project_root() is not None


# ============================================================================
# MAIN ENTRY POINT
# ============================================================================

def main():
    """
    Main entry point for hook capture.

    Accepts input in two ways:
    1. JSON on stdin (from Claude Code hook)
    2. CLI arguments (for manual testing)
    """
    # Parse input
    if len(sys.argv) > 1 and sys.argv[1] == "--help":
        print(__doc__)
        sys.exit(0)

    project_root: Optional[Path] = None
    event: Optional[CaptureEvent] = None

    # Check for CLI args
    if len(sys.argv) > 1 and sys.argv[1].startswith("--"):
        event = parse_cli_args()
    else:
        # Read JSON from stdin
        try:
            input_data = sys.stdin.read()
            if not input_data.strip():
                sys.exit(0)
            data = json.loads(input_data)
            hook = HookInput.from_json(data)

            # Use cwd from hook if provided (hooks may run from different directory)
            if hook.cwd:
                project_root = find_project_root(Path(hook.cwd))
            else:
                project_root = find_project_root()

            event = parse_hook_input(hook)
        except json.JSONDecodeError as e:
            print(f"Invalid JSON input: {e}", file=sys.stderr)
            sys.exit(1)

    # Check if Diachron is enabled (using detected project root)
    if not is_diachron_enabled(project_root):
        sys.exit(0)

    # Skip if no event to capture
    if event is None:
        sys.exit(0)

    # Save event
    event_id = save_event(event, project_root)

    # Silent success (hooks should not produce output)
    sys.exit(0 if event_id > 0 else 1)


def parse_cli_args() -> Optional[CaptureEvent]:
    """Parse CLI arguments for manual capture."""
    import argparse

    parser = argparse.ArgumentParser(description="Capture a Diachron event")
    parser.add_argument("--tool", required=True, help="Tool name (Write, Edit, Bash)")
    parser.add_argument("--path", help="File path")
    parser.add_argument("--operation", default="unknown", help="Operation type")
    parser.add_argument("--summary", help="Diff summary")
    parser.add_argument("--raw", help="Raw input (truncated)")

    args = parser.parse_args()

    try:
        operation = Operation(args.operation)
    except ValueError:
        operation = Operation.UNKNOWN

    return CaptureEvent(
        tool_name=args.tool,
        file_path=args.path,
        operation=operation,
        diff_summary=args.summary,
        raw_input=args.raw,
    )


if __name__ == "__main__":
    main()
