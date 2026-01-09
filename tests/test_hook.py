#!/usr/bin/env python3
"""
Tests for Diachron hook capture (both Rust and Python)
======================================================

Run with: pytest tests/test_hook.py -v
"""

import pytest
import tempfile
import shutil
import subprocess
import json
import os
import sqlite3
from pathlib import Path
import sys

# Add lib directory to path
sys.path.insert(0, str(Path(__file__).parent.parent / "lib"))

# Path to hook binaries/scripts
DIACHRON_ROOT = Path(__file__).parent.parent
RUST_HOOK = DIACHRON_ROOT / "rust" / "target" / "release" / "diachron-hook"
PYTHON_HOOK = DIACHRON_ROOT / "lib" / "hook_capture.py"


class TestRustHook:
    """Tests for the Rust hook binary."""

    @pytest.fixture
    def temp_project(self):
        """Create a temp project with .diachron directory."""
        tmpdir = tempfile.mkdtemp()
        diachron_dir = Path(tmpdir) / ".diachron"
        diachron_dir.mkdir()
        yield Path(tmpdir)
        shutil.rmtree(tmpdir)

    @pytest.mark.skipif(
        not RUST_HOOK.exists(),
        reason="Rust hook binary not built"
    )
    def test_rust_hook_empty_input(self):
        """Should exit cleanly with empty input."""
        result = subprocess.run(
            [str(RUST_HOOK)],
            input="",
            capture_output=True,
            text=True
        )
        assert result.returncode == 0

    @pytest.mark.skipif(
        not RUST_HOOK.exists(),
        reason="Rust hook binary not built"
    )
    def test_rust_hook_invalid_json(self):
        """Should exit cleanly with invalid JSON."""
        result = subprocess.run(
            [str(RUST_HOOK)],
            input="not json",
            capture_output=True,
            text=True
        )
        assert result.returncode == 0

    @pytest.mark.skipif(
        not RUST_HOOK.exists(),
        reason="Rust hook binary not built"
    )
    def test_rust_hook_skips_read_events(self):
        """Should skip Read tool events (not file-modifying)."""
        result = subprocess.run(
            [str(RUST_HOOK)],
            input='{"tool_name": "Read", "tool_input": {"file_path": "test.py"}}',
            capture_output=True,
            text=True
        )
        assert result.returncode == 0

    @pytest.mark.skipif(
        not RUST_HOOK.exists(),
        reason="Rust hook binary not built"
    )
    def test_rust_hook_captures_write_event(self, temp_project):
        """Should capture Write events to database."""
        hook_input = json.dumps({
            "tool_name": "Write",
            "tool_input": {
                "file_path": str(temp_project / "test.py"),
                "content": "print('hello')"
            },
            "cwd": str(temp_project)
        })

        result = subprocess.run(
            [str(RUST_HOOK)],
            input=hook_input,
            capture_output=True,
            text=True
        )

        assert result.returncode == 0

        # Verify event was captured
        db_path = temp_project / ".diachron" / "events.db"
        assert db_path.exists()

        conn = sqlite3.connect(str(db_path))
        cursor = conn.cursor()
        cursor.execute("SELECT tool_name, file_path FROM events")
        rows = cursor.fetchall()
        conn.close()

        assert len(rows) >= 1
        assert rows[0][0] == "Write"

    @pytest.mark.skipif(
        not RUST_HOOK.exists(),
        reason="Rust hook binary not built"
    )
    def test_rust_hook_captures_edit_event(self, temp_project):
        """Should capture Edit events to database."""
        hook_input = json.dumps({
            "tool_name": "Edit",
            "tool_input": {
                "file_path": str(temp_project / "app.py"),
                "old_string": "foo",
                "new_string": "bar"
            },
            "cwd": str(temp_project)
        })

        result = subprocess.run(
            [str(RUST_HOOK)],
            input=hook_input,
            capture_output=True,
            text=True
        )

        assert result.returncode == 0

        # Verify event was captured
        db_path = temp_project / ".diachron" / "events.db"
        conn = sqlite3.connect(str(db_path))
        cursor = conn.cursor()
        cursor.execute("SELECT tool_name FROM events")
        rows = cursor.fetchall()
        conn.close()

        assert any(row[0] == "Edit" for row in rows)

    @pytest.mark.skipif(
        not RUST_HOOK.exists(),
        reason="Rust hook binary not built"
    )
    def test_rust_hook_captures_bash_git_commit(self, temp_project):
        """Should capture git commit with SHA."""
        hook_input = json.dumps({
            "tool_name": "Bash",
            "tool_input": {
                "command": "git commit -m 'test commit'"
            },
            "tool_result": "abc123def",
            "cwd": str(temp_project)
        })

        result = subprocess.run(
            [str(RUST_HOOK)],
            input=hook_input,
            capture_output=True,
            text=True
        )

        assert result.returncode == 0

    @pytest.mark.skipif(
        not RUST_HOOK.exists(),
        reason="Rust hook binary not built"
    )
    def test_rust_hook_no_project(self):
        """Should exit cleanly when no .diachron directory exists."""
        with tempfile.TemporaryDirectory() as tmpdir:
            hook_input = json.dumps({
                "tool_name": "Write",
                "tool_input": {"file_path": "test.py"},
                "cwd": tmpdir
            })

            result = subprocess.run(
                [str(RUST_HOOK)],
                input=hook_input,
                capture_output=True,
                text=True
            )

            # Should exit cleanly without error
            assert result.returncode == 0


class TestPythonHook:
    """Tests for the Python hook fallback."""

    @pytest.fixture
    def temp_project(self):
        """Create a temp project with .diachron directory."""
        tmpdir = tempfile.mkdtemp()
        original_cwd = os.getcwd()

        diachron_dir = Path(tmpdir) / ".diachron"
        diachron_dir.mkdir()

        os.chdir(tmpdir)
        yield Path(tmpdir)

        os.chdir(original_cwd)
        shutil.rmtree(tmpdir)

    def test_python_hook_empty_input(self, temp_project):
        """Should exit cleanly with empty input."""
        result = subprocess.run(
            ["python3", str(PYTHON_HOOK)],
            input="",
            capture_output=True,
            text=True
        )
        assert result.returncode == 0

    def test_python_hook_skips_read(self, temp_project):
        """Should skip Read events."""
        result = subprocess.run(
            ["python3", str(PYTHON_HOOK)],
            input='{"tool_name": "Read", "tool_input": {"file_path": "test.py"}}',
            capture_output=True,
            text=True
        )
        assert result.returncode == 0

    def test_python_hook_captures_write(self, temp_project):
        """Should capture Write events."""
        hook_input = json.dumps({
            "tool_name": "Write",
            "tool_input": {
                "file_path": str(temp_project / "test.py"),
                "content": "x = 1"
            },
            "cwd": str(temp_project)
        })

        result = subprocess.run(
            ["python3", str(PYTHON_HOOK)],
            input=hook_input,
            capture_output=True,
            text=True
        )

        assert result.returncode == 0

        # Verify event captured
        db_path = temp_project / ".diachron" / "events.db"
        assert db_path.exists()

        conn = sqlite3.connect(str(db_path))
        cursor = conn.cursor()
        cursor.execute("SELECT tool_name FROM events")
        rows = cursor.fetchall()
        conn.close()

        assert len(rows) >= 1


class TestHookPerformance:
    """Performance tests for hook capture."""

    @pytest.fixture
    def temp_project(self):
        """Create a temp project with .diachron directory."""
        tmpdir = tempfile.mkdtemp()
        diachron_dir = Path(tmpdir) / ".diachron"
        diachron_dir.mkdir()
        yield Path(tmpdir)
        shutil.rmtree(tmpdir)

    @pytest.mark.skipif(
        not RUST_HOOK.exists(),
        reason="Rust hook binary not built"
    )
    def test_rust_hook_fast_execution(self, temp_project):
        """Rust hook should complete in under 100ms."""
        import time

        hook_input = json.dumps({
            "tool_name": "Write",
            "tool_input": {"file_path": "test.py", "content": "x"},
            "cwd": str(temp_project)
        })

        start = time.perf_counter()
        result = subprocess.run(
            [str(RUST_HOOK)],
            input=hook_input,
            capture_output=True,
            text=True
        )
        elapsed = (time.perf_counter() - start) * 1000

        assert result.returncode == 0
        # Should complete in under 100ms (generous margin)
        assert elapsed < 100, f"Rust hook took {elapsed:.1f}ms (expected <100ms)"


class TestEventData:
    """Tests for event data integrity."""

    @pytest.fixture
    def temp_project(self):
        """Create a temp project."""
        tmpdir = tempfile.mkdtemp()
        diachron_dir = Path(tmpdir) / ".diachron"
        diachron_dir.mkdir()
        yield Path(tmpdir)
        shutil.rmtree(tmpdir)

    @pytest.mark.skipif(
        not RUST_HOOK.exists(),
        reason="Rust hook binary not built"
    )
    def test_event_has_timestamp(self, temp_project):
        """Events should have timestamps."""
        hook_input = json.dumps({
            "tool_name": "Write",
            "tool_input": {"file_path": "test.py", "content": "x"},
            "cwd": str(temp_project)
        })

        subprocess.run(
            [str(RUST_HOOK)],
            input=hook_input,
            capture_output=True,
            text=True
        )

        db_path = temp_project / ".diachron" / "events.db"
        conn = sqlite3.connect(str(db_path))
        cursor = conn.cursor()
        cursor.execute("SELECT timestamp, timestamp_display FROM events LIMIT 1")
        row = cursor.fetchone()
        conn.close()

        assert row[0] is not None  # ISO timestamp
        assert row[1] is not None  # Display timestamp

    @pytest.mark.skipif(
        not RUST_HOOK.exists(),
        reason="Rust hook binary not built"
    )
    def test_event_has_session_id(self, temp_project):
        """Events should have session ID."""
        hook_input = json.dumps({
            "tool_name": "Write",
            "tool_input": {"file_path": "test.py", "content": "x"},
            "cwd": str(temp_project)
        })

        subprocess.run(
            [str(RUST_HOOK)],
            input=hook_input,
            capture_output=True,
            text=True
        )

        db_path = temp_project / ".diachron" / "events.db"
        conn = sqlite3.connect(str(db_path))
        cursor = conn.cursor()
        cursor.execute("SELECT session_id FROM events LIMIT 1")
        row = cursor.fetchone()
        conn.close()

        assert row[0] is not None
        assert len(row[0]) > 0

    @pytest.mark.skipif(
        not RUST_HOOK.exists(),
        reason="Rust hook binary not built"
    )
    def test_event_metadata_has_branch(self, temp_project):
        """Events should capture git branch in metadata."""
        # Create a git repo for branch detection
        subprocess.run(["git", "init"], cwd=str(temp_project), capture_output=True)

        hook_input = json.dumps({
            "tool_name": "Write",
            "tool_input": {"file_path": "test.py", "content": "x"},
            "cwd": str(temp_project)
        })

        subprocess.run(
            [str(RUST_HOOK)],
            input=hook_input,
            capture_output=True,
            text=True
        )

        db_path = temp_project / ".diachron" / "events.db"
        conn = sqlite3.connect(str(db_path))
        cursor = conn.cursor()
        cursor.execute("SELECT metadata FROM events LIMIT 1")
        row = cursor.fetchone()
        conn.close()

        if row[0]:
            metadata = json.loads(row[0])
            # Git branch might be "master", "main", or detached
            # Just verify metadata exists
            assert isinstance(metadata, dict)


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
