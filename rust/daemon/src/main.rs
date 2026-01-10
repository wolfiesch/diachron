//! Diachron Daemon (diachrond)
//!
//! Long-running service providing:
//! - Event capture (code changes)
//! - Conversation memory indexing
//! - Semantic search (vector + FTS)
//!
//! Architecture:
//! - Unix socket listener at ~/.diachron/diachron.sock
//! - JSON-RPC style messages (IpcMessage/IpcResponse)
//! - Keeps ONNX model hot in memory (Phase 2)

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{info, warn};

mod db;
mod handlers;
mod server;

pub use db::Database;
use diachron_core::{IpcMessage, IpcResponse};

/// Global state for the daemon
pub struct DaemonState {
    /// When the daemon started
    start_time: Instant,

    /// Total events captured this session
    events_count: AtomicU64,

    /// Shutdown signal
    shutdown: AtomicBool,

    /// Path to the global diachron directory
    diachron_home: PathBuf,

    /// Database handle
    pub db: Database,
}

impl DaemonState {
    pub fn new() -> anyhow::Result<Self> {
        let diachron_home = dirs::home_dir()
            .map(|h| h.join(".diachron"))
            .unwrap_or_else(|| PathBuf::from("/tmp/.diachron"));

        // Ensure directory exists
        std::fs::create_dir_all(&diachron_home)?;

        // Open database
        let db_path = diachron_home.join("diachron.db");
        let db = Database::open(db_path)?;

        Ok(Self {
            start_time: Instant::now(),
            events_count: AtomicU64::new(0),
            shutdown: AtomicBool::new(false),
            diachron_home,
            db,
        })
    }

    pub fn uptime_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    pub fn events_count(&self) -> u64 {
        self.events_count.load(Ordering::Relaxed)
    }

    pub fn increment_events(&self) {
        self.events_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn should_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::Relaxed)
    }

    pub fn request_shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }

    pub fn socket_path(&self) -> PathBuf {
        self.diachron_home.join("diachron.sock")
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("diachrond=info".parse()?)
        )
        .init();

    info!("Starting diachrond v{}", env!("CARGO_PKG_VERSION"));

    let state = Arc::new(DaemonState::new()?);

    // Remove stale socket
    let socket_path = state.socket_path();
    if socket_path.exists() {
        std::fs::remove_file(&socket_path)?;
    }

    // Start the server
    server::run(state).await
}

/// Handle a single client connection
async fn handle_client(
    mut stream: tokio::net::UnixStream,
    state: Arc<DaemonState>,
) -> Result<()> {
    let (reader, mut writer) = stream.split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    while reader.read_line(&mut line).await? > 0 {
        let response = match serde_json::from_str::<IpcMessage>(&line) {
            Ok(msg) => handlers::handle_message(msg, &state).await,
            Err(e) => {
                warn!("Invalid message: {}", e);
                IpcResponse::Error(format!("Invalid message: {}", e))
            }
        };

        let response_json = serde_json::to_string(&response)? + "\n";
        writer.write_all(response_json.as_bytes()).await?;

        line.clear();
    }

    Ok(())
}
