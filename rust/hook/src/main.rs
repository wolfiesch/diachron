//! Diachron Hook - Fast PostToolUse event capture
//!
//! This is a Rust port of the Python hook_capture.py for maximum performance.
//! Target: <20ms total execution time vs Python's ~300ms.
//!
//! Architecture:
//! 1. Try sending event to daemon via Unix socket IPC (~16ms)
//! 2. If daemon unavailable, fall back to local database write (~13ms)
//!
//! Phase 2 enhancements:
//! - Git branch capture on every event
//! - Commit SHA for git commit events
//! - Semantic command categories (git, test, build, deploy, file_ops)

use chrono::Local;
use rusqlite::{params, Connection};
use serde::Deserialize;
use serde_json::{json, Value};
use std::env;
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

// Import shared types from core
use diachron_core::{CaptureEvent, CommandCategory, Operation, send_to_daemon, IpcError};

// ============================================================================
// HOOK INPUT PARSING
// ============================================================================

#[derive(Debug, Deserialize)]
struct HookInput {
    tool_name: String,
    tool_input: Value,
    tool_result: Option<String>,
    #[allow(dead_code)]
    session_id: Option<String>,
    #[allow(dead_code)]
    timestamp: Option<String>,
    cwd: Option<String>,
}

// ============================================================================
// GIT HELPERS
// ============================================================================

/// Get the current git branch name
fn get_current_branch(project_root: &PathBuf) -> Option<String> {
    Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(project_root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
            } else {
                None
            }
        })
}

/// Get the most recent commit SHA (short form)
fn get_last_commit_sha(project_root: &PathBuf) -> Option<String> {
    Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(project_root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
            } else {
                None
            }
        })
}

// ============================================================================
// COMMAND CLASSIFICATION
// ============================================================================

/// Bash commands that should be skipped (read-only)
const SKIP_PREFIXES: &[&str] = &[
    "ls", "cat", "head", "tail", "less", "more",
    "grep", "rg", "find", "fd", "ag",
    "git status", "git log", "git diff", "git branch", "git show",
    "pwd", "cd", "echo", "printf", "which", "whereis",
    "ps", "top", "htop", "df", "du",
    "python3 -c", "node -e",
    "hyperfine",  // Don't capture benchmark commands
];

fn classify_bash_command(cmd: &str) -> (Operation, Option<String>, CommandCategory) {
    let cmd_lower = cmd.to_lowercase();

    // Skip read-only commands
    for prefix in SKIP_PREFIXES {
        if cmd_lower.starts_with(prefix) {
            return (Operation::Unknown, None, CommandCategory::Unknown);
        }
    }

    // Git commands
    if cmd_lower.starts_with("git commit") {
        // Extract commit message
        let detail = if cmd.contains("-m") {
            cmd.split("-m")
                .nth(1)
                .map(|s| s.trim().trim_matches('"').trim_matches('\''))
                .map(|s| s.chars().take(200).collect())
        } else {
            None
        };
        return (Operation::Commit, detail, CommandCategory::Git);
    }

    if cmd_lower.starts_with("git push") || cmd_lower.starts_with("git pull")
       || cmd_lower.starts_with("git checkout") || cmd_lower.starts_with("git merge")
       || cmd_lower.starts_with("git rebase") || cmd_lower.starts_with("git stash") {
        return (Operation::Execute, None, CommandCategory::Git);
    }

    // Test commands
    if cmd_lower.starts_with("npm test") || cmd_lower.starts_with("yarn test")
       || cmd_lower.starts_with("pytest") || cmd_lower.starts_with("cargo test")
       || cmd_lower.starts_with("jest") || cmd_lower.starts_with("vitest")
       || cmd_lower.starts_with("go test") || cmd_lower.contains("test") {
        return (Operation::Execute, None, CommandCategory::Test);
    }

    // Build commands
    if cmd_lower.starts_with("npm run build") || cmd_lower.starts_with("yarn build")
       || cmd_lower.starts_with("cargo build") || cmd_lower.starts_with("make")
       || cmd_lower.starts_with("go build") || cmd_lower.starts_with("tsc")
       || cmd_lower.starts_with("webpack") || cmd_lower.starts_with("vite build") {
        return (Operation::Execute, None, CommandCategory::Build);
    }

    // Deploy commands
    if cmd_lower.contains("deploy") || cmd_lower.starts_with("vercel")
       || cmd_lower.starts_with("netlify") || cmd_lower.starts_with("fly ")
       || cmd_lower.starts_with("docker push") || cmd_lower.starts_with("kubectl apply") {
        return (Operation::Execute, None, CommandCategory::Deploy);
    }

    // Package management
    if cmd_lower.starts_with("npm install") || cmd_lower.starts_with("yarn add")
       || cmd_lower.starts_with("pip install") || cmd_lower.starts_with("cargo add")
       || cmd_lower.starts_with("brew install") || cmd_lower.starts_with("apt install") {
        return (Operation::Execute, None, CommandCategory::Package);
    }

    // File operations
    if cmd_lower.starts_with("rm") {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        let file = parts.iter().skip(1).find(|p| !p.starts_with('-'));
        return (Operation::Delete, file.map(|s| s.to_string()), CommandCategory::FileOps);
    }

    if cmd_lower.starts_with("mv") {
        let parts: Vec<&str> = cmd.split_whitespace()
            .filter(|p| !p.starts_with('-'))
            .collect();
        if parts.len() >= 3 {
            return (Operation::Move, Some(format!("{} → {}", parts[1], parts[2])), CommandCategory::FileOps);
        }
        return (Operation::Move, None, CommandCategory::FileOps);
    }

    if cmd_lower.starts_with("cp") {
        return (Operation::Copy, None, CommandCategory::FileOps);
    }

    if cmd_lower.starts_with("touch") || cmd_lower.starts_with("mkdir") {
        return (Operation::Create, None, CommandCategory::FileOps);
    }

    // Default for other commands
    (Operation::Execute, None, CommandCategory::Unknown)
}

// ============================================================================
// EVENT PARSING
// ============================================================================

fn parse_write_event(hook: &HookInput) -> CaptureEvent {
    let file_path = hook.tool_input.get("file_path")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let content = hook.tool_input.get("content")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Determine operation based on result
    let operation = match &hook.tool_result {
        Some(result) if result.to_lowercase().contains("overwritten") => Operation::Modify,
        _ => Operation::Create,
    };

    // Count lines
    let line_count = content.lines().count().max(1);
    let diff_summary = Some(format!("+{} lines", line_count));

    // Truncate raw input
    let raw_input = if content.len() > 500 {
        Some(format!("{}...", &content[..500]))
    } else if !content.is_empty() {
        Some(content.to_string())
    } else {
        None
    };

    CaptureEvent {
        tool_name: "Write".to_string(),
        file_path,
        operation,
        diff_summary,
        raw_input,
        metadata: None,
        git_commit_sha: None,
        command_category: None,
    }
}

fn parse_edit_event(hook: &HookInput) -> CaptureEvent {
    let file_path = hook.tool_input.get("file_path")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let old_string = hook.tool_input.get("old_string")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let new_string = hook.tool_input.get("new_string")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let old_lines = old_string.lines().count().max(1);
    let new_lines = new_string.lines().count().max(1);
    let diff = new_lines as i64 - old_lines as i64;

    let diff_summary = if diff > 0 {
        Some(format!("+{} lines", diff))
    } else if diff < 0 {
        Some(format!("{} lines", diff))
    } else {
        Some("modified (same line count)".to_string())
    };

    CaptureEvent {
        tool_name: "Edit".to_string(),
        file_path,
        operation: Operation::Modify,
        diff_summary,
        raw_input: None,
        metadata: None,
        git_commit_sha: None,
        command_category: None,
    }
}

fn parse_bash_event(hook: &HookInput, project_root: &PathBuf) -> Option<CaptureEvent> {
    let command = hook.tool_input.get("command")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let (operation, detail, category) = classify_bash_command(command);

    // Skip uninteresting commands
    if operation == Operation::Unknown {
        return None;
    }

    // Capture commit SHA after git commit commands
    let git_commit_sha = if operation == Operation::Commit {
        get_last_commit_sha(project_root)
    } else {
        None
    };

    Some(CaptureEvent {
        tool_name: "Bash".to_string(),
        file_path: None,
        operation,
        diff_summary: detail,
        raw_input: Some(command.chars().take(500).collect()),
        metadata: None,
        git_commit_sha,
        command_category: Some(category),
    })
}

fn parse_hook_input(hook: &HookInput, project_root: &PathBuf) -> Option<CaptureEvent> {
    let mut event = match hook.tool_name.as_str() {
        "Write" => Some(parse_write_event(hook)),
        "Edit" => Some(parse_edit_event(hook)),
        "Bash" => parse_bash_event(hook, project_root),
        _ => None,
    }?;

    // Add git branch metadata
    let git_branch = get_current_branch(project_root);
    if git_branch.is_some() || event.command_category.is_some() {
        let mut meta = json!({});
        if let Some(branch) = &git_branch {
            meta["git_branch"] = json!(branch);
        }
        if let Some(category) = &event.command_category {
            meta["command_category"] = json!(category.as_str());
        }
        event.metadata = Some(meta.to_string());
    }

    Some(event)
}

// ============================================================================
// DATABASE FALLBACK (when daemon is not running)
// ============================================================================

fn get_timestamp() -> (String, String) {
    let now = Local::now();
    let iso = now.format("%Y-%m-%dT%H:%M:%S%.3f").to_string();

    let tz_name = if now.format("%Z").to_string().contains("DT") {
        "PDT"
    } else {
        "PST"
    };

    let display = now.format(&format!("%m/%d/%Y %I:%M %p {}", tz_name)).to_string();
    (iso, display)
}

fn get_or_create_session_id(diachron_dir: &PathBuf) -> String {
    let session_file = diachron_dir.join(".session_id");
    let session_expiry_secs = 3600; // 1 hour

    if session_file.exists() {
        if let Ok(metadata) = fs::metadata(&session_file) {
            if let Ok(modified) = metadata.modified() {
                let age = SystemTime::now()
                    .duration_since(modified)
                    .unwrap_or_default()
                    .as_secs();

                if age < session_expiry_secs {
                    if let Ok(session_id) = fs::read_to_string(&session_file) {
                        let session_id = session_id.trim().to_string();
                        if !session_id.is_empty() {
                            let _ = fs::write(&session_file, &session_id);
                            return session_id;
                        }
                    }
                }
            }
        }
    }

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let new_id = format!("{:x}", timestamp)[..12.min(format!("{:x}", timestamp).len())].to_string();

    let _ = fs::write(&session_file, &new_id);
    new_id
}

fn init_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS events (
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

        CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(timestamp);
        CREATE INDEX IF NOT EXISTS idx_events_file_path ON events(file_path);
        CREATE INDEX IF NOT EXISTS idx_events_session_id ON events(session_id);
        CREATE INDEX IF NOT EXISTS idx_events_tool_name ON events(tool_name);"
    )
}

/// Fallback: Save event directly to local project database
fn save_to_local_db(event: &CaptureEvent, project_root: &PathBuf) -> rusqlite::Result<i64> {
    let diachron_dir = project_root.join(".diachron");
    let db_path = diachron_dir.join("events.db");

    let conn = Connection::open(&db_path)?;
    init_schema(&conn)?;

    let (timestamp_iso, timestamp_display) = get_timestamp();
    let session_id = get_or_create_session_id(&diachron_dir);

    conn.execute(
        "INSERT INTO events (
            timestamp, timestamp_display, session_id, tool_name, file_path,
            operation, diff_summary, raw_input, git_commit_sha, metadata
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            timestamp_iso,
            timestamp_display,
            session_id,
            event.tool_name,
            event.file_path,
            event.operation.as_str(),
            event.diff_summary,
            event.raw_input,
            event.git_commit_sha,
            event.metadata,
        ],
    )?;

    Ok(conn.last_insert_rowid())
}

// ============================================================================
// PROJECT ROOT DETECTION
// ============================================================================

fn find_project_root(start_path: Option<PathBuf>) -> Option<PathBuf> {
    let mut current = start_path.or_else(|| env::current_dir().ok())?;

    loop {
        if current.join(".diachron").exists() {
            return Some(current);
        }
        if current.join(".git").exists() {
            // Found git root but no .diachron
            return None;
        }
        if !current.pop() {
            return None;
        }
    }
}

// ============================================================================
// MAIN
// ============================================================================

fn main() {
    // Read JSON from stdin - Claude Code sends JSON and closes stdin properly
    let mut input = String::new();
    if io::stdin().read_to_string(&mut input).is_err() {
        std::process::exit(0);
    }

    // Skip empty input
    if input.trim().is_empty() {
        std::process::exit(0);
    }

    // Parse hook input
    let hook: HookInput = match serde_json::from_str(&input) {
        Ok(h) => h,
        Err(_) => std::process::exit(0),
    };

    // Determine project root from cwd in hook or current directory
    let start_path = hook.cwd.as_ref().map(PathBuf::from);
    let project_root = match find_project_root(start_path) {
        Some(p) => p,
        None => std::process::exit(0), // Not in a Diachron-enabled project
    };

    // Parse event
    let event = match parse_hook_input(&hook, &project_root) {
        Some(e) => e,
        None => std::process::exit(0), // Event should be skipped
    };

    // Try sending to daemon first (preferred path)
    match send_to_daemon(event.clone()) {
        Ok(()) => {
            // Daemon handled the event - success!
            std::process::exit(0);
        }
        Err(IpcError::DaemonNotRunning) => {
            // Daemon not available - fall back to local DB
            // This is the expected path when daemon hasn't been started
        }
        Err(_) => {
            // Other error - fall back to local DB
            // Could log this error somewhere for debugging
        }
    }

    // Fallback: Save directly to local project database
    let _ = save_to_local_db(&event, &project_root);

    std::process::exit(0);
}
