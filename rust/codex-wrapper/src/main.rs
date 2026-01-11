//! Diachron Codex Wrapper
//! ======================
//!
//! Standalone wrapper for OpenAI Codex CLI that automatically captures
//! file operations for Diachron provenance tracking.
//!
//! Usage:
//!     diachron-codex exec "task description"
//!     diachron-codex exec --model gpt-5.2-codex "task description"
//!
//! This is a transparent wrapper - all arguments are passed through to `codex`.
//!
//! ============================================================================
//! CHANGELOG (recent first, max 5 entries)
//! 01/11/2026 - Initial implementation for Diachron v0.7 (Claude)
//! ============================================================================

use anyhow::{Context, Result};
use clap::Parser;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::Command;
use tracing::{debug, info, warn};

/// Diachron wrapper for Codex CLI - tracks file operations for provenance
#[derive(Parser, Debug)]
#[command(
    name = "diachron-codex",
    about = "Codex CLI wrapper with Diachron provenance tracking",
    version,
    trailing_var_arg = true
)]
struct Args {
    /// Arguments to pass through to codex
    #[arg(trailing_var_arg = true)]
    codex_args: Vec<String>,

    /// Skip sending events to Diachron (useful for testing)
    #[arg(long, hide = true)]
    no_diachron: bool,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,
}

/// Codex session metadata from session_meta event
#[derive(Debug, Deserialize)]
struct SessionMeta {
    id: String,
    cwd: Option<String>,
    cli_version: Option<String>,
}

/// A file operation extracted from Codex session
#[derive(Debug, Clone)]
struct FileOperation {
    file_path: Option<String>,
    operation: String,
    diff_summary: Option<String>,
    command_category: Option<String>,
    raw_input: Option<String>,
    timestamp: Option<String>,
}

/// IPC message to Diachron daemon
#[derive(Debug, Serialize)]
struct CaptureMessage {
    #[serde(rename = "type")]
    msg_type: String,
    payload: CapturePayload,
}

#[derive(Debug, Serialize)]
struct CapturePayload {
    tool_name: String,
    file_path: Option<String>,
    operation: Option<String>,
    diff_summary: Option<String>,
    raw_input: Option<String>,
    metadata: Option<String>,
    git_commit_sha: Option<String>,
    command_category: Option<String>,
}

/// File-modifying commands to capture
fn is_file_modifying_command(cmd: &str) -> Option<&'static str> {
    let cmd_lower = cmd.to_lowercase();

    let patterns = [
        ("git commit", "git"),
        ("git add", "git"),
        ("git rm", "git"),
        ("git mv", "git"),
        ("rm ", "fileops"),
        ("rm -", "fileops"),
        ("mv ", "fileops"),
        ("cp ", "fileops"),
        ("touch ", "fileops"),
        ("mkdir ", "fileops"),
        ("rmdir ", "fileops"),
        ("> ", "fileops"),
        (">> ", "fileops"),
        ("npm install", "package"),
        ("yarn add", "package"),
        ("pip install", "package"),
        ("cargo add", "package"),
    ];

    for (pattern, category) in patterns {
        if cmd_lower.contains(pattern) {
            return Some(category);
        }
    }
    None
}

/// Get current git branch
fn get_git_branch() -> Option<String> {
    let output = Command::new("git")
        .args(["branch", "--show-current"])
        .output()
        .ok()?;

    if output.status.success() {
        let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !branch.is_empty() {
            return Some(branch);
        }
    }
    None
}

/// Find the most recent Codex session JSONL file
fn find_latest_session() -> Option<PathBuf> {
    let codex_dir = dirs::home_dir()?.join(".codex").join("sessions");
    if !codex_dir.exists() {
        return None;
    }

    // Find all JSONL files and sort by modification time
    let mut files: Vec<_> = walkdir::WalkDir::new(&codex_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "jsonl"))
        .collect();

    // Sort by modification time (most recent first)
    files.sort_by(|a, b| {
        let a_time = a.metadata().ok().and_then(|m| m.modified().ok());
        let b_time = b.metadata().ok().and_then(|m| m.modified().ok());
        b_time.cmp(&a_time)
    });

    files.first().map(|e| e.path().to_path_buf())
}

/// Parse patch content to extract file operations
fn parse_patch_content(patch: &str) -> Vec<FileOperation> {
    let mut operations = Vec::new();

    let add_re = Regex::new(r"\*\*\* Add File:\s*(.+?)(?:\n|$)").unwrap();
    let update_re = Regex::new(r"\*\*\* Update File:\s*(.+?)(?:\n|$)").unwrap();
    let delete_re = Regex::new(r"\*\*\* Delete File:\s*(.+?)(?:\n|$)").unwrap();

    // Count lines for diff summary
    let lines_added = patch.lines().filter(|l| l.starts_with('+') && !l.starts_with("++")).count();
    let lines_removed = patch.lines().filter(|l| l.starts_with('-') && !l.starts_with("--")).count();

    let diff_summary = if lines_added > 0 || lines_removed > 0 {
        let mut parts = Vec::new();
        if lines_added > 0 {
            parts.push(format!("+{}", lines_added));
        }
        if lines_removed > 0 {
            parts.push(format!("-{}", lines_removed));
        }
        Some(format!("{} lines", parts.join(" ")))
    } else {
        None
    };

    for cap in add_re.captures_iter(patch) {
        operations.push(FileOperation {
            file_path: Some(cap[1].trim().to_string()),
            operation: "create".to_string(),
            diff_summary: diff_summary.clone().or(Some("new file".to_string())),
            command_category: None,
            raw_input: Some(patch.chars().take(500).collect()),
            timestamp: None,
        });
    }

    for cap in update_re.captures_iter(patch) {
        operations.push(FileOperation {
            file_path: Some(cap[1].trim().to_string()),
            operation: "modify".to_string(),
            diff_summary: diff_summary.clone().or(Some("updated".to_string())),
            command_category: None,
            raw_input: Some(patch.chars().take(500).collect()),
            timestamp: None,
        });
    }

    for cap in delete_re.captures_iter(patch) {
        operations.push(FileOperation {
            file_path: Some(cap[1].trim().to_string()),
            operation: "delete".to_string(),
            diff_summary: Some("file deleted".to_string()),
            command_category: None,
            raw_input: None,
            timestamp: None,
        });
    }

    operations
}

/// Parse a Codex session JSONL file
fn parse_codex_session(path: &PathBuf) -> Result<(Option<SessionMeta>, Vec<FileOperation>)> {
    let file = std::fs::File::open(path)?;
    let reader = BufReader::new(file);

    let mut session_meta: Option<SessionMeta> = None;
    let mut operations = Vec::new();

    for line in reader.lines() {
        let line = line?;
        if line.is_empty() {
            continue;
        }

        let event: serde_json::Value = serde_json::from_str(&line)
            .with_context(|| format!("Failed to parse JSON line: {}", &line[..50.min(line.len())]))?;

        let event_type = event.get("type").and_then(|v| v.as_str());
        let timestamp = event.get("timestamp").and_then(|v| v.as_str()).map(|s| s.to_string());
        let payload = event.get("payload");

        match event_type {
            Some("session_meta") => {
                if let Some(p) = payload {
                    session_meta = serde_json::from_value(p.clone()).ok();
                }
            }
            Some("response_item") => {
                if let Some(p) = payload {
                    let inner_type = p.get("type").and_then(|v| v.as_str());
                    let tool_name = p.get("name").and_then(|v| v.as_str());

                    // apply_patch events
                    if inner_type == Some("custom_tool_call") && tool_name == Some("apply_patch") {
                        if let Some(input) = p.get("input").and_then(|v| v.as_str()) {
                            let mut ops = parse_patch_content(input);
                            for op in &mut ops {
                                op.timestamp = timestamp.clone();
                            }
                            operations.extend(ops);
                        }
                    }

                    // exec_command events
                    if inner_type == Some("function_call") && tool_name == Some("exec_command") {
                        if let Some(args_str) = p.get("arguments").and_then(|v| v.as_str()) {
                            if let Ok(args) = serde_json::from_str::<serde_json::Value>(args_str) {
                                if let Some(cmd) = args.get("cmd").and_then(|v| v.as_str()) {
                                    if let Some(category) = is_file_modifying_command(cmd) {
                                        operations.push(FileOperation {
                                            file_path: None, // Complex to extract reliably
                                            operation: "execute".to_string(),
                                            diff_summary: Some(cmd.chars().take(100).collect()),
                                            command_category: Some(category.to_string()),
                                            raw_input: Some(cmd.to_string()),
                                            timestamp: timestamp.clone(),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    Ok((session_meta, operations))
}

/// Send operations to Diachron daemon
fn send_to_daemon(
    operations: &[FileOperation],
    session_id: &str,
    git_branch: Option<&str>,
    cli_version: Option<&str>,
    cwd: Option<&str>,
) -> Result<usize> {
    let socket_path = dirs::home_dir()
        .context("No home directory")?
        .join(".diachron")
        .join("diachron.sock");

    if !socket_path.exists() {
        warn!("Diachron daemon not running ({})", socket_path.display());
        return Ok(0);
    }

    let mut success_count = 0;

    for op in operations {
        let mut metadata = HashMap::new();
        metadata.insert("codex_session_id", session_id.to_string());
        if let Some(v) = cli_version {
            metadata.insert("codex_version", v.to_string());
        }
        if let Some(b) = git_branch {
            metadata.insert("git_branch", b.to_string());
        }
        if let Some(c) = cwd {
            metadata.insert("cwd", c.to_string());
        }
        if let Some(cat) = &op.command_category {
            metadata.insert("command_category", cat.clone());
        }

        let message = CaptureMessage {
            msg_type: "Capture".to_string(),
            payload: CapturePayload {
                tool_name: "Codex".to_string(),
                file_path: op.file_path.clone(),
                operation: Some(op.operation.clone()),
                diff_summary: op.diff_summary.clone(),
                raw_input: op.raw_input.clone(),
                metadata: Some(serde_json::to_string(&metadata)?),
                git_commit_sha: None,
                command_category: op.command_category.clone(),
            },
        };

        match send_message(&socket_path, &message) {
            Ok(_) => success_count += 1,
            Err(e) => warn!("Failed to send event: {}", e),
        }
    }

    Ok(success_count)
}

/// Send a single message to the daemon
fn send_message(socket_path: &PathBuf, message: &CaptureMessage) -> Result<()> {
    let mut stream = UnixStream::connect(socket_path)?;

    let json = serde_json::to_string(message)? + "\n";
    stream.write_all(json.as_bytes())?;

    // Read response
    let mut response = String::new();
    let mut reader = BufReader::new(&stream);
    reader.read_line(&mut response)?;

    let resp: serde_json::Value = serde_json::from_str(&response)?;
    if resp.get("type").and_then(|v| v.as_str()) == Some("Ok") {
        Ok(())
    } else {
        anyhow::bail!("Daemon error: {:?}", resp)
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    if args.verbose {
        tracing_subscriber::fmt()
            .with_env_filter("debug")
            .init();
    }

    // Get git branch before running codex
    let git_branch = get_git_branch();
    debug!("Git branch: {:?}", git_branch);

    // Run codex with passthrough args
    info!("Running codex with args: {:?}", args.codex_args);

    let status = Command::new("codex")
        .args(&args.codex_args)
        .status()
        .context("Failed to run codex. Is it installed?")?;

    // After codex completes, capture events if not disabled
    if !args.no_diachron {
        // Find the latest session
        if let Some(session_path) = find_latest_session() {
            info!("Parsing session: {}", session_path.display());

            match parse_codex_session(&session_path) {
                Ok((meta, operations)) => {
                    let session_id = meta.as_ref().map(|m| m.id.as_str()).unwrap_or("unknown");
                    let cli_version = meta.as_ref().and_then(|m| m.cli_version.as_deref());
                    let cwd = meta.as_ref().and_then(|m| m.cwd.as_deref());

                    if operations.is_empty() {
                        info!("No file operations found in session");
                    } else {
                        match send_to_daemon(
                            &operations,
                            session_id,
                            git_branch.as_deref(),
                            cli_version,
                            cwd,
                        ) {
                            Ok(count) => {
                                info!("Captured {}/{} Codex operations for Diachron", count, operations.len());
                            }
                            Err(e) => {
                                warn!("Failed to send to Diachron: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to parse Codex session: {}", e);
                }
            }
        } else {
            warn!("No Codex session files found");
        }
    }

    // Exit with codex's exit code
    std::process::exit(status.code().unwrap_or(1));
}

// Add walkdir dependency
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_patch_create() {
        let patch = "*** Begin Patch\n*** Add File: src/new.rs\n+fn main() {}\n*** End Patch";
        let ops = parse_patch_content(patch);
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].operation, "create");
        assert_eq!(ops[0].file_path, Some("src/new.rs".to_string()));
    }

    #[test]
    fn test_parse_patch_modify() {
        let patch = "*** Begin Patch\n*** Update File: src/lib.rs\n+added line\n-removed line\n*** End Patch";
        let ops = parse_patch_content(patch);
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].operation, "modify");
        assert!(ops[0].diff_summary.as_ref().unwrap().contains("+1"));
    }

    #[test]
    fn test_is_file_modifying() {
        assert_eq!(is_file_modifying_command("git commit -m 'test'"), Some("git"));
        assert_eq!(is_file_modifying_command("rm -rf node_modules"), Some("fileops"));
        assert_eq!(is_file_modifying_command("npm install lodash"), Some("package"));
        assert_eq!(is_file_modifying_command("ls -la"), None);
        assert_eq!(is_file_modifying_command("cat file.txt"), None);
    }
}
