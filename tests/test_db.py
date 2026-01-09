#!/usr/bin/env python3
"""
Unit tests for Diachron database layer (db.py)
==============================================

Run with: pytest tests/test_db.py -v
Or: python3 -m pytest tests/test_db.py -v
"""

import pytest
import tempfile
import os
import shutil
from pathlib import Path
from datetime import datetime, timedelta
import sys

# Add lib directory to path
sys.path.insert(0, str(Path(__file__).parent.parent / "lib"))

from db import (
    DiachronDB,
    get_project_root,
    get_timestamp,
    generate_session_id,
    get_or_create_session_id,
)


class TestGetTimestamp:
    """Tests for the get_timestamp function."""

    def test_returns_tuple(self):
        """Should return a tuple of two strings."""
        result = get_timestamp()
        assert isinstance(result, tuple)
        assert len(result) == 2
        assert isinstance(result[0], str)
        assert isinstance(result[1], str)

    def test_iso_format(self):
        """ISO timestamp should be parseable."""
        iso_ts, _ = get_timestamp()
        # Should not raise
        dt = datetime.fromisoformat(iso_ts)
        assert dt is not None

    def test_display_format_not_empty(self):
        """Display timestamp should not be empty."""
        _, display_ts = get_timestamp()
        assert len(display_ts) > 0


class TestGenerateSessionId:
    """Tests for session ID generation."""

    def test_generates_string(self):
        """Should generate a string ID."""
        session_id = generate_session_id()
        assert isinstance(session_id, str)

    def test_consistent_length(self):
        """Should generate 12-character IDs."""
        session_id = generate_session_id()
        assert len(session_id) == 12

    def test_unique_ids(self):
        """Should generate unique IDs each time."""
        ids = [generate_session_id() for _ in range(100)]
        assert len(set(ids)) == 100


class TestGetOrCreateSessionId:
    """Tests for session ID persistence."""

    def test_creates_new_session(self):
        """Should create a new session file when none exists."""
        with tempfile.TemporaryDirectory() as tmpdir:
            diachron_dir = Path(tmpdir) / ".diachron"
            diachron_dir.mkdir()

            session_id = get_or_create_session_id(diachron_dir)

            assert len(session_id) == 12
            assert (diachron_dir / ".session_id").exists()

    def test_reuses_existing_session(self):
        """Should reuse existing session within expiry window."""
        with tempfile.TemporaryDirectory() as tmpdir:
            diachron_dir = Path(tmpdir) / ".diachron"
            diachron_dir.mkdir()

            # Create first session
            id1 = get_or_create_session_id(diachron_dir)
            # Get again immediately
            id2 = get_or_create_session_id(diachron_dir)

            assert id1 == id2


class TestDiachronDB:
    """Tests for the DiachronDB class."""

    @pytest.fixture
    def temp_project(self):
        """Create a temporary project directory with .diachron."""
        tmpdir = tempfile.mkdtemp()
        diachron_dir = Path(tmpdir) / ".diachron"
        diachron_dir.mkdir()
        yield Path(tmpdir)
        shutil.rmtree(tmpdir)

    @pytest.fixture
    def db(self, temp_project):
        """Create a DiachronDB instance for testing."""
        db = DiachronDB(project_root=temp_project)
        yield db
        db.close()

    def test_init_creates_db(self, db, temp_project):
        """Should create database file on first use."""
        # Trigger connection
        db._get_connection()
        assert (temp_project / ".diachron" / "events.db").exists()

    def test_insert_event_basic(self, db):
        """Should insert a basic event and return ID."""
        event_id = db.insert_event(
            tool_name="Write",
            file_path="src/test.py",
            operation="create",
            diff_summary="+10 lines"
        )

        assert isinstance(event_id, int)
        assert event_id > 0

    def test_insert_event_with_metadata(self, db):
        """Should insert event with JSON metadata."""
        event_id = db.insert_event(
            tool_name="Edit",
            file_path="src/app.py",
            operation="modify",
            metadata={"git_branch": "main", "command_category": "build"}
        )

        events = db.query_events(limit=1)
        assert len(events) == 1
        assert events[0]["metadata"] is not None

    def test_insert_event_truncates_long_input(self, db):
        """Should truncate raw_input longer than 10000 chars."""
        long_input = "x" * 20000
        event_id = db.insert_event(
            tool_name="Write",
            file_path="test.txt",
            raw_input=long_input
        )

        events = db.query_events(limit=1)
        assert len(events[0]["raw_input"]) < 20000
        assert "[truncated]" in events[0]["raw_input"]

    def test_query_events_returns_list(self, db):
        """Should return a list of event dictionaries."""
        db.insert_event(tool_name="Test", file_path="test.py")
        events = db.query_events()

        assert isinstance(events, list)
        assert len(events) >= 1
        assert isinstance(events[0], dict)

    def test_query_events_limit(self, db):
        """Should respect limit parameter."""
        # Insert 10 events
        for i in range(10):
            db.insert_event(tool_name="Test", file_path=f"test{i}.py")

        events = db.query_events(limit=5)
        assert len(events) == 5

    def test_query_events_by_tool(self, db):
        """Should filter by tool name."""
        db.insert_event(tool_name="Write", file_path="write.py")
        db.insert_event(tool_name="Edit", file_path="edit.py")
        db.insert_event(tool_name="Write", file_path="write2.py")

        events = db.query_events(tool_name="Write")
        assert len(events) == 2
        assert all(e["tool_name"] == "Write" for e in events)

    def test_query_events_by_file_path(self, db):
        """Should filter by file path prefix."""
        db.insert_event(tool_name="Write", file_path="src/app.py")
        db.insert_event(tool_name="Write", file_path="src/utils.py")
        db.insert_event(tool_name="Write", file_path="tests/test.py")

        events = db.query_events(file_path="src/")
        assert len(events) == 2
        assert all(e["file_path"].startswith("src/") for e in events)

    def test_query_events_ordered_desc(self, db):
        """Should return events in descending timestamp order."""
        db.insert_event(tool_name="Test", file_path="first.py")
        db.insert_event(tool_name="Test", file_path="second.py")
        db.insert_event(tool_name="Test", file_path="third.py")

        events = db.query_events()
        # Most recent should be first
        assert events[0]["file_path"] == "third.py"

    def test_get_stats(self, db):
        """Should return statistics dictionary."""
        db.insert_event(tool_name="Write", file_path="test1.py")
        db.insert_event(tool_name="Edit", file_path="test2.py")
        db.insert_event(tool_name="Write", file_path="test3.py")

        stats = db.get_stats()

        assert stats["total_events"] == 3
        assert stats["by_tool"]["Write"] == 2
        assert stats["by_tool"]["Edit"] == 1
        assert stats["unique_files"] == 3
        assert stats["first_event"] is not None
        assert stats["last_event"] is not None

    def test_session_id_consistency(self, db):
        """Should use consistent session ID within instance."""
        db.insert_event(tool_name="Test", file_path="test1.py")
        db.insert_event(tool_name="Test", file_path="test2.py")

        events = db.query_events()
        session_ids = set(e["session_id"] for e in events)

        assert len(session_ids) == 1


class TestParseRelativeTime:
    """Tests for relative time parsing."""

    @pytest.fixture
    def db(self):
        """Create a DiachronDB instance for accessing _parse_relative_time."""
        with tempfile.TemporaryDirectory() as tmpdir:
            diachron_dir = Path(tmpdir) / ".diachron"
            diachron_dir.mkdir()
            db = DiachronDB(project_root=Path(tmpdir))
            yield db
            db.close()

    def test_parse_now(self, db):
        """Should parse 'now' as current time."""
        result = db._parse_relative_time("now")
        assert result is not None
        # Should be within last second
        assert (datetime.now() - result).total_seconds() < 1

    def test_parse_yesterday(self, db):
        """Should parse 'yesterday' as 24 hours ago."""
        result = db._parse_relative_time("yesterday")
        assert result is not None
        expected = datetime.now() - timedelta(days=1)
        # Should be close (within a few seconds)
        assert abs((result - expected).total_seconds()) < 5

    def test_parse_today(self, db):
        """Should parse 'today' as start of current day."""
        result = db._parse_relative_time("today")
        assert result is not None
        assert result.hour == 0
        assert result.minute == 0
        assert result.second == 0

    def test_parse_hours_ago(self, db):
        """Should parse 'N hours ago' format."""
        result = db._parse_relative_time("2 hours ago")
        assert result is not None
        expected = datetime.now() - timedelta(hours=2)
        assert abs((result - expected).total_seconds()) < 5

    def test_parse_minutes_ago(self, db):
        """Should parse 'N minutes ago' format."""
        result = db._parse_relative_time("30 minutes ago")
        assert result is not None
        expected = datetime.now() - timedelta(minutes=30)
        assert abs((result - expected).total_seconds()) < 5

    def test_parse_days_ago(self, db):
        """Should parse 'N days ago' format."""
        result = db._parse_relative_time("7 days ago")
        assert result is not None
        expected = datetime.now() - timedelta(days=7)
        assert abs((result - expected).total_seconds()) < 5

    def test_parse_weeks_ago(self, db):
        """Should parse 'N weeks ago' format."""
        result = db._parse_relative_time("2 weeks ago")
        assert result is not None
        expected = datetime.now() - timedelta(weeks=2)
        assert abs((result - expected).total_seconds()) < 5

    def test_parse_iso_format(self, db):
        """Should parse ISO format dates."""
        result = db._parse_relative_time("2026-01-01")
        assert result is not None
        assert result.year == 2026
        assert result.month == 1
        assert result.day == 1

    def test_parse_invalid_returns_none(self, db):
        """Should return None for unparseable strings."""
        result = db._parse_relative_time("not a time")
        assert result is None


class TestSchemaCreation:
    """Tests for database schema initialization."""

    def test_creates_events_table(self):
        """Should create events table with correct columns."""
        with tempfile.TemporaryDirectory() as tmpdir:
            diachron_dir = Path(tmpdir) / ".diachron"
            diachron_dir.mkdir()
            db = DiachronDB(project_root=Path(tmpdir))

            # Trigger schema creation
            conn = db._get_connection()
            cursor = conn.cursor()

            cursor.execute("PRAGMA table_info(events)")
            columns = {row[1] for row in cursor.fetchall()}

            expected_columns = {
                "id", "timestamp", "timestamp_display", "session_id",
                "tool_name", "file_path", "operation", "diff_summary",
                "raw_input", "ai_summary", "git_commit_sha",
                "parent_event_id", "metadata"
            }

            assert expected_columns.issubset(columns)
            db.close()

    def test_creates_indexes(self):
        """Should create indexes for common query patterns."""
        with tempfile.TemporaryDirectory() as tmpdir:
            diachron_dir = Path(tmpdir) / ".diachron"
            diachron_dir.mkdir()
            db = DiachronDB(project_root=Path(tmpdir))

            conn = db._get_connection()
            cursor = conn.cursor()

            cursor.execute("""
                SELECT name FROM sqlite_master
                WHERE type='index' AND tbl_name='events'
            """)
            indexes = {row[0] for row in cursor.fetchall()}

            assert "idx_events_timestamp" in indexes
            assert "idx_events_file_path" in indexes
            assert "idx_events_session_id" in indexes
            assert "idx_events_tool_name" in indexes

            db.close()


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
