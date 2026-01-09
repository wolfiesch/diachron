#!/usr/bin/env python3
"""
Integration tests for Diachron timeline CLI (timeline_cli.py)
=============================================================

Run with: pytest tests/test_timeline.py -v
"""

import pytest
import tempfile
import shutil
import subprocess
import json
import os
from pathlib import Path
import sys

# Add lib directory to path
sys.path.insert(0, str(Path(__file__).parent.parent / "lib"))

from db import DiachronDB
from timeline_cli import (
    format_timestamp_str,
    get_display_timestamp,
    parse_metadata,
    print_timeline,
    print_stats,
)


class TestFormatTimestamp:
    """Tests for timestamp formatting utilities."""

    def test_format_iso_timestamp(self):
        """Should format ISO timestamp to display format."""
        iso = "2026-01-08T14:30:00"
        result = format_timestamp_str(iso)
        assert "01/08/2026" in result
        assert "02:30 PM" in result

    def test_format_empty_returns_unknown(self):
        """Should return 'Unknown' for empty input."""
        assert format_timestamp_str("") == "Unknown"
        assert format_timestamp_str(None) == "Unknown"

    def test_format_invalid_returns_original(self):
        """Should return original string if unparseable."""
        result = format_timestamp_str("not a date")
        assert result == "not a date"


class TestGetDisplayTimestamp:
    """Tests for display timestamp extraction."""

    def test_prefers_display_timestamp(self):
        """Should prefer timestamp_display if available."""
        event = {
            "timestamp": "2026-01-08T14:30:00",
            "timestamp_display": "01/08/2026 02:30 PM PST"
        }
        result = get_display_timestamp(event)
        assert result == "01/08/2026 02:30 PM PST"

    def test_falls_back_to_iso(self):
        """Should format ISO timestamp if display not available."""
        event = {
            "timestamp": "2026-01-08T14:30:00",
            "timestamp_display": None
        }
        result = get_display_timestamp(event)
        assert "01/08/2026" in result


class TestParseMetadata:
    """Tests for metadata JSON parsing."""

    def test_parse_valid_json(self):
        """Should parse valid JSON metadata."""
        result = parse_metadata('{"git_branch": "main", "category": "test"}')
        assert result["git_branch"] == "main"
        assert result["category"] == "test"

    def test_parse_empty_returns_dict(self):
        """Should return empty dict for empty input."""
        assert parse_metadata("") == {}
        assert parse_metadata(None) == {}

    def test_parse_invalid_returns_dict(self):
        """Should return empty dict for invalid JSON."""
        assert parse_metadata("not json") == {}
        assert parse_metadata("{broken") == {}


class TestCLIExecution:
    """Integration tests for CLI execution."""

    @pytest.fixture
    def temp_project(self):
        """Create a temp project with events."""
        tmpdir = tempfile.mkdtemp()
        original_cwd = os.getcwd()

        # Create .diachron directory
        diachron_dir = Path(tmpdir) / ".diachron"
        diachron_dir.mkdir()

        # Create database with test events
        db = DiachronDB(project_root=Path(tmpdir))
        db.insert_event(
            tool_name="Write",
            file_path="src/app.py",
            operation="create",
            diff_summary="+50 lines",
            metadata={"git_branch": "main"}
        )
        db.insert_event(
            tool_name="Edit",
            file_path="src/utils.py",
            operation="modify",
            diff_summary="+5/-3 lines"
        )
        db.insert_event(
            tool_name="Bash",
            file_path=None,
            operation="npm test",
            metadata={"command_category": "test"}
        )
        db.close()

        os.chdir(tmpdir)
        yield Path(tmpdir)

        os.chdir(original_cwd)
        shutil.rmtree(tmpdir)

    def test_timeline_basic(self, temp_project):
        """Should display timeline without errors."""
        cli_path = Path(__file__).parent.parent / "lib" / "timeline_cli.py"
        result = subprocess.run(
            ["python3", str(cli_path)],
            capture_output=True,
            text=True
        )

        assert result.returncode == 0
        assert "Timeline" in result.stdout
        assert "3 events" in result.stdout

    def test_timeline_stats(self, temp_project):
        """Should display statistics."""
        cli_path = Path(__file__).parent.parent / "lib" / "timeline_cli.py"
        result = subprocess.run(
            ["python3", str(cli_path), "--stats"],
            capture_output=True,
            text=True
        )

        assert result.returncode == 0
        assert "Statistics" in result.stdout
        assert "Total Events:" in result.stdout
        assert "3" in result.stdout

    def test_timeline_json_output(self, temp_project):
        """Should output valid JSON with --json flag."""
        cli_path = Path(__file__).parent.parent / "lib" / "timeline_cli.py"
        result = subprocess.run(
            ["python3", str(cli_path), "--json"],
            capture_output=True,
            text=True
        )

        assert result.returncode == 0
        events = json.loads(result.stdout)
        assert isinstance(events, list)
        assert len(events) == 3

    def test_timeline_filter_by_tool(self, temp_project):
        """Should filter events by tool name."""
        cli_path = Path(__file__).parent.parent / "lib" / "timeline_cli.py"
        result = subprocess.run(
            ["python3", str(cli_path), "--tool", "Write", "--json"],
            capture_output=True,
            text=True
        )

        assert result.returncode == 0
        events = json.loads(result.stdout)
        assert len(events) == 1
        assert events[0]["tool_name"] == "Write"

    def test_timeline_filter_by_file(self, temp_project):
        """Should filter events by file path."""
        cli_path = Path(__file__).parent.parent / "lib" / "timeline_cli.py"
        result = subprocess.run(
            ["python3", str(cli_path), "--file", "src/", "--json"],
            capture_output=True,
            text=True
        )

        assert result.returncode == 0
        events = json.loads(result.stdout)
        assert len(events) == 2
        assert all(e["file_path"].startswith("src/") for e in events)

    def test_timeline_limit(self, temp_project):
        """Should respect limit parameter."""
        cli_path = Path(__file__).parent.parent / "lib" / "timeline_cli.py"
        result = subprocess.run(
            ["python3", str(cli_path), "--limit", "1", "--json"],
            capture_output=True,
            text=True
        )

        assert result.returncode == 0
        events = json.loads(result.stdout)
        assert len(events) == 1

    def test_timeline_not_initialized_error(self):
        """Should show error when diachron not initialized."""
        with tempfile.TemporaryDirectory() as tmpdir:
            original_cwd = os.getcwd()
            os.chdir(tmpdir)

            cli_path = Path(__file__).parent.parent / "lib" / "timeline_cli.py"
            result = subprocess.run(
                ["python3", str(cli_path)],
                capture_output=True,
                text=True
            )

            os.chdir(original_cwd)

            assert result.returncode != 0
            assert "not initialized" in result.stderr.lower()


class TestExportFunctionality:
    """Tests for export features."""

    @pytest.fixture
    def temp_project(self):
        """Create a temp project with events."""
        tmpdir = tempfile.mkdtemp()
        original_cwd = os.getcwd()

        diachron_dir = Path(tmpdir) / ".diachron"
        diachron_dir.mkdir()

        db = DiachronDB(project_root=Path(tmpdir))
        db.insert_event(
            tool_name="Write",
            file_path="README.md",
            operation="create"
        )
        db.close()

        os.chdir(tmpdir)
        yield Path(tmpdir)

        os.chdir(original_cwd)
        shutil.rmtree(tmpdir)

    def test_export_markdown(self, temp_project):
        """Should export to markdown file."""
        cli_path = Path(__file__).parent.parent / "lib" / "timeline_cli.py"
        result = subprocess.run(
            ["python3", str(cli_path), "--export", "markdown"],
            capture_output=True,
            text=True
        )

        assert result.returncode == 0
        assert (temp_project / "TIMELINE.md").exists()

        content = (temp_project / "TIMELINE.md").read_text()
        assert "Timeline" in content
        assert "README.md" in content


class TestOutputFormatting:
    """Tests for output formatting functions."""

    def test_print_timeline_empty(self, capsys):
        """Should handle empty event list."""
        print_timeline([])
        captured = capsys.readouterr()
        assert "No events found" in captured.out

    def test_print_timeline_with_events(self, capsys):
        """Should format events with emojis."""
        events = [{
            "timestamp": "2026-01-08T14:30:00",
            "timestamp_display": "01/08/2026 02:30 PM PST",
            "tool_name": "Write",
            "file_path": "test.py",
            "operation": "create",
            "diff_summary": "+10 lines",
            "ai_summary": None,
            "git_commit_sha": None,
            "metadata": '{"git_branch": "main"}',
            "session_id": "abc123def456"
        }]

        print_timeline(events)
        captured = capsys.readouterr()

        assert "📝" in captured.out  # Write emoji
        assert "test.py" in captured.out
        assert "create" in captured.out
        assert "main" in captured.out  # Branch from metadata

    def test_print_timeline_bash_category(self, capsys):
        """Should show command category for Bash events."""
        events = [{
            "timestamp": "2026-01-08T14:30:00",
            "timestamp_display": "01/08/2026 02:30 PM PST",
            "tool_name": "Bash",
            "file_path": None,
            "operation": "npm test",
            "diff_summary": None,
            "ai_summary": None,
            "git_commit_sha": None,
            "metadata": '{"command_category": "test"}',
            "session_id": "abc123def456"
        }]

        print_timeline(events)
        captured = capsys.readouterr()

        assert "🖥️" in captured.out  # Bash emoji
        assert "[test]" in captured.out  # Category badge

    def test_print_stats(self, capsys):
        """Should format statistics."""
        stats = {
            "total_events": 100,
            "total_sessions": 5,
            "unique_files": 25,
            "first_event": "2026-01-01T00:00:00",
            "last_event": "2026-01-08T14:30:00",
            "by_tool": {
                "Write": 50,
                "Edit": 30,
                "Bash": 20
            }
        }

        print_stats(stats)
        captured = capsys.readouterr()

        assert "Statistics" in captured.out
        assert "100" in captured.out
        assert "Write" in captured.out
        assert "50%" in captured.out  # Write percentage


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
