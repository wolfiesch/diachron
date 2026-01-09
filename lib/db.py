#!/usr/bin/env python3
"""
Diachron Database Layer
=======================
SQLite operations for the Diachron provenance system.

Usage:
    from db import DiachronDB
    db = DiachronDB()  # Auto-detects project root
    db.insert_event(tool_name="Write", file_path="src/app.ts", ...)
    events = db.query_events(since="1 hour ago", file_path="src/")
"""

import sqlite3
import os
import json
import hashlib
from datetime import datetime, timedelta
from pathlib import Path
from typing import Optional, List, Dict, Any
import subprocess
import re


def get_project_root() -> Path:
    """Find project root by looking for .git or .diachron directory."""
    current = Path.cwd()
    while current != current.parent:
        if (current / ".git").exists() or (current / ".diachron").exists():
            return current
        current = current.parent
    # Default to cwd if no markers found
    return Path.cwd()


def get_timestamp() -> tuple[str, str]:
    """
    Get current timestamp in both ISO (for sorting) and display (for humans) formats.

    Returns:
        tuple of (iso_timestamp, display_timestamp)
        - iso_timestamp: ISO 8601 format for database sorting/filtering
        - display_timestamp: Human-readable format from pst-timestamp or fallback
    """
    iso_ts = datetime.now().isoformat()
    display_ts = iso_ts  # Fallback

    try:
        result = subprocess.run(
            ["pst-timestamp"],
            capture_output=True,
            text=True,
            timeout=5
        )
        if result.returncode == 0:
            display_ts = result.stdout.strip()
    except (subprocess.TimeoutExpired, FileNotFoundError):
        # Use a formatted fallback
        display_ts = datetime.now().strftime("%m/%d/%Y %I:%M %p")

    return iso_ts, display_ts


def generate_session_id() -> str:
    """Generate a unique session ID based on timestamp and random bytes."""
    timestamp = datetime.now().isoformat()
    random_bytes = os.urandom(8).hex()
    return hashlib.sha256(f"{timestamp}-{random_bytes}".encode()).hexdigest()[:12]


def get_or_create_session_id(diachron_dir: Path) -> str:
    """
    Get existing session ID or create a new one.
    Session IDs persist for 1 hour to group related events.
    """
    import time

    session_file = diachron_dir / ".session_id"
    SESSION_EXPIRY_SECONDS = 3600  # 1 hour

    try:
        if session_file.exists():
            mtime = session_file.stat().st_mtime
            age = time.time() - mtime
            if age < SESSION_EXPIRY_SECONDS:
                session_id = session_file.read_text().strip()
                if session_id:  # Verify it's not empty
                    # Touch the file to extend the session
                    session_file.touch()
                    return session_id

        # Generate new session ID
        new_id = generate_session_id()
        session_file.parent.mkdir(parents=True, exist_ok=True)
        session_file.write_text(new_id)
        return new_id

    except (OSError, PermissionError):
        # If we can't persist, just generate a new one
        return generate_session_id()


class DiachronDB:
    """SQLite database interface for Diachron events."""

    SCHEMA_VERSION = 1

    def __init__(self, project_root: Optional[Path] = None):
        self.project_root = project_root or get_project_root()
        self.diachron_dir = self.project_root / ".diachron"
        self.db_path = self.diachron_dir / "events.db"
        self.config_path = self.diachron_dir / "config.json"
        self._session_id: Optional[str] = None
        self._conn: Optional[sqlite3.Connection] = None

    @property
    def session_id(self) -> str:
        """Get or generate session ID for current session."""
        if self._session_id is None:
            self._session_id = get_or_create_session_id(self.diachron_dir)
        return self._session_id

    def _ensure_dir(self) -> None:
        """Ensure .diachron directory exists."""
        self.diachron_dir.mkdir(parents=True, exist_ok=True)

    def _get_connection(self) -> sqlite3.Connection:
        """Get or create database connection."""
        if self._conn is None:
            self._ensure_dir()
            self._conn = sqlite3.connect(str(self.db_path))
            self._conn.row_factory = sqlite3.Row
            self._init_schema()
        return self._conn

    def _init_schema(self) -> None:
        """Initialize database schema if needed."""
        conn = self._conn
        cursor = conn.cursor()

        # Check if tables exist
        cursor.execute("""
            SELECT name FROM sqlite_master
            WHERE type='table' AND name='events'
        """)

        if cursor.fetchone() is None:
            cursor.executescript("""
                CREATE TABLE events (
                    id INTEGER PRIMARY KEY,
                    timestamp TEXT NOT NULL,
                    timestamp_display TEXT,
                    session_id TEXT,
                    tool_name TEXT NOT NULL,
                    file_path TEXT,
                    operation TEXT,
                    diff_summary TEXT,
                    raw_input TEXT,
                    ai_summary TEXT,
                    git_commit_sha TEXT,
                    parent_event_id INTEGER,
                    metadata TEXT,
                    FOREIGN KEY (parent_event_id) REFERENCES events(id)
                );

                CREATE INDEX idx_events_timestamp ON events(timestamp);
                CREATE INDEX idx_events_file_path ON events(file_path);
                CREATE INDEX idx_events_session_id ON events(session_id);
                CREATE INDEX idx_events_tool_name ON events(tool_name);

                CREATE TABLE schema_version (
                    version INTEGER PRIMARY KEY
                );

                INSERT INTO schema_version VALUES (2);
            """)
            conn.commit()
        else:
            # Check if we need to migrate from v1 to v2
            cursor.execute("PRAGMA table_info(events)")
            columns = [row[1] for row in cursor.fetchall()]
            if "timestamp_display" not in columns:
                cursor.execute("ALTER TABLE events ADD COLUMN timestamp_display TEXT")
                conn.commit()

    def insert_event(
        self,
        tool_name: str,
        file_path: Optional[str] = None,
        operation: Optional[str] = None,
        diff_summary: Optional[str] = None,
        raw_input: Optional[str] = None,
        ai_summary: Optional[str] = None,
        git_commit_sha: Optional[str] = None,
        parent_event_id: Optional[int] = None,
        metadata: Optional[Dict[str, Any]] = None
    ) -> int:
        """
        Insert a new event into the database.

        Returns the ID of the inserted event.
        """
        conn = self._get_connection()
        cursor = conn.cursor()

        timestamp_iso, timestamp_display = get_timestamp()
        metadata_json = json.dumps(metadata) if metadata else None

        # Truncate raw_input if too long
        if raw_input and len(raw_input) > 10000:
            raw_input = raw_input[:10000] + "\n... [truncated]"

        cursor.execute("""
            INSERT INTO events (
                timestamp, timestamp_display, session_id, tool_name, file_path,
                operation, diff_summary, raw_input, ai_summary, git_commit_sha,
                parent_event_id, metadata
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        """, (
            timestamp_iso, timestamp_display, self.session_id, tool_name, file_path,
            operation, diff_summary, raw_input, ai_summary, git_commit_sha,
            parent_event_id, metadata_json
        ))

        conn.commit()
        return cursor.lastrowid

    def query_events(
        self,
        since: Optional[str] = None,
        until: Optional[str] = None,
        file_path: Optional[str] = None,
        tool_name: Optional[str] = None,
        session_id: Optional[str] = None,
        limit: int = 50,
        offset: int = 0
    ) -> List[Dict[str, Any]]:
        """
        Query events with various filters.

        Args:
            since: Human-readable time like "1 hour ago", "yesterday"
            until: Human-readable time for upper bound
            file_path: Filter by file path (supports prefix matching)
            tool_name: Filter by tool name
            session_id: Filter by session
            limit: Max results to return
            offset: Pagination offset

        Returns list of event dictionaries.
        """
        conn = self._get_connection()
        cursor = conn.cursor()

        conditions = []
        params = []

        if since:
            since_dt = self._parse_relative_time(since)
            if since_dt:
                conditions.append("timestamp >= ?")
                params.append(since_dt.isoformat())

        if until:
            until_dt = self._parse_relative_time(until)
            if until_dt:
                conditions.append("timestamp <= ?")
                params.append(until_dt.isoformat())

        if file_path:
            conditions.append("file_path LIKE ?")
            params.append(f"{file_path}%")

        if tool_name:
            conditions.append("tool_name = ?")
            params.append(tool_name)

        if session_id:
            conditions.append("session_id = ?")
            params.append(session_id)

        where_clause = " AND ".join(conditions) if conditions else "1=1"

        query = f"""
            SELECT * FROM events
            WHERE {where_clause}
            ORDER BY timestamp DESC
            LIMIT ? OFFSET ?
        """
        params.extend([limit, offset])

        cursor.execute(query, params)
        rows = cursor.fetchall()

        return [dict(row) for row in rows]

    def _parse_relative_time(self, time_str: str) -> Optional[datetime]:
        """
        Parse relative time strings like "1 hour ago", "yesterday", "2 days ago".
        """
        now = datetime.now()
        time_str = time_str.lower().strip()

        if time_str == "now":
            return now

        if time_str == "yesterday":
            return now - timedelta(days=1)

        if time_str == "today":
            return now.replace(hour=0, minute=0, second=0, microsecond=0)

        # Parse patterns like "1 hour ago", "30 minutes ago", "2 days ago"
        patterns = [
            (r"(\d+)\s*hours?\s*ago", lambda m: now - timedelta(hours=int(m.group(1)))),
            (r"(\d+)\s*minutes?\s*ago", lambda m: now - timedelta(minutes=int(m.group(1)))),
            (r"(\d+)\s*days?\s*ago", lambda m: now - timedelta(days=int(m.group(1)))),
            (r"(\d+)\s*weeks?\s*ago", lambda m: now - timedelta(weeks=int(m.group(1)))),
        ]

        for pattern, handler in patterns:
            match = re.match(pattern, time_str)
            if match:
                return handler(match)

        # Try parsing as ISO format
        try:
            return datetime.fromisoformat(time_str)
        except ValueError:
            return None

    def get_stats(self) -> Dict[str, Any]:
        """Get statistics about the events database."""
        conn = self._get_connection()
        cursor = conn.cursor()

        cursor.execute("SELECT COUNT(*) as total FROM events")
        total = cursor.fetchone()["total"]

        cursor.execute("""
            SELECT tool_name, COUNT(*) as count
            FROM events
            GROUP BY tool_name
            ORDER BY count DESC
        """)
        by_tool = {row["tool_name"]: row["count"] for row in cursor.fetchall()}

        cursor.execute("SELECT COUNT(DISTINCT session_id) as sessions FROM events")
        sessions = cursor.fetchone()["sessions"]

        cursor.execute("SELECT COUNT(DISTINCT file_path) as files FROM events WHERE file_path IS NOT NULL")
        files = cursor.fetchone()["files"]

        cursor.execute("SELECT MIN(timestamp) as first, MAX(timestamp) as last FROM events")
        time_range = cursor.fetchone()

        return {
            "total_events": total,
            "by_tool": by_tool,
            "total_sessions": sessions,
            "unique_files": files,
            "first_event": time_range["first"],
            "last_event": time_range["last"]
        }

    def close(self) -> None:
        """Close database connection."""
        if self._conn:
            self._conn.close()
            self._conn = None


# CLI interface for testing
if __name__ == "__main__":
    import sys

    db = DiachronDB()

    if len(sys.argv) < 2:
        print("Usage: db.py [stats|query|insert]")
        sys.exit(1)

    cmd = sys.argv[1]

    if cmd == "stats":
        stats = db.get_stats()
        print(json.dumps(stats, indent=2))

    elif cmd == "query":
        # Simple query - last 10 events
        events = db.query_events(limit=10)
        for e in events:
            print(f"[{e['timestamp']}] {e['tool_name']}: {e['file_path'] or '(no file)'}")

    elif cmd == "insert":
        # Test insert
        event_id = db.insert_event(
            tool_name="Test",
            file_path="test/example.py",
            operation="create",
            diff_summary="+10 lines"
        )
        print(f"Inserted event ID: {event_id}")

    db.close()
