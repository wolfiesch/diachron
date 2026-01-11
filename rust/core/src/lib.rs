//! Diachron Core - Shared types and database operations
//!
//! This crate provides:
//! - Common data structures (events, exchanges, embeddings)
//! - Database schema and migrations
//! - Query builders for events and conversations
//! - IPC client for daemon communication
//! - Vector index for semantic search

pub mod error;
pub mod evidence_pack;
pub mod fingerprint;
pub mod hash_chain;
pub mod ipc;
pub mod pr_correlation;
pub mod schema;
pub mod types;
pub mod vector;

pub use error::Error;
pub use evidence_pack::{
    export_json, generate_evidence_pack, render_markdown_narrative, EvidencePack,
    VerificationStatus, DIACHRON_VERSION,
};
pub use fingerprint::{
    compute_fingerprint, cosine_similarity, extract_context, format_fingerprint, match_fingerprint,
    FingerprintMatch, HunkFingerprint, MatchConfidence, MatchType, DEFAULT_CONTEXT_LINES,
    DEFAULT_SIMILARITY_THRESHOLD,
};
pub use hash_chain::{
    compute_event_hash, create_checkpoint, format_hash, format_hash_short, get_last_event_hash,
    verify_chain, ChainBreak, ChainCheckpoint, ChainVerificationResult, EventHashInput,
    GENESIS_HASH,
};
pub use ipc::{is_daemon_running, send_to_daemon, IpcClient, IpcError};
pub use pr_correlation::{
    correlate_events_to_pr, CommitEvidence, MatchConfidence as PRMatchConfidence, PREvidence,
    PRSummary, DEFAULT_TIME_WINDOW_SECS,
};
pub use schema::{fts_search_events, fts_search_exchanges, init_schema, FtsSearchResult};
pub use types::*;
pub use vector::{VectorError, VectorIndex, VectorSearchResult, EMBEDDING_DIM};

/// Re-export commonly used items
pub mod prelude {
    pub use crate::error::Error;
    pub use crate::ipc::{is_daemon_running, send_to_daemon, IpcClient, IpcError};
    pub use crate::types::*;
    pub use crate::vector::{VectorError, VectorIndex, VectorSearchResult};
}
