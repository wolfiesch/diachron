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
    /// Return the lowercase string representation used for storage.
    ///
    /// # Returns
    /// String slice for this operation variant.
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
    /// Return the lowercase string representation used for storage.
    ///
    /// # Returns
    /// String slice for this command category.
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

/// A captured code change event.
///
/// # Fields
/// - `tool_name`: Tool name that produced the event (Write, Edit, Bash).
/// - `file_path`: Optional file path affected by the event.
/// - `operation`: Operation type for the change.
/// - `diff_summary`: Short summary of the change.
/// - `raw_input`: Raw tool input or command string.
/// - `metadata`: Optional JSON metadata (branch, category, etc.).
/// - `git_commit_sha`: Optional commit SHA.
/// - `command_category`: Optional semantic category for bash commands.
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

/// A conversation exchange used for memory indexing.
///
/// # Fields
/// - `id`: Stable identifier for the exchange.
/// - `timestamp`: ISO timestamp string.
/// - `project`: Optional project name.
/// - `session_id`: Optional session identifier.
/// - `user_message`: User message text.
/// - `assistant_message`: Assistant message text.
/// - `tool_calls`: Optional JSON array of tool names.
/// - `archive_path`: Optional path to the source archive.
/// - `line_start`: Optional starting line in the archive.
/// - `line_end`: Optional ending line in the archive.
/// - `embedding`: Optional embedding vector.
/// - `summary`: Optional summary text.
/// - `git_branch`: Optional git branch name.
/// - `cwd`: Optional working directory.
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

/// Search result from vector or text search.
///
/// # Fields
/// - `id`: Identifier of the matched item.
/// - `score`: Similarity score (higher is better).
/// - `source`: Search source (event or exchange).
/// - `snippet`: Highlighted snippet for display.
/// - `timestamp`: Timestamp for the matched item.
/// - `project`: Optional project name for context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub score: f32,
    pub source: SearchSource,
    pub snippet: String,
    pub timestamp: String,
    pub project: Option<String>,
}

/// Source of a search result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchSource {
    Event,
    Exchange,
}

/// IPC message between CLI and daemon.
///
/// Messages are serialized to JSON and sent over the Unix socket.
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
        /// Filter by time (e.g., "1h", "7d", "2024-01-01")
        since: Option<String>,
        /// Filter by project name
        project: Option<String>,
    },

    /// Get timeline events
    Timeline {
        since: Option<String>,
        file_filter: Option<String>,
        limit: usize,
    },

    /// Index pending conversations
    IndexConversations,

    /// Get diagnostic information
    DoctorInfo,

    /// Summarize exchanges without summaries
    SummarizeExchanges {
        /// Maximum exchanges to summarize (default: 100)
        limit: usize,
    },

    /// Health check
    Ping,

    /// Shutdown daemon
    Shutdown,

    /// Run database maintenance (VACUUM, ANALYZE, prune old events)
    Maintenance {
        /// Prune events older than this many days (0 = no pruning)
        retention_days: u32,
    },

    /// Blame a specific file line using fingerprint matching
    BlameByFingerprint {
        /// File path being blamed
        file_path: String,
        /// Line number to blame
        line_number: u32,
        /// Current content of the line
        content: String,
        /// Surrounding context (±5 lines)
        context: String,
        /// Blame mode: "strict", "best-effort", or "inferred"
        mode: String,
    },

    /// Correlate events with PR commits and generate evidence pack
    CorrelateEvidence {
        /// Pull request number
        pr_id: u64,
        /// Git commit SHAs in the PR
        commits: Vec<String>,
        /// Branch name
        branch: String,
        /// Start time (ISO timestamp)
        start_time: String,
        /// End time (ISO timestamp)
        end_time: String,
        /// Optional user intent
        intent: Option<String>,
    },
}

/// Response from daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum IpcResponse {
    Ok,
    Error(String),
    SearchResults(Vec<SearchResult>),
    Events(Vec<StoredEvent>),
    Pong {
        uptime_secs: u64,
        events_count: u64,
    },
    /// Result of indexing conversations
    IndexStats {
        exchanges_indexed: u64,
        archives_processed: u64,
        errors: u64,
    },
    /// Diagnostic information
    Doctor(DiagnosticInfo),
    /// Result of summarization
    SummarizeStats {
        summarized: u64,
        skipped: u64,
        errors: u64,
    },
    /// Result of database maintenance
    MaintenanceStats {
        /// Database size before maintenance (bytes)
        size_before: u64,
        /// Database size after maintenance (bytes)
        size_after: u64,
        /// Events pruned (if retention enabled)
        events_pruned: u64,
        /// Exchanges pruned (if retention enabled)
        exchanges_pruned: u64,
        /// Time taken (milliseconds)
        duration_ms: u64,
    },
    /// Result of fingerprint-based blame
    BlameResult(BlameMatch),
    /// No blame match found
    BlameNotFound {
        reason: String,
    },
    /// Result of PR evidence correlation
    EvidenceResult(EvidencePackResult),
}

/// Blame match result from fingerprint lookup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlameMatch {
    /// The matched event
    pub event: StoredEvent,
    /// Confidence level: "high", "medium", "low", "inferred"
    pub confidence: String,
    /// Match type description
    pub match_type: String,
    /// Similarity score (0.0 - 1.0)
    pub similarity: f32,
    /// User intent if available from conversation
    pub intent: Option<String>,
}

/// Evidence pack result from PR correlation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidencePackResult {
    /// Pull request number
    pub pr_id: u64,
    /// When this evidence pack was generated
    pub generated_at: String,
    /// Diachron version
    pub diachron_version: String,
    /// Branch name
    pub branch: String,
    /// Summary statistics
    pub summary: EvidenceSummary,
    /// Evidence grouped by commit
    pub commits: Vec<CommitEvidenceResult>,
    /// Verification status
    pub verification: VerificationStatusResult,
    /// User intent (if provided)
    pub intent: Option<String>,
    /// Coverage percentage (events matched to commits)
    pub coverage_pct: f32,
    /// Number of unmatched events
    pub unmatched_count: usize,
    /// Total events considered
    pub total_events: u64,
}

/// Summary statistics for evidence pack
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceSummary {
    /// Number of files changed
    pub files_changed: usize,
    /// Lines added
    pub lines_added: usize,
    /// Lines removed
    pub lines_removed: usize,
    /// Number of tool operations
    pub tool_operations: usize,
    /// Number of unique sessions
    pub sessions: usize,
}

/// Evidence for a single commit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitEvidenceResult {
    /// Git commit SHA
    pub sha: String,
    /// Commit message (first line)
    pub message: Option<String>,
    /// Events linked to this commit
    pub events: Vec<StoredEvent>,
    /// Confidence level: "HIGH", "MEDIUM", "LOW"
    pub confidence: String,
}

/// Verification status for evidence pack
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationStatusResult {
    /// Hash chain integrity verified
    pub chain_verified: bool,
    /// Tests were executed after changes
    pub tests_executed: bool,
    /// Build succeeded
    pub build_succeeded: bool,
    /// Human has reviewed
    pub human_reviewed: bool,
}

/// Diagnostic information for doctor command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticInfo {
    /// Daemon uptime in seconds
    pub uptime_secs: u64,
    /// Total events in database
    pub events_count: u64,
    /// Total exchanges in database
    pub exchanges_count: u64,
    /// Events vector index count
    pub events_index_count: usize,
    /// Exchanges vector index count
    pub exchanges_index_count: usize,
    /// Database file size in bytes
    pub database_size_bytes: u64,
    /// Events index file size in bytes
    pub events_index_size_bytes: u64,
    /// Exchanges index file size in bytes
    pub exchanges_index_size_bytes: u64,
    /// Whether embedding model is loaded
    pub model_loaded: bool,
    /// Model file size in bytes (0 if not found)
    pub model_size_bytes: u64,
    /// Daemon memory usage in bytes (RSS)
    pub memory_rss_bytes: u64,
}

/// Event as stored in the database (with ID and timestamps).
///
/// # Fields
/// - `id`: Database row ID.
/// - `timestamp`: ISO timestamp string.
/// - `timestamp_display`: Optional human-friendly timestamp.
/// - `session_id`: Optional session identifier.
/// - `tool_name`: Tool name that produced the event.
/// - `file_path`: Optional file path affected by the event.
/// - `operation`: Optional operation string.
/// - `diff_summary`: Optional diff summary.
/// - `raw_input`: Optional raw tool input.
/// - `ai_summary`: Optional AI summary.
/// - `git_commit_sha`: Optional commit SHA.
/// - `metadata`: Optional JSON metadata string.
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
