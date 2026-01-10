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
//! - Keeps ONNX model hot in memory for fast embeddings

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Instant;

use anyhow::Result;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{info, warn};

mod background;
mod db;
mod handlers;
mod indexer;
mod server;
mod summarization;

pub use db::Database;
use diachron_core::{IpcMessage, IpcResponse, VectorIndex, EMBEDDING_DIM};
use diachron_embeddings::EmbeddingEngine;
use summarization::Summarizer;

/// Global state for the daemon.
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

    /// Embedding engine (loaded lazily, may be None if model not available)
    pub embedding_engine: RwLock<Option<EmbeddingEngine>>,

    /// Vector index for events
    pub events_index: RwLock<VectorIndex>,

    /// Vector index for exchanges (conversations)
    pub exchanges_index: RwLock<VectorIndex>,

    /// Summarizer for conversation exchanges (optional)
    pub summarizer: Option<Summarizer>,
}

impl DaemonState {
    /// Create a new daemon state instance.
    ///
    /// # Errors
    /// Returns `anyhow::Error` if directories, database, or indexes fail to load.
    pub fn new() -> anyhow::Result<Self> {
        let diachron_home = dirs::home_dir()
            .map(|h| h.join(".diachron"))
            .unwrap_or_else(|| PathBuf::from("/tmp/.diachron"));

        // Ensure directories exist
        std::fs::create_dir_all(&diachron_home)?;
        std::fs::create_dir_all(diachron_home.join("indexes"))?;

        // Open database
        let db_path = diachron_home.join("diachron.db");
        let db = Database::open(db_path)?;

        // Try to load embedding engine (may fail if model not downloaded)
        let embedding_engine = match EmbeddingEngine::new_default() {
            Ok(engine) => {
                info!("Embedding engine loaded successfully");
                Some(engine)
            }
            Err(e) => {
                warn!(
                    "Failed to load embedding engine: {}. Semantic search will be unavailable.",
                    e
                );
                warn!("Run 'diachron download-model' to download the embedding model.");
                None
            }
        };

        // Load or create vector indexes
        let events_index_path = diachron_home.join("indexes").join("events");
        let events_index = if VectorIndex::exists(&events_index_path) {
            match VectorIndex::load(&events_index_path) {
                Ok(idx) => {
                    info!("Loaded events vector index ({} vectors)", idx.len());
                    idx
                }
                Err(e) => {
                    warn!("Failed to load events index, creating new: {}", e);
                    VectorIndex::new(EMBEDDING_DIM)?
                }
            }
        } else {
            info!("Creating new events vector index");
            VectorIndex::new(EMBEDDING_DIM)?
        };

        let exchanges_index_path = diachron_home.join("indexes").join("exchanges");
        let exchanges_index = if VectorIndex::exists(&exchanges_index_path) {
            match VectorIndex::load(&exchanges_index_path) {
                Ok(idx) => {
                    info!("Loaded exchanges vector index ({} vectors)", idx.len());
                    idx
                }
                Err(e) => {
                    warn!("Failed to load exchanges index, creating new: {}", e);
                    VectorIndex::new(EMBEDDING_DIM)?
                }
            }
        } else {
            info!("Creating new exchanges vector index");
            VectorIndex::new(EMBEDDING_DIM)?
        };

        // Initialize summarizer (optional - depends on API key availability)
        let summarizer = Summarizer::new(&diachron_home);
        if summarizer.is_available() {
            info!("Summarizer available (API key found)");
        } else {
            info!("Summarizer unavailable (no API key). Set ANTHROPIC_API_KEY to enable.");
        }

        Ok(Self {
            start_time: Instant::now(),
            events_count: AtomicU64::new(0),
            shutdown: AtomicBool::new(false),
            diachron_home,
            db,
            embedding_engine: RwLock::new(embedding_engine),
            events_index: RwLock::new(events_index),
            exchanges_index: RwLock::new(exchanges_index),
            summarizer: if summarizer.is_available() { Some(summarizer) } else { None },
        })
    }

    /// Get uptime in seconds.
    ///
    /// # Returns
    /// Number of seconds since daemon start.
    pub fn uptime_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// Get the number of events captured in this session.
    ///
    /// # Returns
    /// Total events captured since startup.
    pub fn events_count(&self) -> u64 {
        self.events_count.load(Ordering::Relaxed)
    }

    /// Increment the in-memory event counter.
    pub fn increment_events(&self) {
        self.events_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Check whether a shutdown has been requested.
    ///
    /// # Returns
    /// True if shutdown has been requested.
    pub fn should_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::Relaxed)
    }

    /// Request a graceful shutdown.
    pub fn request_shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }

    /// Get the path to the daemon socket.
    ///
    /// # Returns
    /// Path to the Unix socket used by the daemon.
    pub fn socket_path(&self) -> PathBuf {
        self.diachron_home.join("diachron.sock")
    }

    /// Get the path to the indexes directory.
    ///
    /// # Returns
    /// Path to the vector index directory.
    pub fn indexes_path(&self) -> PathBuf {
        self.diachron_home.join("indexes")
    }

    /// Save vector indexes to disk.
    ///
    /// # Errors
    /// Returns `anyhow::Error` if index persistence fails.
    pub fn save_indexes(&self) -> anyhow::Result<()> {
        let indexes_path = self.indexes_path();

        // Save events index
        if let Ok(idx) = self.events_index.read() {
            if idx.len() > 0 {
                idx.save(&indexes_path.join("events"))?;
                info!("Saved events index ({} vectors)", idx.len());
            }
        }

        // Save exchanges index
        if let Ok(idx) = self.exchanges_index.read() {
            if idx.len() > 0 {
                idx.save(&indexes_path.join("exchanges"))?;
                info!("Saved exchanges index ({} vectors)", idx.len());
            }
        }

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("diachrond=info".parse()?),
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
async fn handle_client(mut stream: tokio::net::UnixStream, state: Arc<DaemonState>) -> Result<()> {
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
