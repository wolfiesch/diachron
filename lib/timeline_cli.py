#!/usr/bin/env python3
"""
Diachron Timeline CLI
=====================
Command-line interface for viewing the project timeline.

Usage:
    python3 timeline_cli.py                          # Last 20 events
    python3 timeline_cli.py --since "1 hour ago"     # Time filter
    python3 timeline_cli.py --file src/              # File filter
    python3 timeline_cli.py --stats                  # Show statistics
    python3 timeline_cli.py --export markdown        # Export to file
    python3 timeline_cli.py --summarize              # Generate AI summaries
"""

import argparse
import sys
import json
from pathlib import Path
from datetime import datetime

# Add lib directory to path
sys.path.insert(0, str(Path(__file__).parent))

from db import DiachronDB


def format_timestamp_str(ts: str) -> str:
    """Format a timestamp string for display.

    Args:
        ts: ISO 8601 timestamp or a preformatted display string.

    Returns:
        A display-friendly timestamp string.
    """
    if not ts:
        return "Unknown"
    try:
        if "T" in ts:
            dt = datetime.fromisoformat(ts.replace("Z", "+00:00"))
            return dt.strftime("%m/%d/%Y %I:%M %p")
        return ts
    except:
        return ts


def get_display_timestamp(event: dict) -> str:
    """Resolve the display timestamp for an event.

    Args:
        event: Event dictionary containing timestamp fields.

    Returns:
        A human-readable timestamp, preferring `timestamp_display` when present.
    """
    # Prefer the display timestamp if available
    display_ts = event.get("timestamp_display")
    if display_ts:
        return display_ts

    # Fall back to formatting the ISO timestamp
    return format_timestamp_str(event.get("timestamp", ""))


def parse_metadata(metadata_str: str) -> dict:
    """Parse metadata JSON safely.

    Args:
        metadata_str: JSON-encoded metadata string.

    Returns:
        Parsed metadata dictionary or an empty dict on failure.
    """
    if not metadata_str:
        return {}
    try:
        return json.loads(metadata_str)
    except (json.JSONDecodeError, TypeError):
        return {}


def print_timeline(events: list, verbose: bool = False):
    """Print events in timeline format with metadata.

    Args:
        events: List of event dictionaries to display.
        verbose: Whether to print raw input lines for each event.
    """
    project_name = Path.cwd().name

    print(f"\n📍 Timeline for {project_name}")
    print("━" * 55)
    print()

    if not events:
        print("  No events found.")
        print()
        return

    for event in events:
        ts = get_display_timestamp(event)
        tool = event.get("tool_name", "Unknown")
        file_path = event.get("file_path")
        operation = event.get("operation", "unknown")
        diff = event.get("diff_summary", "")
        ai_summary = event.get("ai_summary", "")
        git_sha = event.get("git_commit_sha", "")
        metadata = parse_metadata(event.get("metadata", ""))

        # Extract metadata fields
        git_branch = metadata.get("git_branch")
        command_category = metadata.get("command_category")

        # Tool emoji with category badge for Bash
        tool_emoji = {"Write": "📝", "Edit": "✏️", "Bash": "🖥️"}.get(tool, "🔧")
        tool_display = f"{tool_emoji} {tool}"
        if tool == "Bash" and command_category and command_category != "unknown":
            tool_display += f" [{command_category}]"

        print(f"🕐 {ts}")
        print(f"   ├─ Tool: {tool_display}")

        if file_path:
            print(f"   ├─ File: {file_path}")

        if git_branch:
            print(f"   ├─ Branch: 🌿 {git_branch}")

        if operation:
            op_display = operation
            if git_sha:
                op_display += f" → {git_sha}"
            print(f"   ├─ Operation: {op_display}")

        # Determine final line prefix (└─)
        if ai_summary:
            print(f"   └─ Summary: {ai_summary}")
        elif diff:
            print(f"   └─ Change: {diff}")
        else:
            print(f"   └─ (no details)")

        if verbose and event.get("raw_input"):
            print(f"\n   Raw input:")
            for line in event["raw_input"].split("\n")[:10]:
                print(f"   │ {line}")

        print()

    print("━" * 55)
    session = events[0].get("session_id", "unknown")[:8] if events else "none"
    print(f"Showing {len(events)} events • Session: {session}")
    print()


def print_stats(stats: dict):
    """Print database statistics.

    Args:
        stats: Dictionary of stats from the database.
    """
    print("\n📊 Diachron Statistics")
    print("━" * 55)
    print()

    print(f"  Total Events:     {stats.get('total_events', 0)}")
    print(f"  Total Sessions:   {stats.get('total_sessions', 0)}")
    print(f"  Unique Files:     {stats.get('unique_files', 0)}")
    print(f"  First Event:      {format_timestamp_str(stats.get('first_event', 'N/A'))}")
    print(f"  Last Event:       {format_timestamp_str(stats.get('last_event', 'N/A'))}")
    print()

    by_tool = stats.get("by_tool", {})
    if by_tool:
        total = sum(by_tool.values())
        print("  By Tool:")
        for tool, count in sorted(by_tool.items(), key=lambda x: -x[1]):
            pct = (count / total * 100) if total > 0 else 0
            print(f"    • {tool}:   {count} events ({pct:.0f}%)")

    print()
    print("━" * 55)
    print()


def export_markdown(events: list, output_path: str = "TIMELINE.md"):
    """Export events to a markdown file.

    Args:
        events: List of event dictionaries to export.
        output_path: Output file path for the markdown document.
    """
    project_name = Path.cwd().name

    lines = [
        f"# {project_name} Timeline",
        "",
        f"Generated: {datetime.now().strftime('%m/%d/%Y %I:%M %p')}",
        "",
        "## Recent Changes",
        "",
    ]

    current_date = None
    for event in events:
        full_ts = get_display_timestamp(event)
        parts = full_ts.split()
        date_str = parts[0] if parts else "Unknown"

        if date_str != current_date:
            current_date = date_str
            lines.append(f"### {current_date}")
            lines.append("")

        # Extract time portion (e.g., "03:54 AM PST" from "01/08/2026 03:54 AM PST")
        time_display = " ".join(parts[1:]) if len(parts) > 1 else full_ts

        tool = event.get("tool_name", "Unknown")
        file_path = event.get("file_path")
        operation = event.get("operation", "")
        diff = event.get("diff_summary", "")

        if file_path:
            lines.append(f"#### {time_display} - {operation.title()} `{file_path}`")
        else:
            lines.append(f"#### {time_display} - {operation.title()}")

        lines.append(f"- **Tool:** {tool}")
        if diff:
            lines.append(f"- **Change:** {diff}")
        lines.append("")

    with open(output_path, "w") as f:
        f.write("\n".join(lines))

    print(f"Exported {len(events)} events to {output_path}")


def run_summarization(limit: int = 50, verbose: bool = True):
    """Run AI summarization on unsummarized events.

    Args:
        limit: Maximum number of events to summarize.
        verbose: Whether to print progress output.

    Returns:
        Number of events successfully summarized.

    Raises:
        SystemExit: If the summarizer module cannot be loaded.
    """
    try:
        from summarize import DiachronSummarizer
    except ImportError as e:
        print(f"Error loading summarizer: {e}", file=sys.stderr)
        print("Note: The Python summarizer is deprecated. Use 'diachron memory summarize' instead.", file=sys.stderr)
        print("If using legacy mode, install: pip install openai", file=sys.stderr)
        sys.exit(1)

    summarizer = DiachronSummarizer()
    count = summarizer.summarize_pending(limit=limit, verbose=verbose)
    return count


def main():
    """Run the timeline CLI.

    Parses CLI arguments, queries the database, and prints or exports results.

    Raises:
        SystemExit: For initialization errors or fatal CLI failures.
    """
    parser = argparse.ArgumentParser(description="View Diachron timeline")
    parser.add_argument("--since", "-s", help="Show events since (e.g., '1 hour ago')")
    parser.add_argument("--until", "-u", help="Show events until")
    parser.add_argument("--file", "-f", help="Filter by file path")
    parser.add_argument("--tool", "-t", help="Filter by tool (Write, Edit, Bash)")
    parser.add_argument("--limit", "-n", type=int, default=20, help="Max events to show")
    parser.add_argument("--verbose", "-v", action="store_true", help="Show full details")
    parser.add_argument("--stats", action="store_true", help="Show statistics")
    parser.add_argument("--export", choices=["markdown", "json"], help="Export format")
    parser.add_argument("--json", action="store_true", help="Output as JSON")
    parser.add_argument("--summarize", action="store_true",
                       help="Generate AI summaries for unsummarized events")

    args = parser.parse_args()

    # Check if initialized
    if not Path(".diachron/events.db").exists():
        print("Diachron is not initialized for this project.", file=sys.stderr)
        print("Run /diachron init to set it up.", file=sys.stderr)
        sys.exit(1)

    # Handle summarization separately (doesn't need db connection)
    if args.summarize:
        run_summarization(limit=args.limit, verbose=True)
        return

    db = DiachronDB()

    try:
        if args.stats:
            stats = db.get_stats()
            if args.json:
                print(json.dumps(stats, indent=2))
            else:
                print_stats(stats)
        else:
            events = db.query_events(
                since=args.since,
                until=args.until,
                file_path=args.file,
                tool_name=args.tool,
                limit=args.limit
            )

            if args.export == "json" or args.json:
                print(json.dumps(events, indent=2))
            elif args.export == "markdown":
                export_markdown(events)
            else:
                print_timeline(events, verbose=args.verbose)

    finally:
        db.close()


if __name__ == "__main__":
    main()
