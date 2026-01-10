//! Diachron Core - Shared types and database operations
//!
//! This crate provides:
//! - Common data structures (events, exchanges, embeddings)
//! - Database schema and migrations
//! - Query builders for events and conversations
//! - IPC client for daemon communication
//! - Vector index for semantic search

pub mod error;
pub mod ipc;
pub mod schema;
pub mod types;
pub mod vector;

pub use error::Error;
pub use ipc::{is_daemon_running, send_to_daemon, IpcClient, IpcError};
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
