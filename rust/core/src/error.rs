//! Error types for Diachron

use thiserror::Error;

/// Core error type for Diachron operations.
#[derive(Error, Debug)]
pub enum Error {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Project not initialized: {path}")]
    NotInitialized { path: String },

    #[error("Daemon not running")]
    DaemonNotRunning,

    #[error("IPC error: {0}")]
    Ipc(String),

    #[error("{0}")]
    Other(String),
}

/// Result alias for core operations.
pub type Result<T> = std::result::Result<T, Error>;
