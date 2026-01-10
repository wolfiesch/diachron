//! Diachron Core - Shared types and database operations
//!
//! This crate provides:
//! - Common data structures (events, exchanges, embeddings)
//! - Database schema and migrations
//! - Query builders for events and conversations
//! - IPC client for daemon communication

pub mod error;
pub mod ipc;
pub mod schema;
pub mod types;

pub use error::Error;
pub use ipc::{send_to_daemon, is_daemon_running, IpcClient, IpcError};
pub use types::*;

/// Re-export commonly used items
pub mod prelude {
    pub use crate::error::Error;
    pub use crate::ipc::{send_to_daemon, is_daemon_running, IpcClient, IpcError};
    pub use crate::types::*;
}
