//! Message handlers for the daemon

use std::collections::HashSet;
use std::sync::Arc;

use tracing::{debug, error, info, warn};

use diachron_core::{
    fts_search_events, fts_search_exchanges, DiagnosticInfo, IpcMessage, IpcResponse, SearchResult, SearchSource,
};

use crate::cache::{CacheEntry, CacheKey};

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

        IpcMessage::Maintenance { retention_days } => {
            info!("Maintenance requested (retention: {} days)", retention_days);
            let start = std::time::Instant::now();

            // Get size before
            let size_before = state.db.file_size();

            // Prune old data if retention is set
            let (events_pruned, exchanges_pruned) = if retention_days > 0 {
                let events = state.db.prune_old_events(retention_days).unwrap_or(0);
                let exchanges = state.db.prune_old_exchanges(retention_days).unwrap_or(0);
                info!("Pruned {} events and {} exchanges", events, exchanges);
                (events, exchanges)
            } else {
                (0, 0)
            };

            // Run VACUUM and ANALYZE
            match state.db.vacuum_and_analyze() {
                Ok(()) => {
                    let size_after = state.db.file_size();
                    let duration_ms = start.elapsed().as_millis() as u64;

                    info!(
                        "Maintenance complete: {} → {} bytes ({:.1}% reduction) in {}ms",
                        size_before,
                        size_after,
                        if size_before > 0 {
                            (1.0 - size_after as f64 / size_before as f64) * 100.0
                        } else {
                            0.0
                        },
                        duration_ms
                    );

                    IpcResponse::MaintenanceStats {
                        size_before,
                        size_after,
                        events_pruned,
                        exchanges_pruned,
                        duration_ms,
                    }
                }
                Err(e) => {
                    error!("Maintenance failed: {}", e);
                    IpcResponse::Error(format!("Maintenance failed: {}", e))
                }
            }
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

        IpcMessage::BlameByFingerprint {
            file_path,
            line_number,
            content,
            context,
            mode,
        } => {
            use diachron_core::fingerprint::{compute_fingerprint, match_fingerprint};

            info!(
                "Blame request: {}:{} mode={}",
                file_path, line_number, mode
            );

            // Compute fingerprint for the current line content
            let current_fp = compute_fingerprint(&content, Some(&context), None);

            // Query events that modified this file
            let conn = state.db.conn.lock().unwrap();
            let events = match crate::db::query_events_for_file(&conn, &file_path, 100) {
                Ok(e) => e,
                Err(e) => {
                    return IpcResponse::Error(format!("Database error: {}", e));
                }
            };
            drop(conn);

            if events.is_empty() {
                return IpcResponse::BlameNotFound {
                    reason: format!(
                        "No Diachron events found for file: {}. The line may have been written before Diachron was enabled.",
                        file_path
                    ),
                };
            }

            // Build fingerprint candidates from events with content_hash
            let conn = state.db.conn.lock().unwrap();
            let candidates = crate::db::get_event_fingerprints(&conn, &events);
            drop(conn);

            // Try fingerprint matching first
            if !candidates.is_empty() {
                if let Some(fp_match) = match_fingerprint(&current_fp, &candidates, 0.8) {
                    // Find the matching event
                    if let Some(matched_event) = events.iter().find(|e| e.id == fp_match.event_id) {
                        let confidence = match fp_match.match_type {
                            diachron_core::fingerprint::MatchType::ContentHash => "high",
                            diachron_core::fingerprint::MatchType::ContextHash => "medium",
                            diachron_core::fingerprint::MatchType::SemanticSimilarity => "low",
                        };

                        // Apply mode filtering
                        let should_return = match mode.as_str() {
                            "strict" => confidence == "high",
                            "best-effort" => confidence == "high" || confidence == "medium",
                            _ => true, // "inferred" accepts all
                        };

                        if should_return {
                            // Extract intent from conversation history (v0.5)
                            let intent = {
                                let conn = state.db.conn.lock().unwrap();
                                crate::db::find_intent_for_event(&conn, matched_event, 5)
                            };

                            return IpcResponse::BlameResult(diachron_core::BlameMatch {
                                event: matched_event.clone(),
                                confidence: confidence.to_string(),
                                match_type: format!("{:?}", fp_match.match_type),
                                similarity: fp_match.similarity,
                                intent,
                            });
                        }
                    }
                }
            }

            // Fallback to file-path heuristic (inferred confidence)
            if mode != "strict" {
                if let Some(best_match) = events.first() {
                    // Extract intent from conversation history (v0.5)
                    let intent = {
                        let conn = state.db.conn.lock().unwrap();
                        crate::db::find_intent_for_event(&conn, best_match, 5)
                    };

                    return IpcResponse::BlameResult(diachron_core::BlameMatch {
                        event: best_match.clone(),
                        confidence: "inferred".to_string(),
                        match_type: "file_path".to_string(),
                        similarity: 0.5,
                        intent,
                    });
                }
            }

            IpcResponse::BlameNotFound {
                reason: format!(
                    "No matching event found for {}:{} with mode '{}'",
                    file_path, line_number, mode
                ),
            }
        }

        IpcMessage::CorrelateEvidence {
            pr_id,
            commits,
            branch,
            start_time,
            end_time,
            intent,
        } => {
            use diachron_core::pr_correlation::correlate_events_to_pr;
            use diachron_core::{
                CommitEvidenceResult, EvidencePackResult, EvidenceSummary, VerificationStatusResult,
            };

            debug!(
                "CorrelateEvidence: PR #{}, {} commits, branch={}",
                pr_id,
                commits.len(),
                branch
            );

            // Get database connection
            let conn = state.db.conn.lock().unwrap();

            // Correlate events to commits
            match correlate_events_to_pr(&conn, pr_id, &commits, &branch, &start_time, &end_time) {
                Ok(pr_evidence) => {
                    // Generate summary
                    let summary = pr_evidence.summary();

                    // Check verification status from events
                    let mut tests_executed = false;
                    let mut build_succeeded = false;

                    for commit in &pr_evidence.commits {
                        for event in &commit.events {
                            if event.tool_name == "Bash" {
                                if let Some(ref metadata) = event.metadata {
                                    if let Ok(meta) =
                                        serde_json::from_str::<serde_json::Value>(metadata)
                                    {
                                        if let Some(category) =
                                            meta.get("command_category").and_then(|c| c.as_str())
                                        {
                                            if category == "test" {
                                                tests_executed = true;
                                            }
                                            if category == "build" {
                                                build_succeeded = true;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Verify hash chain
                    let chain_verified = {
                        match diachron_core::verify_chain(&conn) {
                            Ok(verify_result) => verify_result.valid,
                            Err(_) => false,
                        }
                    };

                    drop(conn);

                    // Convert to IPC result types
                    let commit_results: Vec<CommitEvidenceResult> = pr_evidence
                        .commits
                        .into_iter()
                        .map(|c| CommitEvidenceResult {
                            sha: c.sha,
                            message: c.message,
                            events: c.events,
                            confidence: c.confidence.as_str().to_string(),
                        })
                        .collect();

                    let result = EvidencePackResult {
                        pr_id,
                        generated_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
                        diachron_version: env!("CARGO_PKG_VERSION").to_string(),
                        branch: pr_evidence.branch,
                        summary: EvidenceSummary {
                            files_changed: summary.files_changed,
                            lines_added: summary.lines_added,
                            lines_removed: summary.lines_removed,
                            tool_operations: summary.tool_operations,
                            sessions: summary.sessions,
                        },
                        commits: commit_results,
                        verification: VerificationStatusResult {
                            chain_verified,
                            tests_executed,
                            build_succeeded,
                            human_reviewed: false,
                        },
                        intent,
                        coverage_pct: pr_evidence.coverage_pct,
                        unmatched_count: pr_evidence.unmatched_events.len(),
                        total_events: pr_evidence.total_events,
                    };

                    IpcResponse::EvidenceResult(result)
                }
                Err(e) => {
                    drop(conn);
                    error!("Failed to correlate events: {}", e);
                    IpcResponse::Error(format!("Correlation failed: {}", e))
                }
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

    let db_version = state.db.search_version().unwrap_or_else(|_| "e0:x0".to_string());
    let cache_key = CacheKey {
        query: query.to_string(),
        limit,
        source_filter: source_filter.map(|s| match s {
            SearchSource::Event => 0,
            SearchSource::Exchange => 1,
        }),
        since: since.map(str::to_string),
        project: project.map(str::to_string),
        db_version,
    };

    if let Ok(mut cache) = state.search_cache.write() {
        if let Some(entry) = cache.get(&cache_key) {
            debug!(
                "Hybrid search returned {} results (vector: {}, fts: {}, cache: hit)",
                entry.results.len(),
                entry.embedding_used,
                true
            );
            return entry.results;
        }
    }

    let query_vec = query.to_string();
    let query_fts = query_vec.clone();
    let source_filter_vec = source_filter;
    let source_filter_fts = source_filter_vec;

    let state_for_vector = Arc::clone(state);
    let vector_handle = tokio::task::spawn_blocking(move || {
        let mut results = Vec::new();
        let mut embedding_used = false;

        let events_empty = state_for_vector
            .events_index
            .read()
            .map(|idx| idx.is_empty())
            .unwrap_or(true);
        let exchanges_empty = state_for_vector
            .exchanges_index
            .read()
            .map(|idx| idx.is_empty())
            .unwrap_or(true);
        let should_embed = match source_filter_vec {
            Some(SearchSource::Event) => !events_empty,
            Some(SearchSource::Exchange) => !exchanges_empty,
            None => !(events_empty && exchanges_empty),
        };

        if !should_embed {
            return (results, false);
        }

        let query_embedding = if let Ok(mut engine_guard) = state_for_vector.embedding_engine.write()
        {
            if let Some(ref mut engine) = *engine_guard {
                match engine.embed(&query_vec) {
                    Ok(emb) => {
                        embedding_used = true;
                        Some(emb)
                    }
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
            if source_filter_vec.is_none() || source_filter_vec == Some(SearchSource::Event) {
                if let Ok(idx) = state_for_vector.events_index.read() {
                    match idx.search(emb, limit) {
                        Ok(vector_results) => {
                            for vr in vector_results {
                                if let Some(id_str) = vr.id.strip_prefix("event:") {
                                    results.push(SearchResult {
                                        id: id_str.to_string(),
                                        score: vr.score,
                                        source: SearchSource::Event,
                                        snippet: String::new(),
                                        timestamp: String::new(),
                                        project: None,
                                    });
                                }
                            }
                        }
                        Err(e) => warn!("Vector search failed: {}", e),
                    }
                }
            }

            if source_filter_vec.is_none() || source_filter_vec == Some(SearchSource::Exchange) {
                if let Ok(idx) = state_for_vector.exchanges_index.read() {
                    match idx.search(emb, limit) {
                        Ok(vector_results) => {
                            for vr in vector_results {
                                if let Some(id_str) = vr.id.strip_prefix("exchange:") {
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
                        Err(e) => warn!("Vector search failed: {}", e),
                    }
                }
            }
        }

        (results, embedding_used)
    });

    let state_for_fts = Arc::clone(state);
    let fts_handle = tokio::task::spawn_blocking(move || {
        let mut results = Vec::new();
        let conn = match state_for_fts.db.open_readonly() {
            Ok(conn) => conn,
            Err(e) => {
                warn!("Failed to open read-only connection for FTS: {}", e);
                return results;
            }
        };

        if source_filter_fts.is_none() || source_filter_fts == Some(SearchSource::Event) {
            match fts_search_events(&conn, &query_fts, limit) {
                Ok(fts_results) => {
                    for fts in fts_results {
                        results.push(SearchResult {
                            id: fts.id,
                            score: (-fts.score) as f32,
                            source: SearchSource::Event,
                            snippet: fts.snippet,
                            timestamp: fts.timestamp,
                            project: fts.context,
                        });
                    }
                }
                Err(e) => warn!("FTS events search failed: {}", e),
            }
        }

        if source_filter_fts.is_none() || source_filter_fts == Some(SearchSource::Exchange) {
            match fts_search_exchanges(&conn, &query_fts, limit) {
                Ok(fts_results) => {
                    for fts in fts_results {
                        results.push(SearchResult {
                            id: fts.id,
                            score: (-fts.score) as f32,
                            source: SearchSource::Exchange,
                            snippet: fts.snippet,
                            timestamp: fts.timestamp,
                            project: fts.context,
                        });
                    }
                }
                Err(e) => warn!("FTS exchanges search failed: {}", e),
            }
        }

        results
    });

    let (vector_results, embedding_used) = match vector_handle.await {
        Ok((results, used)) => (results, used),
        Err(e) => {
            warn!("Vector search task failed: {}", e);
            (Vec::new(), false)
        }
    };
    let fts_results = match fts_handle.await {
        Ok(results) => results,
        Err(e) => {
            warn!("FTS search task failed: {}", e);
            Vec::new()
        }
    };

    let mut results = Vec::new();
    let mut seen_ids = HashSet::new();

    for result in vector_results {
        let key = match result.source {
            SearchSource::Event => format!("event:{}", result.id),
            SearchSource::Exchange => format!("exchange:{}", result.id),
        };
        if seen_ids.insert(key) {
            results.push(result);
        }
    }

    for result in fts_results {
        let key = match result.source {
            SearchSource::Event => format!("event:{}", result.id),
            SearchSource::Exchange => format!("exchange:{}", result.id),
        };
        if seen_ids.insert(key) {
            results.push(result);
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

    if let Ok(mut cache) = state.search_cache.write() {
        cache.insert(
            cache_key,
            CacheEntry {
                results: results.clone(),
                embedding_used,
            },
        );
    }

    debug!(
        "Hybrid search returned {} results (vector: {}, fts: {}, cache: miss)",
        results.len(),
        embedding_used,
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

#[cfg(test)]
mod tests {
    use super::hybrid_search;
    use crate::DaemonState;
    use diachron_core::{CaptureEvent, Exchange, Operation, SearchSource};
    use std::collections::HashSet;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("diachron-test-{}", nanos));
        std::fs::create_dir_all(&dir).expect("failed to create temp dir");
        dir
    }

    #[tokio::test]
    async fn test_search_golden_output_and_cache_invalidation() {
        let dir = temp_dir();
        let db_path = dir.join("diachron.db");
        let state = DaemonState::new_for_tests(db_path).expect("test state");
        let state = Arc::new(state);

        let event = CaptureEvent {
            tool_name: "Write".to_string(),
            file_path: Some("src/auth.rs".to_string()),
            operation: Operation::Create,
            diff_summary: Some("only_event_token".to_string()),
            raw_input: Some("auth token added".to_string()),
            metadata: None,
            git_commit_sha: None,
            command_category: None,
        };
        let first_id = state.db.save_event(&event, Some("session-1"), None).unwrap();

        let exchange = Exchange {
            id: "ex-1".to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            project: Some("test-project".to_string()),
            session_id: Some("session-1".to_string()),
            user_message: "only_exchange_token".to_string(),
            assistant_message: "response".to_string(),
            tool_calls: None,
            archive_path: None,
            line_start: None,
            line_end: None,
            embedding: None,
            summary: None,
            git_branch: None,
            cwd: None,
        };
        state.db.save_exchange(&exchange, None).unwrap();

        let results = hybrid_search(
            &state,
            "only_event_token",
            10,
            Some(SearchSource::Event),
            None,
            None,
        )
        .await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source, SearchSource::Event);
        assert_eq!(results[0].id, first_id.to_string());

        // Insert another matching event to ensure cache invalidates.
        let event2 = CaptureEvent {
            tool_name: "Write".to_string(),
            file_path: Some("src/auth2.rs".to_string()),
            operation: Operation::Modify,
            diff_summary: Some("only_event_token".to_string()),
            raw_input: Some("auth token updated".to_string()),
            metadata: None,
            git_commit_sha: None,
            command_category: None,
        };
        let second_id = state.db.save_event(&event2, Some("session-2"), None).unwrap();

        let results_after = hybrid_search(
            &state,
            "only_event_token",
            10,
            Some(SearchSource::Event),
            None,
            None,
        )
        .await;

        let ids: HashSet<String> = results_after.into_iter().map(|r| r.id).collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&first_id.to_string()));
        assert!(ids.contains(&second_id.to_string()));
    }
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
