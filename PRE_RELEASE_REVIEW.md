# Diachron Pre-Release Review

**Review Date:** 01/08/2026 05:50 AM PST
**Reviewer:** Claude (Opus 4.5)
**Status:** ⚠️ Ready with caveats

---

## Executive Summary

The Diachron core (database, timeline CLI, export) is **production-ready**. However, the **automatic capture via hooks is experimental** and needs live testing before relying on it.

---

## ✅ What's Working Well

### 1. Database Layer (`lib/db.py`)
- [x] SQL injection protected (parameterized queries)
- [x] Unicode file paths supported
- [x] Large input truncation
- [x] Session ID generation
- [x] Schema migration (v1 → v2)
- [x] Timestamp sorting FIXED (ISO + display format)
- [x] Time filtering works correctly
- [x] Concurrent access tested

### 2. Timeline CLI (`lib/timeline_cli.py`)
- [x] Clean formatted output with emojis
- [x] Time filtering (`--since`, `--until`)
- [x] File filtering (`--file`)
- [x] Tool filtering (`--tool`)
- [x] Statistics view (`--stats`)
- [x] Markdown export (`--export markdown`)
- [x] JSON export (`--export json`)

### 3. Skill Structure
- [x] Proper frontmatter format
- [x] User-invocable commands (`/timeline`, `/diachron`)
- [x] Documentation in README.md

---

## ⚠️ Issues to Address Before Publishing

### 🔴 CRITICAL: Hook Mechanism Uncertainty

**Issue:** The `capture.md` skill uses PostToolUse hooks, but:
1. Claude Code 2.1's hook context passing is undocumented
2. It's unclear if the skill receives tool name, parameters, and results
3. The skill contains instructions for Claude to execute, but may not have the context needed

**Recommendation:**
- Test the hook manually by writing a file and checking if an event appears
- If hooks don't work as expected, implement alternative:
  - **Option A:** Shell-based hook in `settings.json`:
    ```json
    {
      "hooks": {
        "PostToolUse": [
          {
            "matcher": "Write|Edit",
            "command": "python3 ~/.claude/skills/diachron/lib/capture_event.py --tool $TOOL_NAME --file $FILE_PATH"
          }
        ]
      }
    }
    ```
  - **Option B:** Manual capture command `/diachron capture`

**Status:** ⚠️ Needs live testing

---

### 🟡 MODERATE: Session ID Not Persisted

**Issue:** Each Python script invocation generates a new session ID. Events within a single Claude Code session may have different session IDs.

**Impact:** The `--session` filter won't group events correctly.

**Fix Options:**
1. Store session ID in a temp file (`.diachron/.session_id`)
2. Use Claude Code's actual session ID if accessible via environment variable
3. Group by time proximity instead of session ID

**Recommended Fix:**
```python
def get_or_create_session_id():
    session_file = Path(".diachron/.session_id")
    if session_file.exists():
        mtime = session_file.stat().st_mtime
        if time.time() - mtime < 3600:  # 1 hour expiry
            return session_file.read_text().strip()
    new_id = generate_session_id()
    session_file.write_text(new_id)
    return new_id
```

---

### 🟡 MODERATE: Error Messages Could Be Friendlier

**Issue:** Some error messages are technical:
- "Diachron not initialized in this project" → Good
- Database errors may expose SQLite internals → Should wrap

**Recommendation:** Add try/except wrappers with user-friendly messages in timeline_cli.py

---

### 🟢 MINOR: Missing Features (OK for MVP)

These are fine to skip for initial release:

1. **AI Summaries** - Phase 2 feature, config exists but not implemented
2. **Git correlation** - Would be nice but not critical
3. **Web dashboard** - Separate project
4. **Retention cleanup** - `/diachron clean` exists but runs manually

---

### 🟢 MINOR: Documentation Gaps

1. Add installation instructions:
   ```bash
   # Clone to skills directory
   git clone https://github.com/youruser/diachron ~/.claude/skills/diachron
   ```

2. Add troubleshooting section:
   - "Events not appearing" → Check `.diachron` exists
   - "Database locked" → Close other connections

---

## Security Checklist

| Check | Status | Notes |
|-------|--------|-------|
| SQL injection | ✅ Safe | Parameterized queries |
| Path traversal | ✅ Safe | Paths relative to project root |
| Sensitive data exposure | ✅ Safe | No secrets stored |
| Input validation | ⚠️ Minimal | Large inputs truncated, but no strict validation |
| Permissions | ✅ Safe | Uses user's permissions |

---

## Performance Considerations

| Concern | Status | Notes |
|---------|--------|-------|
| Large databases | ✅ OK | SQLite handles millions of rows |
| Query performance | ✅ OK | Indexed on timestamp, file_path, session_id |
| Hook overhead | ⚠️ Unknown | Each Write/Edit triggers hook |
| Memory usage | ✅ Low | Streaming queries, no bulk loading |

---

## Recommended Pre-Publish Tasks

### Must Do (Blocking)
1. [ ] **Test the PostToolUse hook live** - Write a file, check if event captured
2. [ ] **If hook fails, implement alternative** - Shell hook or manual capture
3. [ ] **Fix session ID persistence** - Store in temp file

### Should Do
4. [ ] Add try/except wrappers in CLI for friendlier errors
5. [ ] Add installation instructions to README
6. [ ] Test on a fresh project (not the Diachron dev project)

### Nice to Have
7. [ ] Add `--watch` mode to timeline (live updates)
8. [ ] Add `--json` output to all commands
9. [ ] Create demo GIF for README

---

## Test Commands for Live Verification

```bash
# 1. Initialize in a test project
cd ~/projects/test-project
/diachron init

# 2. Write a file (should trigger capture)
# Use Claude Code to write src/test.ts

# 3. Check if event was captured
/timeline

# 4. If no event appears, the hook didn't work
# Check ~/.claude/skills/diachron/capture.md

# 5. Manual capture test
python3 ~/.claude/skills/diachron/lib/capture_event.py \
  --tool Write \
  --file src/test.ts \
  --op create \
  --diff "+10 lines"

# 6. Verify manual capture worked
/timeline
```

---

## Conclusion

**Diachron is 85% production-ready.** The core functionality is solid, but the automatic hook capture needs live validation before claiming it works.

**Recommended approach:**
1. Publish as "beta" with clear documentation that auto-capture is experimental
2. Provide the manual capture CLI as a fallback
3. Gather user feedback on hook behavior
4. Iterate based on real-world usage

---

*Generated by Claude Code review process*
