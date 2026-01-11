#!/usr/bin/env python3
"""
Tests for Diachron Codex Capture Module
=======================================
Unit tests for JSONL parsing and file operation extraction.

Run with: python3 -m pytest test_codex_capture.py -v
"""

import json
import tempfile
from pathlib import Path
import sys

# Add lib directory to path
sys.path.insert(0, str(Path(__file__).parent))

from codex_capture import (
    parse_patch_content,
    classify_command,
    parse_codex_jsonl,
)


class TestParsePatchContent:
    """Tests for apply_patch content parsing."""

    def test_add_file(self):
        """Test parsing a new file creation."""
        patch = """*** Begin Patch
*** Add File: src/new_module.py
+#!/usr/bin/env python3
+def hello():
+    print("Hello")
*** End Patch"""

        ops = parse_patch_content(patch)
        assert len(ops) == 1
        assert ops[0]["operation"] == "create"
        assert ops[0]["file_path"] == "src/new_module.py"
        assert "+3" in ops[0]["diff_summary"]

    def test_update_file(self):
        """Test parsing a file modification."""
        patch = """*** Begin Patch
*** Update File: src/existing.py
@@
-old_line
+new_line
+another_new_line
@@
*** End Patch"""

        ops = parse_patch_content(patch)
        assert len(ops) == 1
        assert ops[0]["operation"] == "modify"
        assert ops[0]["file_path"] == "src/existing.py"
        assert "+2" in ops[0]["diff_summary"]
        assert "-1" in ops[0]["diff_summary"]

    def test_delete_file(self):
        """Test parsing a file deletion."""
        patch = """*** Begin Patch
*** Delete File: src/obsolete.py
*** End Patch"""

        ops = parse_patch_content(patch)
        assert len(ops) == 1
        assert ops[0]["operation"] == "delete"
        assert ops[0]["file_path"] == "src/obsolete.py"

    def test_multiple_files(self):
        """Test parsing multiple file operations in one patch."""
        patch = """*** Begin Patch
*** Add File: src/new1.py
+content
*** Update File: src/existing.py
+more content
*** Delete File: src/old.py
*** End Patch"""

        ops = parse_patch_content(patch)
        assert len(ops) == 3
        assert ops[0]["operation"] == "create"
        assert ops[1]["operation"] == "modify"
        assert ops[2]["operation"] == "delete"


class TestClassifyCommand:
    """Tests for shell command classification."""

    def test_git_commands(self):
        """Test git command classification."""
        assert classify_command("git commit -m 'test'") == ("git", None)
        assert classify_command("git add .") == ("git", ".")
        assert classify_command("git rm file.txt") == ("git", "file.txt")

    def test_file_operations(self):
        """Test file operation command classification."""
        assert classify_command("rm -rf node_modules")[0] == "fileops"
        assert classify_command("mv old.txt new.txt")[0] == "fileops"
        assert classify_command("cp src dst")[0] == "fileops"
        assert classify_command("touch newfile.txt")[0] == "fileops"
        assert classify_command("mkdir newdir")[0] == "fileops"

    def test_package_commands(self):
        """Test package manager command classification."""
        assert classify_command("npm install lodash")[0] == "package"
        assert classify_command("yarn add express")[0] == "package"
        assert classify_command("pip install requests")[0] == "package"
        assert classify_command("cargo add serde")[0] == "package"

    def test_read_only_commands(self):
        """Test that read-only commands are not classified."""
        assert classify_command("ls -la") == (None, None)
        assert classify_command("cat file.txt") == (None, None)
        assert classify_command("grep pattern file") == (None, None)
        assert classify_command("git status") == (None, None)
        assert classify_command("git log --oneline") == (None, None)


class TestParseCodexJsonl:
    """Tests for full JSONL session parsing."""

    def test_parse_session_meta(self):
        """Test extracting session metadata."""
        jsonl_content = """\
{"timestamp":"2026-01-11T10:00:00Z","type":"session_meta","payload":{"id":"test-session-123","cwd":"/test/project","cli_version":"0.80.0"}}
{"timestamp":"2026-01-11T10:00:01Z","type":"response_item","payload":{"type":"text","content":"Hello"}}
"""
        with tempfile.NamedTemporaryFile(mode='w', suffix='.jsonl', delete=False) as f:
            f.write(jsonl_content)
            f.flush()

            result = parse_codex_jsonl(Path(f.name))

            assert result["session_id"] == "test-session-123"
            assert result["cwd"] == "/test/project"
            assert result["cli_version"] == "0.80.0"

    def test_parse_apply_patch_event(self):
        """Test extracting file operations from apply_patch events."""
        patch_content = "*** Begin Patch\n*** Add File: src/test.py\n+def test(): pass\n*** End Patch"
        jsonl_content = f"""\
{{"timestamp":"2026-01-11T10:00:00Z","type":"session_meta","payload":{{"id":"test-123","cwd":"/test"}}}}
{{"timestamp":"2026-01-11T10:00:01Z","type":"response_item","payload":{{"type":"custom_tool_call","name":"apply_patch","input":{json.dumps(patch_content)}}}}}
"""
        with tempfile.NamedTemporaryFile(mode='w', suffix='.jsonl', delete=False) as f:
            f.write(jsonl_content)
            f.flush()

            result = parse_codex_jsonl(Path(f.name))

            assert len(result["operations"]) == 1
            assert result["operations"][0]["operation"] == "create"
            assert result["operations"][0]["file_path"] == "src/test.py"

    def test_parse_exec_command_event(self):
        """Test extracting file-modifying shell commands."""
        jsonl_content = """\
{"timestamp":"2026-01-11T10:00:00Z","type":"session_meta","payload":{"id":"test-123","cwd":"/test"}}
{"timestamp":"2026-01-11T10:00:01Z","type":"response_item","payload":{"type":"function_call","name":"exec_command","arguments":"{\\"cmd\\":\\"git commit -m 'test commit'\\"}"}}
"""
        with tempfile.NamedTemporaryFile(mode='w', suffix='.jsonl', delete=False) as f:
            f.write(jsonl_content)
            f.flush()

            result = parse_codex_jsonl(Path(f.name))

            assert len(result["operations"]) == 1
            assert result["operations"][0]["operation"] == "execute"
            assert result["operations"][0]["command_category"] == "git"

    def test_skip_read_only_commands(self):
        """Test that read-only commands are not captured."""
        jsonl_content = """\
{"timestamp":"2026-01-11T10:00:00Z","type":"session_meta","payload":{"id":"test-123","cwd":"/test"}}
{"timestamp":"2026-01-11T10:00:01Z","type":"response_item","payload":{"type":"function_call","name":"exec_command","arguments":"{\\"cmd\\":\\"ls -la\\"}"}}
{"timestamp":"2026-01-11T10:00:02Z","type":"response_item","payload":{"type":"function_call","name":"exec_command","arguments":"{\\"cmd\\":\\"cat file.txt\\"}"}}
"""
        with tempfile.NamedTemporaryFile(mode='w', suffix='.jsonl', delete=False) as f:
            f.write(jsonl_content)
            f.flush()

            result = parse_codex_jsonl(Path(f.name))

            assert len(result["operations"]) == 0


if __name__ == "__main__":
    import pytest
    pytest.main([__file__, "-v"])
