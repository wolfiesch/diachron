//! Background tasks for the daemon
//!
//! Runs periodic operations like:
//! - Indexing new conversations
//! - Index maintenance

use std::sync::Arc;
use std::time::Duration;

use tokio::time::interval;
use tracing::{debug, info, warn};

use crate::indexer;
use crate::DaemonState;

/// Default interval between background index checks (30 minutes)
const DEFAULT_INDEX_INTERVAL_MINS: u64 = 30;

/// Run the background indexing task
///
/// This task periodically checks for new conversation archives
/// and indexes them incrementally.
pub async fn background_indexing_task(state: Arc<DaemonState>) {
    // Load config interval (or use default)
    let interval_mins = DEFAULT_INDEX_INTERVAL_MINS;
    let mut ticker = interval(Duration::from_secs(interval_mins * 60));

    info!(
        "Background indexing task started (interval: {} mins)",
        interval_mins
    );

    loop {
        // Wait for next tick
        ticker.tick().await;

        // Check for shutdown
        if state.should_shutdown() {
            info!("Background indexing task stopping due to shutdown");
            break;
        }

        debug!("Running background index check...");

        // Run incremental indexing
        match run_incremental_index(&state).await {
            Ok(indexed) => {
                if indexed > 0 {
                    info!("Background indexed {} new exchanges", indexed);
                } else {
                    debug!("No new exchanges to index");
                }
            }
            Err(e) => {
                warn!("Background indexing error: {}", e);
            }
        }
    }
}

/// Run incremental indexing (returns count of new exchanges indexed)
async fn run_incremental_index(state: &DaemonState) -> anyhow::Result<u64> {
    // Get Claude archives directory
    let claude_dir = match dirs::home_dir() {
        Some(home) => home.join(".claude"),
        None => return Ok(0),
    };

    if !claude_dir.exists() {
        return Ok(0);
    }

    // Load index state
    let state_path = state.diachron_home.join("index_state.json");
    let mut index_state = indexer::IndexState::load(&state_path);

    // Discover archives
    let archives = indexer::discover_archives(&claude_dir);
    if archives.is_empty() {
        return Ok(0);
    }

    let mut total_indexed: u64 = 0;

    for archive_path in archives {
        // Check shutdown between archives
        if state.should_shutdown() {
            break;
        }

        let path_str = archive_path.to_string_lossy().to_string();
        let mtime = std::fs::metadata(&archive_path)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        // Skip unchanged archives
        if let Some(prev) = index_state.archives.get(&path_str) {
            if prev.mtime >= mtime {
                continue;
            }
        }

        // Parse new exchanges from archive
        let start_line = index_state
            .archives
            .get(&path_str)
            .map(|p| p.last_line.saturating_add(1))
            .unwrap_or(0);

        let exchanges = match indexer::parse_archive(&archive_path, start_line) {
            Ok(ex) => ex,
            Err(e) => {
                warn!("Failed to parse archive {}: {}", path_str, e);
                continue;
            }
        };

        // Index each exchange
        for exchange in &exchanges {
            // Generate embedding
            let embed_text = indexer::build_exchange_embed_text(exchange);
            let embedding = if let Ok(mut engine_guard) = state.embedding_engine.write() {
                engine_guard.as_mut().and_then(|e| e.embed(&embed_text).ok())
            } else {
                None
            };

            // Save to database
            if let Err(e) = state.db.save_exchange(exchange, embedding.as_deref()) {
                warn!("Failed to save exchange: {}", e);
                continue;
            }

            // Add to vector index
            if let Some(ref emb) = embedding {
                if let Ok(mut idx) = state.exchanges_index.write() {
                    let _ = idx.add(&format!("exchange:{}", exchange.id), emb);
                }
            }

            total_indexed += 1;
        }

        // Update checkpoint
        if let Some(last_exchange) = exchanges.last() {
            index_state.archives.insert(
                path_str,
                indexer::ArchiveState {
                    last_line: last_exchange.line_end.unwrap_or(0) as u64,
                    mtime,
                },
            );
        }
    }

    // Save index state if we indexed anything
    if total_indexed > 0 {
        if let Err(e) = index_state.save(&state_path) {
            warn!("Failed to save index state: {}", e);
        }
    }

    Ok(total_indexed)
}
