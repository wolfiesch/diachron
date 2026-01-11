#!/usr/bin/env python3
"""
Diachron Codex Capture Module
=============================
Parses OpenAI Codex CLI session JSONL logs to extract file operations
and sends them to the Diachron daemon for unified provenance tracking.

Usage:
    # After a Codex session completes:
    python3 codex_capture.py --jsonl /path/to/session.jsonl

    # With parent session correlation (for /handoffcodex integration):
    python3 codex_capture.py --jsonl /path/to/session.jsonl \
        --parent-session "claude-abc123" \
        --git-branch "feature/oauth"

    # Auto-discover most recent session:
    python3 codex_capture.py --latest
"""

import argparse
import json
import os
import re
import socket
import sys
from datetime import datetime
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

# ==============================================================================
# CHANGELOG (recent first, max 5 entries)
# 01/11/2026 - Initial implementation for Diachron v0.7 (Claude)
# ==============================================================================

# Codex session log location
CODEX_SESSIONS_DIR = Path.home() / ".codex" / "sessions"

# Diachron daemon socket
DIACHRON_SOCKET = Path.home() / ".diachron" / "diachron.sock"

# File-modifying shell commands to capture
FILE_MODIFYING_COMMANDS = {
    "git commit": "git",
    "git add": "git",
    "git rm": "git",
    "git mv": "git",
    "rm ": "fileops",
    "rm -": "fileops",
    "mv ": "fileops",
    "cp ": "fileops",
    "touch ": "fileops",
    "mkdir ": "fileops",
    "rmdir ": "fileops",
    "> ": "fileops",  # Redirect to file
    ">> ": "fileops",  # Append to file
    "cat >": "fileops",
    "echo >": "fileops",
    "npm install": "package",
    "npm uninstall": "package",
    "yarn add": "package",
    "yarn remove": "package",
    "pip install": "package",
    "pip uninstall": "package",
    "cargo add": "package",
    "cargo remove": "package",
}


def find_latest_session() -> Optional[Path]:
    """Find the most recent Codex session JSONL file.

    Returns:
        Path to the most recent session file, or None if not found.
    """
    if not CODEX_SESSIONS_DIR.exists():
        return None

    # Find all JSONL files recursively
    jsonl_files = list(CODEX_SESSIONS_DIR.glob("**/*.jsonl"))
    if not jsonl_files:
        return None

    # Sort by modification time, most recent first
    jsonl_files.sort(key=lambda p: p.stat().st_mtime, reverse=True)
    return jsonl_files[0]


def parse_patch_content(patch_input: str) -> List[Dict[str, Any]]:
    """Parse an apply_patch input to extract file operations.

    Args:
        patch_input: The raw patch content from Codex apply_patch event.

    Returns:
        List of file operation dicts with file_path, operation, and diff_summary.
    """
    operations = []

    # Pattern for file operations in patch format
    # *** Add File: path/to/file.py
    # *** Update File: path/to/file.py
    # *** Delete File: path/to/file.py
    add_pattern = re.compile(r'\*\*\* Add File:\s*(.+?)(?:\n|$)')
    update_pattern = re.compile(r'\*\*\* Update File:\s*(.+?)(?:\n|$)')
    delete_pattern = re.compile(r'\*\*\* Delete File:\s*(.+?)(?:\n|$)')

    # Count lines added/removed for diff summary
    lines_added = len(re.findall(r'^\+[^+]', patch_input, re.MULTILINE))
    lines_removed = len(re.findall(r'^-[^-]', patch_input, re.MULTILINE))

    diff_summary = ""
    if lines_added or lines_removed:
        parts = []
        if lines_added:
            parts.append(f"+{lines_added}")
        if lines_removed:
            parts.append(f"-{lines_removed}")
        diff_summary = " ".join(parts) + " lines"

    # Extract file operations
    for match in add_pattern.finditer(patch_input):
        file_path = match.group(1).strip()
        operations.append({
            "file_path": file_path,
            "operation": "create",
            "diff_summary": diff_summary or f"new file",
        })

    for match in update_pattern.finditer(patch_input):
        file_path = match.group(1).strip()
        operations.append({
            "file_path": file_path,
            "operation": "modify",
            "diff_summary": diff_summary or "updated",
        })

    for match in delete_pattern.finditer(patch_input):
        file_path = match.group(1).strip()
        operations.append({
            "file_path": file_path,
            "operation": "delete",
            "diff_summary": "file deleted",
        })

    return operations


def classify_command(cmd: str) -> Tuple[Optional[str], Optional[str]]:
    """Classify a shell command and extract affected file path if applicable.

    Args:
        cmd: The shell command string.

    Returns:
        Tuple of (category, file_path) or (None, None) if not file-modifying.
    """
    cmd_lower = cmd.lower().strip()

    for pattern, category in FILE_MODIFYING_COMMANDS.items():
        if pattern in cmd_lower:
            # Try to extract file path from command
            # This is best-effort - commands have varying syntax
            file_path = None

            if category == "git" and "commit" in cmd_lower:
                # git commit doesn't have a single file path
                return category, None

            # Try to get last argument as file path
            parts = cmd.split()
            if len(parts) > 1:
                # Skip flags (arguments starting with -)
                for part in reversed(parts):
                    if not part.startswith("-") and part != parts[0]:
                        file_path = part
                        break

            return category, file_path

    return None, None


def parse_codex_jsonl(jsonl_path: Path) -> Dict[str, Any]:
    """Parse a Codex session JSONL file and extract file operations.

    Args:
        jsonl_path: Path to the Codex session JSONL file.

    Returns:
        Dict containing session metadata and list of file operations.
    """
    result = {
        "session_id": None,
        "cwd": None,
        "cli_version": None,
        "timestamp": None,
        "operations": [],
    }

    with open(jsonl_path, "r") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue

            try:
                event = json.loads(line)
            except json.JSONDecodeError:
                continue

            event_type = event.get("type")
            timestamp = event.get("timestamp")
            payload = event.get("payload", {})

            # Extract session metadata
            if event_type == "session_meta":
                result["session_id"] = payload.get("id")
                result["cwd"] = payload.get("cwd")
                result["cli_version"] = payload.get("cli_version")
                result["timestamp"] = timestamp

            # Extract file operations from apply_patch
            elif event_type == "response_item":
                inner_type = payload.get("type")

                if inner_type == "custom_tool_call" and payload.get("name") == "apply_patch":
                    patch_input = payload.get("input", "")
                    file_ops = parse_patch_content(patch_input)

                    for op in file_ops:
                        op["timestamp"] = timestamp
                        op["raw_input"] = patch_input[:500]  # Truncate for storage
                        result["operations"].append(op)

                # Extract file-modifying shell commands
                elif inner_type == "function_call" and payload.get("name") == "exec_command":
                    try:
                        args = json.loads(payload.get("arguments", "{}"))
                        cmd = args.get("cmd", "")

                        # Check if this is an apply_patch via exec_command (newer Codex format)
                        if "apply_patch" in cmd and "*** Begin Patch" in cmd:
                            file_ops = parse_patch_content(cmd)
                            for op in file_ops:
                                op["timestamp"] = timestamp
                                op["raw_input"] = cmd[:500]
                                result["operations"].append(op)
                        else:
                            # Regular command classification
                            category, file_path = classify_command(cmd)
                            if category:
                                result["operations"].append({
                                    "file_path": file_path,
                                    "operation": "execute",
                                    "diff_summary": cmd[:100],  # Truncate long commands
                                    "command_category": category,
                                    "timestamp": timestamp,
                                    "raw_input": cmd,
                                })
                    except json.JSONDecodeError:
                        pass

    return result


def send_to_daemon(
    operations: List[Dict[str, Any]],
    session_id: str,
    parent_session: Optional[str] = None,
    git_branch: Optional[str] = None,
    cli_version: Optional[str] = None,
    cwd: Optional[str] = None,
) -> int:
    """Send captured operations to the Diachron daemon via IPC.

    Args:
        operations: List of file operation dicts.
        session_id: Codex session ID.
        parent_session: Optional Claude parent session ID.
        git_branch: Optional git branch name.
        cli_version: Codex CLI version.
        cwd: Working directory of the Codex session.

    Returns:
        Number of events successfully sent.
    """
    if not DIACHRON_SOCKET.exists():
        print(f"Warning: Diachron daemon not running ({DIACHRON_SOCKET})", file=sys.stderr)
        return 0

    success_count = 0

    for op in operations:
        # Build metadata
        metadata = {
            "codex_session_id": session_id,
            "codex_version": cli_version,
        }
        if parent_session:
            metadata["parent_session_id"] = parent_session
        if git_branch:
            metadata["git_branch"] = git_branch
        if cwd:
            metadata["cwd"] = cwd
        if op.get("command_category"):
            metadata["command_category"] = op["command_category"]

        # Build capture message
        message = {
            "type": "Capture",
            "payload": {
                "tool_name": "Codex",
                "file_path": op.get("file_path"),
                "operation": op.get("operation"),
                "diff_summary": op.get("diff_summary"),
                "raw_input": op.get("raw_input"),
                "metadata": json.dumps(metadata),
                "git_commit_sha": None,
                "command_category": op.get("command_category"),
            }
        }

        try:
            sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            sock.connect(str(DIACHRON_SOCKET))
            sock.sendall((json.dumps(message) + "\n").encode())

            # Read response
            response = b""
            while not response.endswith(b"\n"):
                chunk = sock.recv(4096)
                if not chunk:
                    break
                response += chunk

            sock.close()

            resp_json = json.loads(response.decode())
            if resp_json.get("type") == "Ok":
                success_count += 1
            else:
                print(f"Warning: Daemon error for {op.get('file_path')}: {resp_json}", file=sys.stderr)

        except Exception as e:
            print(f"Warning: Failed to send event: {e}", file=sys.stderr)

    return success_count


def save_to_local_db(
    operations: List[Dict[str, Any]],
    session_id: str,
    parent_session: Optional[str] = None,
    git_branch: Optional[str] = None,
    cli_version: Optional[str] = None,
    cwd: Optional[str] = None,
) -> int:
    """Fallback: Save operations directly to local SQLite database.

    Used when daemon is not running but project has .diachron/ initialized.

    Args:
        operations: List of file operation dicts.
        session_id: Codex session ID.
        parent_session: Optional Claude parent session ID.
        git_branch: Optional git branch name.
        cli_version: Codex CLI version.
        cwd: Working directory of the Codex session.

    Returns:
        Number of events successfully saved.
    """
    # Check if we're in a Diachron-enabled project
    diachron_dir = Path(cwd or ".") / ".diachron"
    if not diachron_dir.exists():
        # Try current directory
        diachron_dir = Path(".diachron")
        if not diachron_dir.exists():
            return 0

    try:
        # Import local db module
        sys.path.insert(0, str(Path(__file__).parent))
        from db import DiachronDB

        db = DiachronDB(diachron_dir)
        success_count = 0

        for op in operations:
            metadata = {
                "codex_session_id": session_id,
                "codex_version": cli_version,
            }
            if parent_session:
                metadata["parent_session_id"] = parent_session
            if git_branch:
                metadata["git_branch"] = git_branch
            if op.get("command_category"):
                metadata["command_category"] = op["command_category"]

            try:
                db.insert_event(
                    tool_name="Codex",
                    file_path=op.get("file_path"),
                    operation=op.get("operation"),
                    diff_summary=op.get("diff_summary"),
                    raw_input=op.get("raw_input"),
                    metadata=metadata,
                )
                success_count += 1
            except Exception as e:
                print(f"Warning: Failed to save event: {e}", file=sys.stderr)

        db.close()
        return success_count

    except ImportError:
        print("Warning: Could not import DiachronDB", file=sys.stderr)
        return 0


def main():
    """Main CLI entry point."""
    parser = argparse.ArgumentParser(
        description="Capture Codex CLI file operations for Diachron provenance tracking"
    )
    parser.add_argument(
        "--jsonl", "-j",
        type=Path,
        help="Path to Codex session JSONL file"
    )
    parser.add_argument(
        "--latest", "-l",
        action="store_true",
        help="Auto-discover and parse the most recent Codex session"
    )
    parser.add_argument(
        "--parent-session", "-p",
        help="Parent Claude session ID (for handoff correlation)"
    )
    parser.add_argument(
        "--git-branch", "-b",
        help="Git branch name"
    )
    parser.add_argument(
        "--dry-run", "-n",
        action="store_true",
        help="Parse and show operations without sending to daemon"
    )
    parser.add_argument(
        "--verbose", "-v",
        action="store_true",
        help="Show detailed output"
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Output parsed operations as JSON"
    )

    args = parser.parse_args()

    # Determine which JSONL file to parse
    jsonl_path = args.jsonl
    if args.latest:
        jsonl_path = find_latest_session()
        if not jsonl_path:
            print("Error: No Codex session files found", file=sys.stderr)
            sys.exit(1)
        if args.verbose:
            print(f"Found latest session: {jsonl_path}")

    if not jsonl_path:
        parser.print_help()
        sys.exit(1)

    if not jsonl_path.exists():
        print(f"Error: File not found: {jsonl_path}", file=sys.stderr)
        sys.exit(1)

    # Parse the session
    result = parse_codex_jsonl(jsonl_path)

    if args.json:
        print(json.dumps(result, indent=2, default=str))
        return

    # Summary
    if args.verbose or args.dry_run:
        print(f"\n📦 Codex Session: {result['session_id']}")
        print(f"   CWD: {result['cwd']}")
        print(f"   CLI Version: {result['cli_version']}")
        print(f"   Operations: {len(result['operations'])}")
        print()

        for i, op in enumerate(result['operations'], 1):
            file_display = op.get('file_path') or '(no file)'
            print(f"   [{i}] {op['operation']} → {file_display}")
            if op.get('diff_summary'):
                print(f"       {op['diff_summary']}")
        print()

    if args.dry_run:
        print("Dry run - no events sent")
        return

    # Send to daemon or local DB
    session_id = result.get("session_id") or "unknown"
    cwd = result.get("cwd")
    cli_version = result.get("cli_version")

    if result['operations']:
        # Try daemon first
        count = send_to_daemon(
            result['operations'],
            session_id=session_id,
            parent_session=args.parent_session,
            git_branch=args.git_branch,
            cli_version=cli_version,
            cwd=cwd,
        )

        # Fallback to local DB if daemon unavailable
        if count == 0:
            count = save_to_local_db(
                result['operations'],
                session_id=session_id,
                parent_session=args.parent_session,
                git_branch=args.git_branch,
                cli_version=cli_version,
                cwd=cwd,
            )

        if args.verbose:
            print(f"✅ Captured {count}/{len(result['operations'])} operations")
        else:
            print(f"Captured {count} Codex operations")
    else:
        print("No file operations found in session")


if __name__ == "__main__":
    main()
