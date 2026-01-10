//! Core data types for Diachron
//!
//! These types are shared between the hook, daemon, and CLI.

use serde::{Deserialize, Serialize};

/// Operations that can be performed on files
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Operation {
    Create,
    Modify,
    Delete,
    Move,
    Copy,
    Commit,
    Execute,
    Unknown,
}

impl Operation {
    pub fn as_str(&self) -> &'static str {
        match self {
            Operation::Create => "create",
            Operation::Modify => "modify",
            Operation::Delete => "delete",
            Operation::Move => "move",
            Operation::Copy => "copy",
            Operation::Commit => "commit",
            Operation::Execute => "execute",
            Operation::Unknown => "unknown",
        }
    }
}

/// Semantic command categories for Bash commands
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CommandCategory {
    Git,
    Test,
    Build,
    Deploy,
    FileOps,
    Package,
    Unknown,
}

impl CommandCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            CommandCategory::Git => "git",
            CommandCategory::Test => "test",
            CommandCategory::Build => "build",
            CommandCategory::Deploy => "deploy",
            CommandCategory::FileOps => "file_ops",
            CommandCategory::Package => "package",
            CommandCategory::Unknown => "unknown",
        }
    }
}

/// A captured code change event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureEvent {
    pub tool_name: String,
    pub file_path: Option<String>,
    pub operation: Operation,
    pub diff_summary: Option<String>,
    pub raw_input: Option<String>,
    pub metadata: Option<String>,
    pub git_commit_sha: Option<String>,
    pub command_category: Option<CommandCategory>,
}

/// A conversation exchange (for memory)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Exchange {
    pub id: String,
    pub timestamp: String,
    pub project: Option<String>,
    pub session_id: Option<String>,
    pub user_message: String,
    pub assistant_message: String,
    pub tool_calls: Option<String>,
    pub archive_path: Option<String>,
    pub line_start: Option<i64>,
    pub line_end: Option<i64>,
    pub embedding: Option<Vec<f32>>,
    pub summary: Option<String>,
    pub git_branch: Option<String>,
    pub cwd: Option<String>,
}

/// Search result from vector or text search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub score: f32,
    pub source: SearchSource,
    pub snippet: String,
    pub timestamp: String,
    pub project: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchSource {
    Event,
    Exchange,
}

/// IPC message between CLI and daemon
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum IpcMessage {
    /// Capture a code change event
    Capture(CaptureEvent),

    /// Search for similar content
    Search {
        query: String,
        limit: usize,
        source_filter: Option<SearchSource>,
    },

    /// Get timeline events
    Timeline {
        since: Option<String>,
        file_filter: Option<String>,
        limit: usize,
    },

    /// Index pending conversations
    IndexConversations,

    /// Health check
    Ping,

    /// Shutdown daemon
    Shutdown,
}

/// Response from daemon
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum IpcResponse {
    Ok,
    Error(String),
    SearchResults(Vec<SearchResult>),
    Events(Vec<StoredEvent>),
    Pong { uptime_secs: u64, events_count: u64 },
}

/// Event as stored in database (with ID and timestamps)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredEvent {
    pub id: i64,
    pub timestamp: String,
    pub timestamp_display: Option<String>,
    pub session_id: Option<String>,
    pub tool_name: String,
    pub file_path: Option<String>,
    pub operation: Option<String>,
    pub diff_summary: Option<String>,
    pub raw_input: Option<String>,
    pub ai_summary: Option<String>,
    pub git_commit_sha: Option<String>,
    pub metadata: Option<String>,
}
