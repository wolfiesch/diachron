#!/usr/bin/env python3
"""
Diachron AI Summarization Module
================================
On-demand AI summaries for timeline events using OpenAI gpt-5-mini.

Cost: ~$0.00003 per event (~$0.03 per 1000 events)

Usage:
    from summarize import DiachronSummarizer

    summarizer = DiachronSummarizer()

    # Summarize a single event
    summary = summarizer.summarize_event(event_dict)

    # Batch summarize unsummarized events
    summarizer.summarize_pending(limit=50)
"""

import os
import json
import sqlite3
from pathlib import Path
from typing import Optional, Dict, Any, List

# Lazy import - only load OpenAI when actually summarizing
openai_client = None

def get_openai_client():
    """Lazy-load OpenAI client to avoid import overhead on hook path."""
    global openai_client
    if openai_client is None:
        try:
            from openai import OpenAI
            openai_client = OpenAI()  # Uses OPENAI_API_KEY env var
        except ImportError:
            raise RuntimeError(
                "OpenAI package not installed. Run: pip install openai"
            )
        except Exception as e:
            raise RuntimeError(f"Failed to initialize OpenAI client: {e}")
    return openai_client


# Model configuration
# Using gpt-4o-mini for simple summarization (gpt-5-mini is a reasoning model
# that's overkill for this task and uses internal thinking tokens inefficiently)
MODEL = "gpt-4o-mini"
MAX_TOKENS = 50  # Short summaries
TEMPERATURE = 0.3  # Low temperature for consistent, factual summaries


def get_project_root() -> Path:
    """Find project root by looking for .diachron directory."""
    current = Path.cwd()
    while current != current.parent:
        if (current / ".diachron").exists():
            return current
        current = current.parent
    return Path.cwd()


class DiachronSummarizer:
    """AI-powered summarization for timeline events."""

    def __init__(self, project_root: Optional[Path] = None):
        self.project_root = project_root or get_project_root()
        self.db_path = self.project_root / ".diachron" / "events.db"

    def _get_connection(self) -> sqlite3.Connection:
        """Get database connection."""
        if not self.db_path.exists():
            raise FileNotFoundError(f"Database not found: {self.db_path}")
        conn = sqlite3.connect(str(self.db_path))
        conn.row_factory = sqlite3.Row
        return conn

    def build_prompt(self, event: Dict[str, Any]) -> str:
        """Build a prompt for summarizing an event."""
        tool = event.get("tool_name", "Unknown")
        file_path = event.get("file_path") or "(no file)"
        operation = event.get("operation", "unknown")
        diff_summary = event.get("diff_summary", "")
        raw_input = event.get("raw_input", "")
        metadata = event.get("metadata")

        # Parse metadata if present
        meta_context = ""
        if metadata:
            try:
                meta = json.loads(metadata)
                branch = meta.get("git_branch")
                category = meta.get("command_category")
                if branch:
                    meta_context += f"\nBranch: {branch}"
                if category:
                    meta_context += f"\nCategory: {category}"
            except json.JSONDecodeError:
                pass

        # Truncate raw input for prompt efficiency
        if raw_input and len(raw_input) > 500:
            raw_input = raw_input[:500] + "..."

        prompt = f"""Summarize this code change in 10 words or less. Be specific about what changed.

Tool: {tool}
File: {file_path}
Operation: {operation}
Change details: {diff_summary}{meta_context}
"""

        if raw_input and tool == "Bash":
            prompt += f"\nCommand: {raw_input}"
        elif raw_input and tool in ("Write", "Edit"):
            prompt += f"\nCode snippet: {raw_input[:200]}"

        return prompt

    def summarize_event(self, event: Dict[str, Any]) -> Optional[str]:
        """
        Generate an AI summary for a single event.

        Returns the summary string or None if summarization fails.
        """
        try:
            client = get_openai_client()
            prompt = self.build_prompt(event)

            response = client.chat.completions.create(
                model=MODEL,
                messages=[
                    {
                        "role": "system",
                        "content": "You are a code change summarizer. Generate very brief (10 words max) summaries of code changes. Focus on what was done, not technical details."
                    },
                    {
                        "role": "user",
                        "content": prompt
                    }
                ],
                max_tokens=MAX_TOKENS,
                temperature=TEMPERATURE
            )

            summary = response.choices[0].message.content.strip()

            # Clean up summary - remove quotes, periods at end
            summary = summary.strip('"\'').rstrip('.')

            return summary

        except Exception as e:
            print(f"Summarization failed for event {event.get('id')}: {e}")
            return None

    def get_unsummarized_events(self, limit: int = 50) -> List[Dict[str, Any]]:
        """Get events that don't have AI summaries yet."""
        conn = self._get_connection()
        cursor = conn.cursor()

        cursor.execute("""
            SELECT * FROM events
            WHERE ai_summary IS NULL
            ORDER BY timestamp DESC
            LIMIT ?
        """, (limit,))

        events = [dict(row) for row in cursor.fetchall()]
        conn.close()
        return events

    def update_event_summary(self, event_id: int, summary: str) -> bool:
        """Update an event's ai_summary field."""
        try:
            conn = self._get_connection()
            cursor = conn.cursor()

            cursor.execute(
                "UPDATE events SET ai_summary = ? WHERE id = ?",
                (summary, event_id)
            )

            conn.commit()
            conn.close()
            return True

        except Exception as e:
            print(f"Failed to update event {event_id}: {e}")
            return False

    def summarize_pending(self, limit: int = 50, verbose: bool = False) -> int:
        """
        Summarize all pending (unsummarized) events.

        Returns the number of events successfully summarized.
        """
        events = self.get_unsummarized_events(limit)

        if not events:
            if verbose:
                print("No unsummarized events found.")
            return 0

        if verbose:
            print(f"Summarizing {len(events)} events...")

        success_count = 0

        for i, event in enumerate(events):
            event_id = event.get("id")

            if verbose:
                file_info = event.get("file_path") or event.get("operation") or "event"
                print(f"  [{i+1}/{len(events)}] {file_info}...", end=" ", flush=True)

            summary = self.summarize_event(event)

            if summary:
                if self.update_event_summary(event_id, summary):
                    success_count += 1
                    if verbose:
                        print(f"✓ {summary}")
                else:
                    if verbose:
                        print("✗ (db error)")
            else:
                if verbose:
                    print("✗ (api error)")

        return success_count

    def summarize_event_by_id(self, event_id: int) -> Optional[str]:
        """Summarize a specific event by ID."""
        conn = self._get_connection()
        cursor = conn.cursor()

        cursor.execute("SELECT * FROM events WHERE id = ?", (event_id,))
        row = cursor.fetchone()
        conn.close()

        if not row:
            return None

        event = dict(row)
        summary = self.summarize_event(event)

        if summary:
            self.update_event_summary(event_id, summary)

        return summary


# CLI interface
if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(description="AI summarization for Diachron events")
    parser.add_argument("--pending", "-p", action="store_true",
                       help="Summarize all pending events")
    parser.add_argument("--id", "-i", type=int,
                       help="Summarize a specific event by ID")
    parser.add_argument("--limit", "-n", type=int, default=50,
                       help="Max events to summarize (default: 50)")
    parser.add_argument("--verbose", "-v", action="store_true",
                       help="Show progress")

    args = parser.parse_args()

    summarizer = DiachronSummarizer()

    if args.id:
        summary = summarizer.summarize_event_by_id(args.id)
        if summary:
            print(f"Summary: {summary}")
        else:
            print("Failed to generate summary")

    elif args.pending:
        count = summarizer.summarize_pending(limit=args.limit, verbose=args.verbose)
        print(f"\nSummarized {count} events")

    else:
        # Default: show pending count
        events = summarizer.get_unsummarized_events(limit=1000)
        print(f"Unsummarized events: {len(events)}")
        print("Run with --pending to summarize them")
