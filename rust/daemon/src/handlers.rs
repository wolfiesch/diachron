//! Message handlers for the daemon

use std::collections::HashSet;
use std::sync::Arc;

use tracing::{debug, error, info, warn};

use diachron_core::{
    fts_search_events, fts_search_exchanges, DiagnosticInfo, IpcMessage, IpcResponse, SearchResult, SearchSource,
};

use crate::indexer::{
    build_exchange_embed_text, discover_archives, get_mtime, parse_archive, safe_truncate,
    ArchiveState, IndexState,
};
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

            // Save vector indexes before shutdown
            if let Err(e) = state.save_indexes() {
                error!("Failed to save indexes on shutdown: {}", e);
            }

            state.request_shutdown();
            IpcResponse::Ok
        }

        IpcMessage::Capture(event) => {
            debug!("Capture event: {:?}", event.tool_name);

            // Build text for embedding from event data
            let embed_text = build_event_embed_text(&event);

            // Try to generate embedding if engine is available
            let embedding = if let Ok(mut engine_guard) = state.embedding_engine.write() {
                if let Some(ref mut engine) = *engine_guard {
                    match engine.embed(&embed_text) {
                        Ok(emb) => Some(emb),
                        Err(e) => {
                            warn!("Failed to generate embedding: {}", e);
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            // Save to database (with embedding if available)
            match state.db.save_event(&event, None, embedding.as_deref()) {
                Ok(id) => {
                    debug!("Saved event with id: {}", id);

                    // Add to vector index if we have an embedding
                    if let Some(ref emb) = embedding {
                        if let Ok(mut idx) = state.events_index.write() {
                            let event_id = format!("event:{}", id);
                            if let Err(e) = idx.add(&event_id, emb) {
                                warn!("Failed to add to vector index: {}", e);
                            } else {
                                debug!("Added event {} to vector index", id);
                            }
                        }
                    }

                    state.increment_events();
                    IpcResponse::Ok
                }
                Err(e) => {
                    error!("Failed to save event: {}", e);
                    IpcResponse::Error(format!("Database error: {}", e))
                }
            }
        }

        IpcMessage::Search {
            query,
            limit,
            source_filter,
            since,
            project,
        } => {
            debug!(
                "Search: {} (limit: {}, filter: {:?}, since: {:?}, project: {:?})",
                query, limit, source_filter, since, project
            );

            let results = hybrid_search(state, &query, limit, source_filter, since.as_deref(), project.as_deref()).await;
            IpcResponse::SearchResults(results)
        }

        IpcMessage::DoctorInfo => {
            debug!("DoctorInfo requested");
            let info = gather_diagnostic_info(state);
            IpcResponse::Doctor(info)
        }

        IpcMessage::Timeline {
            since,
            file_filter,
            limit,
        } => {
            debug!(
                "Timeline: since={:?}, file={:?}, limit={}",
                since, file_filter, limit
            );

            // Query events from database
            match state
                .db
                .query_events(since.as_deref(), file_filter.as_deref(), limit)
            {
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
            info!("Starting conversation indexing...");

            // 1. Discover archives
            let claude_dir = match dirs::home_dir() {
                Some(home) => home.join(".claude"),
                None => {
                    error!("Could not determine home directory for archive discovery");
                    return IpcResponse::Error(
                        "Could not determine home directory for archive discovery".to_string(),
                    );
                }
            };
            let archives = discover_archives(&claude_dir);
            info!("Found {} archives to process", archives.len());

            // 2. Load index state for incremental processing
            let state_path = state.diachron_home.join("index_state.json");
            let mut index_state = IndexState::load(&state_path);

            let mut total_indexed: u64 = 0;
            let mut archives_processed: u64 = 0;
            let mut errors: u64 = 0;

            // 3. Process each archive
            for archive_path in archives {
                let path_str = archive_path.to_string_lossy().to_string();
                let mtime = get_mtime(&archive_path);

                // Check if needs indexing (skip unchanged files)
                // Use saturating_add(1) to start after last processed line (avoid off-by-one)
                let start_line = if let Some(prev) = index_state.archives.get(&path_str) {
                    if prev.mtime >= mtime {
                        debug!("Skipping unchanged archive: {}", path_str);
                        continue; // Skip unchanged
                    }
                    prev.last_line.saturating_add(1)
                } else {
                    0
                };

                // 4. Parse exchanges from archive
                match parse_archive(&archive_path, start_line) {
                    Ok(exchanges) => {
                        if exchanges.is_empty() {
                            continue;
                        }

                        let mut last_line: u64 = start_line;

                        for exchange in &exchanges {
                            // Track last line for checkpoint
                            if let Some(line_end) = exchange.line_end {
                                last_line = last_line.max(line_end as u64);
                            }

                            // 5. Generate embedding
                            let embed_text = build_exchange_embed_text(exchange);
                            let embedding =
                                if let Ok(mut engine_guard) = state.embedding_engine.write() {
                                    if let Some(ref mut engine) = *engine_guard {
                                        match engine.embed(&embed_text) {
                                            Ok(emb) => Some(emb),
                                            Err(e) => {
                                                warn!("Failed to embed exchange: {}", e);
                                                None
                                            }
                                        }
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                };

                            // 6. Save to database
                            if let Err(e) = state.db.save_exchange(exchange, embedding.as_deref()) {
                                warn!("Failed to save exchange {}: {}", exchange.id, e);
                                errors += 1;
                                continue;
                            }

                            // 7. Add to vector index
                            if let Some(ref emb) = embedding {
                                if let Ok(mut idx) = state.exchanges_index.write() {
                                    let exchange_id = format!("exchange:{}", exchange.id);
                                    if let Err(e) = idx.add(&exchange_id, emb) {
                                        warn!("Failed to add to vector index: {}", e);
                                    }
                                }
                            }

                            total_indexed += 1;
                        }

                        // 8. Update checkpoint for this archive
                        index_state
                            .archives
                            .insert(path_str.clone(), ArchiveState { last_line, mtime });
                        archives_processed += 1;

                        debug!("Indexed {} exchanges from {}", exchanges.len(), path_str);
                    }
                    Err(e) => {
                        warn!("Failed to parse {}: {}", path_str, e);
                        errors += 1;
                    }
                }
            }

            // 9. Save index state
            if let Err(e) = index_state.save(&state_path) {
                error!("Failed to save index state: {}", e);
            }

            // 10. Save vector indexes
            if let Err(e) = state.save_indexes() {
                error!("Failed to save vector indexes: {}", e);
            }

            info!(
                "Indexing complete: {} exchanges from {} archives ({} errors)",
                total_indexed, archives_processed, errors
            );

            IpcResponse::IndexStats {
                exchanges_indexed: total_indexed,
                archives_processed,
                errors,
            }
        }

        IpcMessage::SummarizeExchanges { limit } => {
            info!("Starting exchange summarization (limit: {})...", limit);

            // Check if summarizer is available
            let summarizer = match &state.summarizer {
                Some(s) => s,
                None => {
                    return IpcResponse::Error(
                        "Summarization unavailable. Set ANTHROPIC_API_KEY env var or add api_key to ~/.diachron/config.toml".to_string()
                    );
                }
            };

            // Get exchanges without summaries
            let exchanges = match state.db.get_exchanges_without_summary(limit) {
                Ok(e) => e,
                Err(e) => {
                    return IpcResponse::Error(format!("Database error: {}", e));
                }
            };

            if exchanges.is_empty() {
                info!("No exchanges need summarization");
                return IpcResponse::SummarizeStats {
                    summarized: 0,
                    skipped: 0,
                    errors: 0,
                };
            }

            info!("Found {} exchanges to summarize", exchanges.len());

            let mut summarized: u64 = 0;
            let mut skipped: u64 = 0;
            let mut errors: u64 = 0;

            for (id, user_msg, assistant_msg) in exchanges {
                // Skip if messages are too short to be meaningful
                if user_msg.len() < 10 || assistant_msg.len() < 10 {
                    skipped += 1;
                    continue;
                }

                match summarizer.summarize(&user_msg, &assistant_msg) {
                    Ok(summary) => {
                        if let Err(e) = state.db.update_exchange_summary(&id, &summary) {
                            warn!("Failed to save summary for {}: {}", id, e);
                            errors += 1;
                        } else {
                            debug!("Summarized {}: {}", id, &summary[..summary.len().min(50)]);
                            summarized += 1;
                        }
                    }
                    Err(e) => {
                        warn!("Failed to summarize {}: {}", id, e);
                        errors += 1;
                    }
                }
            }

            info!(
                "Summarization complete: {} summarized, {} skipped, {} errors",
                summarized, skipped, errors
            );

            IpcResponse::SummarizeStats {
                summarized,
                skipped,
                errors,
            }
        }
    }
}

/// Build text for embedding from event data
fn build_event_embed_text(event: &diachron_core::CaptureEvent) -> String {
    let mut parts = Vec::new();

    parts.push(format!("Tool: {}", event.tool_name));

    if let Some(ref path) = event.file_path {
        parts.push(format!("File: {}", path));
    }

    parts.push(format!("Operation: {}", event.operation.as_str()));

    if let Some(ref diff) = event.diff_summary {
        parts.push(format!("Changes: {}", diff));
    }

    if let Some(ref raw) = event.raw_input {
        // Truncate raw input to avoid overwhelming the embedding
        // Uses shared safe_truncate for UTF-8 char boundary handling
        let truncated = safe_truncate(raw, 500);
        parts.push(format!("Content: {}", truncated));
    }

    parts.join("\n")
}

/// Perform hybrid search combining vector and FTS results
async fn hybrid_search(
    state: &Arc<DaemonState>,
    query: &str,
    limit: usize,
    source_filter: Option<SearchSource>,
    since: Option<&str>,
    project: Option<&str>,
) -> Vec<SearchResult> {
    // Parse since filter to timestamp if provided
    let since_timestamp = since.and_then(|s| parse_time_filter(s));

    debug!("Hybrid search with since={:?}, project={:?}", since_timestamp, project);
    let mut results = Vec::new();
    let mut seen_ids = HashSet::new();

    // 1. Vector search (semantic) - if embedding engine available
    let query_embedding = if let Ok(mut engine_guard) = state.embedding_engine.write() {
        if let Some(ref mut engine) = *engine_guard {
            match engine.embed(query) {
                Ok(emb) => Some(emb),
                Err(e) => {
                    warn!("Failed to embed query: {}", e);
                    None
                }
            }
        } else {
            None
        }
    } else {
        None
    };

    if let Some(ref emb) = query_embedding {
        // Search events vector index
        if source_filter.is_none() || source_filter == Some(SearchSource::Event) {
            if let Ok(idx) = state.events_index.read() {
                match idx.search(emb, limit) {
                    Ok(vector_results) => {
                        for vr in vector_results {
                            // Extract event ID from "event:123" format
                            if let Some(id_str) = vr.id.strip_prefix("event:") {
                                if seen_ids.insert(format!("event:{}", id_str)) {
                                    results.push(SearchResult {
                                        id: id_str.to_string(),
                                        score: vr.score,
                                        source: SearchSource::Event,
                                        snippet: String::new(), // Will be filled from DB
                                        timestamp: String::new(),
                                        project: None,
                                    });
                                }
                            }
                        }
                    }
                    Err(e) => warn!("Vector search failed: {}", e),
                }
            }
        }

        // Search exchanges vector index
        if source_filter.is_none() || source_filter == Some(SearchSource::Exchange) {
            if let Ok(idx) = state.exchanges_index.read() {
                match idx.search(emb, limit) {
                    Ok(vector_results) => {
                        for vr in vector_results {
                            if let Some(id_str) = vr.id.strip_prefix("exchange:") {
                                if seen_ids.insert(format!("exchange:{}", id_str)) {
                                    results.push(SearchResult {
                                        id: id_str.to_string(),
                                        score: vr.score,
                                        source: SearchSource::Exchange,
                                        snippet: String::new(),
                                        timestamp: String::new(),
                                        project: None,
                                    });
                                }
                            }
                        }
                    }
                    Err(e) => warn!("Vector search failed: {}", e),
                }
            }
        }
    }

    // 2. FTS search (keyword) - use with_conn for thread-safe access
    // Search events FTS
    if source_filter.is_none() || source_filter == Some(SearchSource::Event) {
        // Use with_conn and map the diachron_core::Error to rusqlite::Error
        let fts_result = state.db.with_conn(|conn| {
            fts_search_events(conn, query, limit)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
        });
        match fts_result {
            Ok(fts_results) => {
                for fts in fts_results {
                    let key = format!("event:{}", fts.id);
                    if seen_ids.insert(key) {
                        results.push(SearchResult {
                            id: fts.id,
                            score: (-fts.score) as f32, // BM25 returns negative scores, convert
                            source: SearchSource::Event,
                            snippet: fts.snippet,
                            timestamp: fts.timestamp,
                            project: fts.context, // file_path for events
                        });
                    }
                }
            }
            Err(e) => warn!("FTS events search failed: {}", e),
        }
    }

    // Search exchanges FTS
    if source_filter.is_none() || source_filter == Some(SearchSource::Exchange) {
        let fts_result = state.db.with_conn(|conn| {
            fts_search_exchanges(conn, query, limit)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
        });
        match fts_result {
            Ok(fts_results) => {
                for fts in fts_results {
                    let key = format!("exchange:{}", fts.id);
                    if seen_ids.insert(key) {
                        results.push(SearchResult {
                            id: fts.id,
                            score: (-fts.score) as f32,
                            source: SearchSource::Exchange,
                            snippet: fts.snippet,
                            timestamp: fts.timestamp,
                            project: fts.context, // project for exchanges
                        });
                    }
                }
            }
            Err(e) => warn!("FTS exchanges search failed: {}", e),
        }
    }

    // 3. Filter by since and project
    if since_timestamp.is_some() || project.is_some() {
        results.retain(|r| {
            // Filter by timestamp if since is set
            if let Some(ref since_ts) = since_timestamp {
                if r.timestamp < *since_ts {
                    return false;
                }
            }
            // Filter by project if set
            if let Some(proj) = project {
                if let Some(ref result_proj) = r.project {
                    if !result_proj.to_lowercase().contains(&proj.to_lowercase()) {
                        return false;
                    }
                } else {
                    return false; // No project info, exclude if filtering
                }
            }
            true
        });
    }

    // 4. Sort by score and limit
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.truncate(limit);

    debug!(
        "Hybrid search returned {} results (vector: {}, fts: {})",
        results.len(),
        query_embedding.is_some(),
        true
    );

    results
}

/// Parse a time filter string into an ISO timestamp
/// Supports: "1h", "2d", "7d", "1w", "30d", ISO dates, etc.
fn parse_time_filter(filter: &str) -> Option<String> {
    use chrono::{Duration, Utc};

    let filter = filter.trim().to_lowercase();

    // Handle relative time formats
    let duration = if filter.ends_with('h') {
        filter[..filter.len()-1].parse::<i64>().ok().map(Duration::hours)
    } else if filter.ends_with('d') {
        filter[..filter.len()-1].parse::<i64>().ok().map(Duration::days)
    } else if filter.ends_with('w') {
        filter[..filter.len()-1].parse::<i64>().ok().map(Duration::weeks)
    } else if filter == "yesterday" {
        Some(Duration::days(1))
    } else if filter == "today" {
        Some(Duration::hours(0)) // Current time
    } else {
        None
    };

    if let Some(dur) = duration {
        let since = Utc::now() - dur;
        return Some(since.format("%Y-%m-%dT%H:%M:%SZ").to_string());
    }

    // Try parsing as ISO date directly
    if filter.contains('-') && filter.len() >= 10 {
        // Assume it's already an ISO date, validate format roughly
        if filter.chars().take(10).filter(|c| c.is_ascii_digit() || *c == '-').count() >= 10 {
            return Some(if filter.len() == 10 {
                format!("{}T00:00:00Z", filter)
            } else {
                filter.to_string()
            });
        }
    }

    None
}

/// Gather diagnostic information about the daemon state
fn gather_diagnostic_info(state: &Arc<DaemonState>) -> DiagnosticInfo {
    // Get counts from database
    let events_count = state.db.event_count().unwrap_or(0);
    let exchanges_count = state.db.exchange_count().unwrap_or(0);

    // Get vector index counts
    let events_index_count = state.events_index.read().map(|idx| idx.len()).unwrap_or(0);
    let exchanges_index_count = state.exchanges_index.read().map(|idx| idx.len()).unwrap_or(0);

    // Get file sizes
    let db_path = state.diachron_home.join("diachron.db");
    let database_size_bytes = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);

    let events_index_path = state.diachron_home.join("indexes/events.usearch");
    let events_index_size_bytes = std::fs::metadata(&events_index_path).map(|m| m.len()).unwrap_or(0);

    let exchanges_index_path = state.diachron_home.join("indexes/exchanges.usearch");
    let exchanges_index_size_bytes = std::fs::metadata(&exchanges_index_path).map(|m| m.len()).unwrap_or(0);

    // Check if model is loaded
    let model_loaded = state.embedding_engine.read().map(|e| e.is_some()).unwrap_or(false);

    // Get model file size
    let model_path = state.diachron_home.join("models/all-MiniLM-L6-v2/model.onnx");
    let model_size_bytes = std::fs::metadata(&model_path).map(|m| m.len()).unwrap_or(0);

    // Get memory usage (platform-specific)
    let memory_rss_bytes = get_process_memory_rss();

    DiagnosticInfo {
        uptime_secs: state.uptime_secs(),
        events_count,
        exchanges_count,
        events_index_count,
        exchanges_index_count,
        database_size_bytes,
        events_index_size_bytes,
        exchanges_index_size_bytes,
        model_loaded,
        model_size_bytes,
        memory_rss_bytes,
    }
}

/// Get process RSS memory in bytes (platform-specific)
#[cfg(target_os = "macos")]
fn get_process_memory_rss() -> u64 {
    use std::process::Command;

    let pid = std::process::id();
    let output = Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .ok();

    output
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
        .map(|kb| kb * 1024) // Convert KB to bytes
        .unwrap_or(0)
}

#[cfg(target_os = "linux")]
fn get_process_memory_rss() -> u64 {
    // Read from /proc/self/statm
    std::fs::read_to_string("/proc/self/statm")
        .ok()
        .and_then(|s| s.split_whitespace().nth(1)?.parse::<u64>().ok())
        .map(|pages| pages * 4096) // Convert pages to bytes (assuming 4KB pages)
        .unwrap_or(0)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn get_process_memory_rss() -> u64 {
    0 // Unsupported platform
}
