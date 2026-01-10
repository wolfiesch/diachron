//! JSONL Archive Parser for Claude Code Conversations
//!
//! Parses Claude Code's conversation archives (~/.claude/projects/*/*.jsonl)
//! and extracts exchanges for indexing.

use std::collections::HashMap;
use std::fs::{self, File};
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use twox_hash::XxHash64;

use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use diachron_core::Exchange;

/// Raw JSONL message from Claude Code archive
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawMessage {
    /// Message type: "user", "assistant", or "queue-operation"
    #[serde(rename = "type")]
    pub msg_type: String,

    /// Message content wrapper
    pub message: Option<MessageContent>,

    /// ISO timestamp
    pub timestamp: Option<String>,

    /// Session identifier
    pub session_id: Option<String>,

    /// Current working directory
    pub cwd: Option<String>,

    /// Git branch name
    pub git_branch: Option<String>,

    /// Unique message ID
    pub uuid: Option<String>,
}

/// Message content structure
#[derive(Debug, Deserialize)]
pub struct MessageContent {
    /// "user" or "assistant"
    pub role: String,

    /// Either a string or an array of content blocks
    pub content: serde_json::Value,
}

/// Content block types in assistant messages
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },

    #[serde(rename = "thinking")]
    Thinking { thinking: String },

    #[serde(rename = "tool_use")]
    ToolUse {
        name: String,
        #[serde(default)]
        input: serde_json::Value,
    },

    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        #[serde(default)]
        content: serde_json::Value,
    },

    #[serde(other)]
    Unknown,
}

/// Index state for incremental processing
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct IndexState {
    /// Map from archive path to its state
    pub archives: HashMap<String, ArchiveState>,
}

/// State for a single archive
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveState {
    /// Last line number indexed (0-based)
    pub last_line: u64,
    /// File modification time (unix timestamp)
    pub mtime: u64,
}

impl IndexState {
    /// Load index state from disk
    pub fn load(path: &Path) -> Self {
        match fs::read_to_string(path) {
            Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Save index state to disk
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        let contents = serde_json::to_string_pretty(self)?;
        fs::write(path, contents)?;
        Ok(())
    }
}

/// Discover all JSONL archives in the Claude projects directory
pub fn discover_archives(claude_dir: &Path) -> Vec<PathBuf> {
    let projects_dir = claude_dir.join("projects");
    let mut archives = Vec::new();

    if let Ok(entries) = fs::read_dir(&projects_dir) {
        for entry_result in entries {
            let entry = match entry_result {
                Ok(e) => e,
                Err(e) => {
                    warn!("Failed to read project directory entry: {}", e);
                    continue;
                }
            };
            let project_path = entry.path();
            if project_path.is_dir() {
                // Look for .jsonl files in each project directory
                if let Ok(files) = fs::read_dir(&project_path) {
                    for file_result in files {
                        let file = match file_result {
                            Ok(f) => f,
                            Err(e) => {
                                warn!(
                                    "Failed to read file entry in {}: {}",
                                    project_path.display(),
                                    e
                                );
                                continue;
                            }
                        };
                        let file_path = file.path();
                        if file_path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                            archives.push(file_path);
                        }
                    }
                }
            }
        }
    }

    debug!("Discovered {} JSONL archives", archives.len());
    archives
}

/// Extract project name from archive path
fn extract_project_name(archive_path: &Path) -> String {
    archive_path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string()
}

/// Generate a unique exchange ID
///
/// Uses XxHash64 with fixed seed for stable hashing across Rust versions.
/// DefaultHasher is not guaranteed to be stable between Rust releases.
fn generate_exchange_id(project: &str, timestamp: &str, user_prefix: &str) -> String {
    let mut hasher = XxHash64::with_seed(0);
    project.hash(&mut hasher);
    timestamp.hash(&mut hasher);
    user_prefix.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Extract text from content (handles both string and array formats)
fn extract_text_content(content: &serde_json::Value) -> String {
    match content {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(blocks) => {
            let mut texts = Vec::new();
            for block in blocks {
                if let Ok(cb) = serde_json::from_value::<ContentBlock>(block.clone()) {
                    match cb {
                        ContentBlock::Text { text } => texts.push(text),
                        ContentBlock::Thinking { thinking: _ } => {
                            // Skip thinking blocks (internal reasoning)
                        }
                        ContentBlock::ToolUse { name, .. } => {
                            texts.push(format!("[Tool: {}]", name));
                        }
                        ContentBlock::ToolResult { content, .. } => {
                            if let Some(s) = content.as_str() {
                                // Truncate tool results as they can be very long
                                // Use safe_truncate to handle UTF-8 char boundaries
                                let truncated = if s.len() > 200 {
                                    format!("{}...", safe_truncate(s, 200))
                                } else {
                                    s.to_string()
                                };
                                texts.push(format!("[Result: {}]", truncated));
                            }
                        }
                        ContentBlock::Unknown => {}
                    }
                }
            }
            texts.join("\n")
        }
        _ => String::new(),
    }
}

/// Extract tool names from assistant content
fn extract_tool_calls(content: &serde_json::Value) -> Option<String> {
    if let serde_json::Value::Array(blocks) = content {
        let tool_names: Vec<String> = blocks
            .iter()
            .filter_map(|block| {
                if let Ok(ContentBlock::ToolUse { name, .. }) =
                    serde_json::from_value::<ContentBlock>(block.clone())
                {
                    Some(name)
                } else {
                    None
                }
            })
            .collect();

        if tool_names.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&tool_names).unwrap_or_default())
        }
    } else {
        None
    }
}

/// Parse a single JSONL archive file
///
/// Returns exchanges found starting from `start_line`.
pub fn parse_archive(
    archive_path: &Path,
    start_line: u64,
) -> anyhow::Result<Vec<Exchange>> {
    let file = File::open(archive_path)?;
    let reader = BufReader::new(file);

    let project = extract_project_name(archive_path);
    let archive_path_str = archive_path.to_string_lossy().to_string();

    let mut exchanges = Vec::new();
    let mut pending_user: Option<(u64, RawMessage)> = None;

    for (line_idx, line_result) in reader.lines().enumerate() {
        let line_num = line_idx as u64;

        // Skip already-processed lines
        if line_num < start_line {
            continue;
        }

        let line = match line_result {
            Ok(l) => l,
            Err(e) => {
                warn!("Failed to read line {} in {}: {}", line_num, archive_path_str, e);
                continue;
            }
        };

        // Skip empty lines
        if line.trim().is_empty() {
            continue;
        }

        // Parse JSON
        let msg: RawMessage = match serde_json::from_str(&line) {
            Ok(m) => m,
            Err(e) => {
                // Skip malformed lines silently (common in archives)
                debug!("Skipping malformed JSON at line {}: {}", line_num, e);
                continue;
            }
        };

        // Only process user/assistant messages
        match msg.msg_type.as_str() {
            "user" => {
                // Store pending user message
                pending_user = Some((line_num, msg));
            }
            "assistant" => {
                // Pair with pending user message if available
                if let Some((user_line, user_msg)) = pending_user.take() {
                    if let (Some(user_content), Some(assistant_content)) =
                        (&user_msg.message, &msg.message)
                    {
                        let user_text = extract_text_content(&user_content.content);
                        let assistant_text = extract_text_content(&assistant_content.content);

                        // Skip if both are empty (keep exchanges with at least one side)
                        if user_text.is_empty() && assistant_text.is_empty() {
                            continue;
                        }

                        // Use assistant timestamp (more accurate for response time)
                        let timestamp = msg
                            .timestamp
                            .clone()
                            .or(user_msg.timestamp.clone())
                            .unwrap_or_default();

                        // Generate ID from project + timestamp + user message prefix
                        // Use safe_truncate to handle UTF-8 char boundaries
                        let user_prefix = safe_truncate(&user_text, 100);
                        let id = generate_exchange_id(&project, &timestamp, user_prefix);

                        let exchange = Exchange {
                            id,
                            timestamp,
                            project: Some(project.clone()),
                            session_id: msg.session_id.clone().or(user_msg.session_id.clone()),
                            user_message: user_text,
                            assistant_message: assistant_text,
                            tool_calls: extract_tool_calls(&assistant_content.content),
                            archive_path: Some(archive_path_str.clone()),
                            line_start: Some(user_line as i64),
                            line_end: Some(line_num as i64),
                            embedding: None, // Will be generated later
                            summary: None,   // Optional, not implemented yet
                            git_branch: msg.git_branch.clone().or(user_msg.git_branch.clone()),
                            cwd: msg.cwd.clone().or(user_msg.cwd.clone()),
                        };

                        exchanges.push(exchange);
                    }
                }
                // If no pending user, this is an orphan assistant message - skip
            }
            _ => {
                // Skip queue-operation and other types
            }
        }
    }

    debug!(
        "Parsed {} exchanges from {} (starting at line {})",
        exchanges.len(),
        archive_path_str,
        start_line
    );

    Ok(exchanges)
}

/// Safely truncate a string at a character boundary
///
/// UTF-8 strings can't be sliced at arbitrary byte positions - this function
/// finds the nearest valid character boundary at or before the target length.
pub fn safe_truncate(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    // Find the last valid character boundary at or before max_bytes
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Build embed text from an exchange for vector embedding
///
/// Combines user and assistant messages, truncating to stay within
/// the embedding model's context limit (~2000 chars).
pub fn build_exchange_embed_text(exchange: &Exchange) -> String {
    // Truncate each to ~1000 chars for 2000 total
    let user_truncated = if exchange.user_message.len() > 1000 {
        format!("{}...", safe_truncate(&exchange.user_message, 1000))
    } else {
        exchange.user_message.clone()
    };

    let assistant_truncated = if exchange.assistant_message.len() > 900 {
        format!("{}...", safe_truncate(&exchange.assistant_message, 900))
    } else {
        exchange.assistant_message.clone()
    };

    format!(
        "User: {}\nAssistant: {}",
        user_truncated, assistant_truncated
    )
}

/// Get modification time of a file as unix timestamp
pub fn get_mtime(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .map(|t| t.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_text_from_string() {
        let content = serde_json::json!("Hello, world!");
        assert_eq!(extract_text_content(&content), "Hello, world!");
    }

    #[test]
    fn test_extract_text_from_array() {
        let content = serde_json::json!([
            {"type": "text", "text": "First part"},
            {"type": "text", "text": "Second part"}
        ]);
        let text = extract_text_content(&content);
        assert!(text.contains("First part"));
        assert!(text.contains("Second part"));
    }

    #[test]
    fn test_generate_exchange_id() {
        let id1 = generate_exchange_id("project", "2026-01-01T00:00:00Z", "hello");
        let id2 = generate_exchange_id("project", "2026-01-01T00:00:00Z", "hello");
        let id3 = generate_exchange_id("project", "2026-01-01T00:00:01Z", "hello");

        // Same inputs should produce same ID
        assert_eq!(id1, id2);
        // Different inputs should produce different ID
        assert_ne!(id1, id3);
        // ID should be 16 hex chars
        assert_eq!(id1.len(), 16);
    }

    #[test]
    fn test_build_exchange_embed_text() {
        let exchange = Exchange {
            id: "test".to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            project: Some("test-project".to_string()),
            session_id: None,
            user_message: "How do I implement auth?".to_string(),
            assistant_message: "You can use OAuth2 or JWT...".to_string(),
            tool_calls: None,
            archive_path: None,
            line_start: None,
            line_end: None,
            embedding: None,
            summary: None,
            git_branch: None,
            cwd: None,
        };

        let text = build_exchange_embed_text(&exchange);
        assert!(text.contains("User: How do I implement auth?"));
        assert!(text.contains("Assistant: You can use OAuth2"));
    }

    #[test]
    fn test_index_state_roundtrip() {
        let mut state = IndexState::default();
        state.archives.insert(
            "/path/to/archive.jsonl".to_string(),
            ArchiveState {
                last_line: 100,
                mtime: 1704067200,
            },
        );

        let json = serde_json::to_string(&state).unwrap();
        let loaded: IndexState = serde_json::from_str(&json).unwrap();

        assert_eq!(loaded.archives.len(), 1);
        assert_eq!(loaded.archives["/path/to/archive.jsonl"].last_line, 100);
    }

    #[test]
    fn test_safe_truncate_utf8() {
        // ASCII only - should truncate at exact byte
        let ascii = "Hello, world!";
        assert_eq!(safe_truncate(ascii, 5), "Hello");

        // Multi-byte UTF-8 characters (→ is 3 bytes)
        let with_arrows = "test→test→test";
        // "test" = 4 bytes, "→" = 3 bytes, so "test→" = 7 bytes
        // Truncating at 6 should back up to 4 (before the arrow)
        assert_eq!(safe_truncate(with_arrows, 6), "test");
        // Truncating at 7 should include the full arrow
        assert_eq!(safe_truncate(with_arrows, 7), "test→");

        // String shorter than max should return unchanged
        assert_eq!(safe_truncate("short", 1000), "short");

        // Empty string
        assert_eq!(safe_truncate("", 100), "");

        // Chinese characters (each is 3 bytes)
        let chinese = "你好世界";  // 12 bytes total
        assert_eq!(safe_truncate(chinese, 6), "你好");  // 2 chars = 6 bytes
        assert_eq!(safe_truncate(chinese, 5), "你");    // backs up to char boundary
    }
}
