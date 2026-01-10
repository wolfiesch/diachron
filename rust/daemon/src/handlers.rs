//! Message handlers for the daemon

use std::sync::Arc;

use tracing::{debug, error, info};

use diachron_core::{IpcMessage, IpcResponse};

use crate::DaemonState;

/// Handle an incoming IPC message
pub async fn handle_message(msg: IpcMessage, state: &Arc<DaemonState>) -> IpcResponse {
    match msg {
        IpcMessage::Ping => {
            debug!("Ping received");

            // Get actual event count from database
            let events_count = state.db.event_count().unwrap_or(state.events_count());

            IpcResponse::Pong {
                uptime_secs: state.uptime_secs(),
                events_count,
            }
        }

        IpcMessage::Shutdown => {
            info!("Shutdown requested via IPC");
            state.request_shutdown();
            IpcResponse::Ok
        }

        IpcMessage::Capture(event) => {
            debug!("Capture event: {:?}", event.tool_name);

            // Save to database
            match state.db.save_event(&event, None, None) {
                Ok(id) => {
                    debug!("Saved event with id: {}", id);
                    state.increment_events();
                    IpcResponse::Ok
                }
                Err(e) => {
                    error!("Failed to save event: {}", e);
                    IpcResponse::Error(format!("Database error: {}", e))
                }
            }
        }

        IpcMessage::Search { query, limit, source_filter } => {
            debug!("Search: {} (limit: {}, filter: {:?})", query, limit, source_filter);

            // TODO: Phase 2 - Implement vector + FTS hybrid search
            // For now, just return empty results

            IpcResponse::SearchResults(vec![])
        }

        IpcMessage::Timeline { since, file_filter, limit } => {
            debug!("Timeline: since={:?}, file={:?}, limit={}", since, file_filter, limit);

            // Query events from database
            match state.db.query_events(
                since.as_deref(),
                file_filter.as_deref(),
                limit,
            ) {
                Ok(events) => {
                    debug!("Found {} events", events.len());
                    IpcResponse::Events(events)
                }
                Err(e) => {
                    error!("Failed to query events: {}", e);
                    IpcResponse::Error(format!("Database error: {}", e))
                }
            }
        }

        IpcMessage::IndexConversations => {
            info!("Index conversations requested");

            // TODO: Phase 3 - Parse JSONL archives, generate embeddings

            IpcResponse::Ok
        }
    }
}
