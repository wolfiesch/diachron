#!/usr/bin/env python3
"""
Diachron Event Capture CLI
==========================
Quick command-line interface for capturing events from hooks.

Usage:
    python3 capture_event.py --tool Write --file src/app.ts --op create --diff "+45 lines"
    python3 capture_event.py --tool Edit --file package.json --op modify --diff "version: 1.0→1.1"
    python3 capture_event.py --tool Bash --op commit --diff "feat: add auth"
"""

import argparse
import sys
from pathlib import Path

# Add lib directory to path
sys.path.insert(0, str(Path(__file__).parent))

from db import DiachronDB


def main():
    """Capture a Diachron event from CLI arguments.

    Parses CLI flags, verifies the project is initialized, and inserts the
    event into the local Diachron database.

    Raises:
        SystemExit: Exits with a status code for normal termination or errors.
    """
    parser = argparse.ArgumentParser(description="Capture a Diachron event")
    parser.add_argument("--tool", "-t", required=True, help="Tool name (Write, Edit, Bash)")
    parser.add_argument("--file", "-f", default=None, help="File path affected")
    parser.add_argument("--op", "-o", default="modify", help="Operation type (create, modify, delete, commit, etc.)")
    parser.add_argument("--diff", "-d", default=None, help="Diff summary")
    parser.add_argument("--input", "-i", default=None, help="Raw input (truncated)")
    parser.add_argument("--quiet", "-q", action="store_true", help="Suppress output")

    args = parser.parse_args()

    try:
        # Check if .diachron exists
        if not Path(".diachron").exists():
            if not args.quiet:
                print("Diachron not initialized in this project. Run /diachron init first.", file=sys.stderr)
            sys.exit(0)  # Silent exit, not an error

        db = DiachronDB()
        event_id = db.insert_event(
            tool_name=args.tool,
            file_path=args.file,
            operation=args.op,
            diff_summary=args.diff,
            raw_input=args.input
        )
        db.close()

        if not args.quiet:
            print(f"Event {event_id} captured")

    except Exception as e:
        if not args.quiet:
            print(f"Error capturing event: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
