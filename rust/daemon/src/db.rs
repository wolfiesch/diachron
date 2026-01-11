//! Database operations for the daemon
//!
//! Handles event storage and queries for the unified database.
//!
//! Uses a mutex-wrapped connection for thread-safe access in async context.

use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OpenFlags};
use tracing::debug;

use diachron_core::{
    compute_event_hash, get_last_event_hash, CaptureEvent, EventHashInput, Exchange, StoredEvent,
    GENESIS_HASH,
};

/// Database handle for the daemon.
///
/// The connection is wrapped in a `Mutex` because `rusqlite::Connection`
/// is not `Send`/`Sync`, but we share it across async tasks.
pub struct Database {
    /// Path to the database file
    path: PathBuf,
    /// Thread-safe connection wrapper (pub for handler access)
    pub conn: Mutex<Connection>,
    /// Read-only connection for data version tracking
    version_conn: Mutex<Connection>,
}

impl Database {
    /// Open or create a database at the given path.
    ///
    /// # Arguments
    /// - `path`: Path to the SQLite database file.
    ///
    /// # Errors
    /// Returns `anyhow::Error` if the database cannot be opened or initialized.
    pub fn open(path: PathBuf) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        // Open connection and initialize schema
        let conn = Connection::open(&path).context("Failed to open database")?;
        diachron_core::schema::init_schema(&conn).context("Failed to initialize schema")?;

        let version_conn = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .context("Failed to open version connection")?;

        Ok(Self {
            path,
            conn: Mutex::new(conn),
            version_conn: Mutex::new(version_conn),
        })
    }

    /// Access the connection via a mutex lock.
    ///
    /// Use this for FTS queries that need direct connection access.
    ///
    /// # Arguments
    /// - `f`: Callback executed with a locked connection.
    ///
    /// # Errors
    /// Returns any `rusqlite::Error` from the callback.
    pub fn with_conn<F, R>(&self, f: F) -> Result<R, rusqlite::Error>
    where
        F: FnOnce(&Connection) -> Result<R, rusqlite::Error>,
    {
        let conn = self.conn.lock().unwrap();
        f(&conn)
    }

    /// Return the current data version for cache invalidation.
    ///
    /// Uses max row identifiers so writes on this connection are reflected.
    pub fn search_version(&self) -> Result<String, rusqlite::Error> {
        let conn = self.version_conn.lock().unwrap();
        let version: i64 = conn.query_row("PRAGMA data_version", [], |row| row.get(0))?;
        Ok(version.to_string())
    }

    /// Open a new read-only connection for parallel queries.
    ///
    /// This avoids blocking the primary mutex-held connection during FTS.
    pub fn open_readonly(&self) -> Result<Connection, rusqlite::Error> {
        Connection::open_with_flags(&self.path, OpenFlags::SQLITE_OPEN_READ_ONLY)
    }

    /// Save a capture event to the database with hash-chain integrity.
    ///
    /// Each event is linked to the previous event via SHA256 hash chain,
    /// enabling tamper detection across the entire event history.
    ///
    /// # Arguments
    /// - `event`: Capture event data.
    /// - `session_id`: Optional session identifier.
    /// - `embedding`: Optional embedding vector (stored as f32 blob).
    ///
    /// # Returns
    /// Inserted row ID.
    ///
    /// # Errors
    /// Returns `rusqlite::Error` if the insert fails.
    pub fn save_event(
        &self,
        event: &CaptureEvent,
        session_id: Option<&str>,
        embedding: Option<&[f32]>,
    ) -> rusqlite::Result<i64> {
        let timestamp = chrono::Local::now();
        let timestamp_iso = timestamp.format("%Y-%m-%dT%H:%M:%S%.3f").to_string();

        // Use actual system timezone (e.g., PST, EST, UTC, etc.)
        let tz_name = timestamp.format("%Z").to_string();
        let timestamp_display = timestamp
            .format(&format!("%m/%d/%Y %I:%M %p {}", tz_name))
            .to_string();

        // Build metadata JSON - preserve metadata from event (includes git_branch)
        // and merge with any additional fields
        let metadata = if let Some(ref existing_meta) = event.metadata {
            // Try to parse and merge with existing metadata
            if let Ok(mut meta) = serde_json::from_str::<serde_json::Value>(existing_meta) {
                // Add command_category if not already present
                if let Some(category) = event.command_category.as_ref() {
                    if meta.get("command_category").is_none() {
                        meta["command_category"] = serde_json::json!(category.as_str());
                    }
                }
                meta
            } else {
                // Couldn't parse, create fresh metadata
                serde_json::json!({
                    "command_category": event.command_category.as_ref().map(|c| c.as_str()),
                })
            }
        } else {
            serde_json::json!({
                "command_category": event.command_category.as_ref().map(|c| c.as_str()),
            })
        };

        // Convert embedding to blob if present
        let embedding_blob: Option<Vec<u8>> =
            embedding.map(|emb| emb.iter().flat_map(|f| f.to_le_bytes()).collect());

        let conn = self.conn.lock().unwrap();

        // Get the previous event's hash for chain linkage
        let prev_hash = get_last_event_hash(&conn).unwrap_or(GENESIS_HASH);

        // Determine the next event ID (needed for hash computation)
        let next_id: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(id), 0) + 1 FROM events",
                [],
                |row| row.get(0),
            )
            .unwrap_or(1);

        // Build hash input with all event data
        let hash_input = EventHashInput {
            id: next_id,
            timestamp: timestamp_iso.clone(),
            tool_name: event.tool_name.clone(),
            file_path: event.file_path.clone(),
            operation: event.operation.as_str().to_string(),
            diff_summary: event.diff_summary.clone(),
            raw_input: event.raw_input.clone(),
            session_id: session_id.map(|s| s.to_string()),
            git_commit_sha: event.git_commit_sha.clone(),
            metadata: Some(metadata.to_string()),
        };

        // Compute event hash
        let event_hash = compute_event_hash(&hash_input, &prev_hash);

        conn.execute(
            "INSERT INTO events (
                timestamp, timestamp_display, session_id, tool_name, file_path,
                operation, diff_summary, raw_input, git_commit_sha, metadata, embedding,
                prev_hash, event_hash
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                timestamp_iso,
                timestamp_display,
                session_id,
                event.tool_name,
                event.file_path,
                event.operation.as_str(),
                event.diff_summary,
                event.raw_input,
                event.git_commit_sha,
                metadata.to_string(),
                embedding_blob,
                prev_hash.as_slice(),
                event_hash.as_slice(),
            ],
        )?;

        Ok(conn.last_insert_rowid())
    }

    /// Query events with optional filters.
    ///
    /// # Arguments
    /// - `since`: Optional time filter (relative or ISO).
    /// - `file_filter`: Optional file path substring.
    /// - `limit`: Maximum number of events to return.
    ///
    /// # Returns
    /// Vector of stored events ordered by timestamp (descending).
    ///
    /// # Errors
    /// Returns `rusqlite::Error` if the query fails.
    pub fn query_events(
        &self,
        since: Option<&str>,
        file_filter: Option<&str>,
        limit: usize,
    ) -> rusqlite::Result<Vec<StoredEvent>> {
        let conn = self.conn.lock().unwrap();

        // Build query dynamically
        let mut sql = String::from(
            "SELECT id, timestamp, timestamp_display, session_id, tool_name, file_path,
                    operation, diff_summary, raw_input, ai_summary, git_commit_sha, metadata
             FROM events WHERE 1=1",
        );
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(since) = since {
            // Parse relative time or ISO date
            if let Some(timestamp) = parse_time_filter(since) {
                sql.push_str(" AND timestamp >= ?");
                params.push(Box::new(timestamp));
            }
        }

        if let Some(file) = file_filter {
            sql.push_str(" AND file_path LIKE ?");
            params.push(Box::new(format!("%{}%", file)));
        }

        sql.push_str(" ORDER BY timestamp DESC LIMIT ?");
        params.push(Box::new(limit as i64));

        debug!("Query: {} with {} params", sql, params.len());

        let mut stmt = conn.prepare(&sql)?;

        // Convert params to references
        let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();

        let events = stmt
            .query_map(param_refs.as_slice(), |row| {
                Ok(StoredEvent {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    timestamp_display: row.get(2)?,
                    session_id: row.get(3)?,
                    tool_name: row.get(4)?,
                    file_path: row.get(5)?,
                    operation: row.get(6)?,
                    diff_summary: row.get(7)?,
                    raw_input: row.get(8)?,
                    ai_summary: row.get(9)?,
                    git_commit_sha: row.get(10)?,
                    metadata: row.get(11)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(events)
    }

    /// Get total event count.
    ///
    /// # Returns
    /// Total number of events in the database.
    ///
    /// # Errors
    /// Returns `rusqlite::Error` if the query fails.
    pub fn event_count(&self) -> rusqlite::Result<u64> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))?;
        Ok(count as u64)
    }

    /// Save a conversation exchange to the database.
    ///
    /// Uses INSERT OR REPLACE to handle re-indexing gracefully.
    ///
    /// # Arguments
    /// - `exchange`: Exchange record to persist.
    /// - `embedding`: Optional embedding vector (stored as f32 blob).
    ///
    /// # Errors
    /// Returns `rusqlite::Error` if the insert fails.
    pub fn save_exchange(
        &self,
        exchange: &Exchange,
        embedding: Option<&[f32]>,
    ) -> rusqlite::Result<()> {
        // Convert embedding to blob if present
        let embedding_blob: Option<Vec<u8>> =
            embedding.map(|emb| emb.iter().flat_map(|f| f.to_le_bytes()).collect());

        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO exchanges (
                id, timestamp, project, session_id, user_message,
                assistant_message, tool_calls, archive_path, line_start,
                line_end, embedding, summary, git_branch, cwd
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                exchange.id,
                exchange.timestamp,
                exchange.project,
                exchange.session_id,
                exchange.user_message,
                exchange.assistant_message,
                exchange.tool_calls,
                exchange.archive_path,
                exchange.line_start,
                exchange.line_end,
                embedding_blob,
                exchange.summary,
                exchange.git_branch,
                exchange.cwd,
            ],
        )?;

        debug!("Saved exchange: {}", exchange.id);
        Ok(())
    }

    /// Get total exchange count.
    ///
    /// # Returns
    /// Total number of exchanges in the database.
    ///
    /// # Errors
    /// Returns `rusqlite::Error` if the query fails.
    pub fn exchange_count(&self) -> rusqlite::Result<u64> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM exchanges", [], |row| row.get(0))?;
        Ok(count as u64)
    }

    /// Get exchanges without summaries for summarization.
    ///
    /// # Arguments
    /// - `limit`: Maximum number of exchanges to return.
    ///
    /// # Returns
    /// Vector of (id, user_message, assistant_message) tuples.
    ///
    /// # Errors
    /// Returns `rusqlite::Error` if the query fails.
    pub fn get_exchanges_without_summary(
        &self,
        limit: usize,
    ) -> rusqlite::Result<Vec<(String, String, String)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, user_message, assistant_message
             FROM exchanges
             WHERE summary IS NULL OR summary = ''
             ORDER BY timestamp DESC
             LIMIT ?",
        )?;

        let rows = stmt.query_map([limit], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Update an exchange's summary.
    ///
    /// # Arguments
    /// - `id`: Exchange ID.
    /// - `summary`: The generated summary.
    ///
    /// # Errors
    /// Returns `rusqlite::Error` if the update fails.
    pub fn update_exchange_summary(&self, id: &str, summary: &str) -> rusqlite::Result<usize> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE exchanges SET summary = ? WHERE id = ?",
            params![summary, id],
        )
    }

    /// Get the database file size in bytes.
    pub fn file_size(&self) -> u64 {
        std::fs::metadata(&self.path)
            .map(|m| m.len())
            .unwrap_or(0)
    }

    /// Run database maintenance: VACUUM and ANALYZE.
    ///
    /// VACUUM reclaims unused space and defragments the database.
    /// ANALYZE updates query planner statistics for better performance.
    ///
    /// # Errors
    /// Returns `rusqlite::Error` if maintenance operations fail.
    pub fn vacuum_and_analyze(&self) -> rusqlite::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("VACUUM", [])?;
        conn.execute("ANALYZE", [])?;
        Ok(())
    }

    /// Prune events older than a given number of days.
    ///
    /// # Arguments
    /// - `days`: Delete events older than this many days.
    ///
    /// # Returns
    /// Number of events deleted.
    ///
    /// # Errors
    /// Returns `rusqlite::Error` if the delete fails.
    pub fn prune_old_events(&self, days: u32) -> rusqlite::Result<u64> {
        let conn = self.conn.lock().unwrap();
        let cutoff = chrono::Local::now() - chrono::Duration::days(days as i64);
        let cutoff_str = cutoff.format("%Y-%m-%dT%H:%M:%S").to_string();

        let deleted = conn.execute(
            "DELETE FROM events WHERE timestamp < ?",
            params![cutoff_str],
        )?;
        Ok(deleted as u64)
    }

    /// Prune exchanges older than a given number of days.
    ///
    /// # Arguments
    /// - `days`: Delete exchanges older than this many days.
    ///
    /// # Returns
    /// Number of exchanges deleted.
    ///
    /// # Errors
    /// Returns `rusqlite::Error` if the delete fails.
    pub fn prune_old_exchanges(&self, days: u32) -> rusqlite::Result<u64> {
        let conn = self.conn.lock().unwrap();
        let cutoff = chrono::Local::now() - chrono::Duration::days(days as i64);
        let cutoff_str = cutoff.format("%Y-%m-%dT%H:%M:%S").to_string();

        let deleted = conn.execute(
            "DELETE FROM exchanges WHERE timestamp < ?",
            params![cutoff_str],
        )?;
        Ok(deleted as u64)
    }
}

/// Parse a time filter string into an ISO timestamp
fn parse_time_filter(filter: &str) -> Option<String> {
    let now = chrono::Local::now();

    // Handle relative times like "1h", "2d", "yesterday"
    let filter_lower = filter.to_lowercase();

    if filter_lower == "yesterday" {
        let yesterday = now - chrono::Duration::days(1);
        return Some(yesterday.format("%Y-%m-%dT00:00:00").to_string());
    }

    if filter_lower == "today" {
        return Some(now.format("%Y-%m-%dT00:00:00").to_string());
    }

    // Parse "1h", "2d", "30m" etc
    if let Some(num_str) = filter_lower.strip_suffix('h') {
        if let Ok(hours) = num_str.parse::<i64>() {
            let past = now - chrono::Duration::hours(hours);
            return Some(past.format("%Y-%m-%dT%H:%M:%S").to_string());
        }
    }

    if let Some(num_str) = filter_lower.strip_suffix('d') {
        if let Ok(days) = num_str.parse::<i64>() {
            let past = now - chrono::Duration::days(days);
            return Some(past.format("%Y-%m-%dT%H:%M:%S").to_string());
        }
    }

    if let Some(num_str) = filter_lower.strip_suffix('m') {
        if let Ok(minutes) = num_str.parse::<i64>() {
            let past = now - chrono::Duration::minutes(minutes);
            return Some(past.format("%Y-%m-%dT%H:%M:%S").to_string());
        }
    }

    // Try parsing as ISO date
    if filter.len() >= 10 {
        // Assume it's a date like "2024-01-01"
        return Some(format!("{}T00:00:00", &filter[..10]));
    }

    None
}

/// Query events that modified a specific file
///
/// # Arguments
/// - `conn`: Database connection
/// - `file_path`: Path to the file (can be partial match)
/// - `limit`: Maximum number of events to return
///
/// # Returns
/// Events matching the file path, ordered by timestamp descending
pub fn query_events_for_file(
    conn: &Connection,
    file_path: &str,
    limit: usize,
) -> rusqlite::Result<Vec<StoredEvent>> {
    let mut stmt = conn.prepare(
        "SELECT id, timestamp, timestamp_display, session_id, tool_name, file_path,
                operation, diff_summary, raw_input, ai_summary, git_commit_sha, metadata
         FROM events
         WHERE file_path LIKE ?1
         ORDER BY timestamp DESC
         LIMIT ?2",
    )?;

    let events = stmt
        .query_map(params![format!("%{}", file_path), limit as i64], |row| {
            Ok(StoredEvent {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                timestamp_display: row.get(2)?,
                session_id: row.get(3)?,
                tool_name: row.get(4)?,
                file_path: row.get(5)?,
                operation: row.get(6)?,
                diff_summary: row.get(7)?,
                raw_input: row.get(8)?,
                ai_summary: row.get(9)?,
                git_commit_sha: row.get(10)?,
                metadata: row.get(11)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(events)
}

/// Get fingerprints for a set of events
///
/// # Arguments
/// - `conn`: Database connection
/// - `events`: Events to get fingerprints for
///
/// # Returns
/// Vector of (event_id, HunkFingerprint) tuples for events that have fingerprints stored
pub fn get_event_fingerprints(
    conn: &Connection,
    events: &[StoredEvent],
) -> Vec<(i64, diachron_core::fingerprint::HunkFingerprint)> {
    use diachron_core::fingerprint::HunkFingerprint;

    let mut fingerprints = Vec::new();

    for event in events {
        // Query for stored fingerprint hashes
        let result: rusqlite::Result<(Vec<u8>, Vec<u8>)> = conn.query_row(
            "SELECT content_hash, context_hash FROM events WHERE id = ?1",
            params![event.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        );

        if let Ok((content_hash, context_hash)) = result {
            // Convert blobs to fixed-size arrays
            if content_hash.len() == 32 && context_hash.len() == 32 {
                let mut ch = [0u8; 32];
                let mut xh = [0u8; 32];
                ch.copy_from_slice(&content_hash);
                xh.copy_from_slice(&context_hash);

                fingerprints.push((
                    event.id,
                    HunkFingerprint {
                        content_hash: ch,
                        context_hash: xh,
                        semantic_sig: None, // Not stored in DB yet
                    },
                ));
            }
        }
    }

    fingerprints
}

// ============================================================================
// INTENT EXTRACTION FUNCTIONS (v0.5)
// 01/11/2026 - Added query_exchanges_for_intent, score_intent_match,
//              find_intent_for_event, extract_intent_summary (Claude)
// ============================================================================

/// Query exchanges that could explain an event's intent.
///
/// Returns exchanges from the same session that occurred BEFORE the event,
/// ordered by timestamp descending (most recent first).
///
/// # Arguments
/// - `conn`: Database connection
/// - `session_id`: Session ID to filter by
/// - `before_timestamp`: Only return exchanges before this ISO timestamp
/// - `limit`: Maximum number of exchanges to return
///
/// # Returns
/// Vector of exchanges that could contain the user's intent
pub fn query_exchanges_for_intent(
    conn: &Connection,
    session_id: &str,
    before_timestamp: &str,
    limit: usize,
) -> rusqlite::Result<Vec<Exchange>> {
    let mut stmt = conn.prepare(
        "SELECT id, timestamp, project, session_id, user_message,
                assistant_message, tool_calls, archive_path, line_start,
                line_end, embedding, summary, git_branch, cwd
         FROM exchanges
         WHERE session_id = ?1 AND timestamp < ?2
         ORDER BY timestamp DESC
         LIMIT ?3",
    )?;

    let exchanges = stmt
        .query_map(params![session_id, before_timestamp, limit as i64], |row| {
            // Handle embedding blob -> Vec<f32> conversion
            let embedding_blob: Option<Vec<u8>> = row.get(10)?;
            let embedding = embedding_blob.map(|blob| {
                blob.chunks_exact(4)
                    .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                    .collect()
            });

            Ok(Exchange {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                project: row.get(2)?,
                session_id: row.get(3)?,
                user_message: row.get(4)?,
                assistant_message: row.get(5)?,
                tool_calls: row.get(6)?,
                archive_path: row.get(7)?,
                line_start: row.get(8)?,
                line_end: row.get(9)?,
                embedding,
                summary: row.get(11)?,
                git_branch: row.get(12)?,
                cwd: row.get(13)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(exchanges)
}

/// Score how well an exchange explains an event's intent.
///
/// Scoring factors:
/// - +3: File path mentioned in user_message
/// - +2: Tool name in tool_calls matches event.tool_name
/// - +1: Same git branch
///
/// # Arguments
/// - `exchange`: The exchange to score
/// - `event`: The event we're trying to explain
///
/// # Returns
/// Score indicating relevance (higher is better)
pub fn score_intent_match(exchange: &Exchange, event: &StoredEvent) -> u32 {
    let mut score = 0u32;

    // +3 for file path mention
    if let Some(ref file_path) = event.file_path {
        // Extract just the filename for matching (more likely to appear in user message)
        let filename = file_path
            .rsplit('/')
            .next()
            .unwrap_or(file_path);

        if exchange.user_message.contains(filename)
            || exchange.user_message.contains(file_path) {
            score += 3;
        }
    }

    // +2 for tool name match in tool_calls
    if let Some(ref tool_calls) = exchange.tool_calls {
        if tool_calls.contains(&event.tool_name) {
            score += 2;
        }
    }

    // +1 for same git branch
    if let (Some(ref exchange_branch), Some(ref metadata)) = (&exchange.git_branch, &event.metadata) {
        // Parse metadata JSON to extract git_branch
        if let Ok(meta) = serde_json::from_str::<serde_json::Value>(metadata) {
            if let Some(event_branch) = meta.get("git_branch").and_then(|v| v.as_str()) {
                if exchange_branch == event_branch {
                    score += 1;
                }
            }
        }
    }

    score
}

/// Find the user intent that motivated an event.
///
/// Queries exchanges from the same session, scores them by relevance,
/// and extracts the intent from the best-matching user message.
///
/// # Arguments
/// - `conn`: Database connection
/// - `event`: The event to find intent for
/// - `max_exchanges`: Maximum exchanges to consider
///
/// # Returns
/// Extracted intent string, or None if no matching exchanges found
pub fn find_intent_for_event(
    conn: &Connection,
    event: &StoredEvent,
    max_exchanges: usize,
) -> Option<String> {
    // Need session_id to correlate
    let session_id = event.session_id.as_ref()?;

    // Query exchanges from same session, before this event
    let exchanges = query_exchanges_for_intent(
        conn,
        session_id,
        &event.timestamp,
        max_exchanges,
    ).ok()?;

    if exchanges.is_empty() {
        return None;
    }

    // Score each exchange and find the best match
    let mut scored: Vec<(u32, &Exchange)> = exchanges
        .iter()
        .map(|ex| (score_intent_match(ex, event), ex))
        .collect();

    // Sort by score descending, then by timestamp descending (most recent)
    scored.sort_by(|a, b| b.0.cmp(&a.0));

    // Take the best-scoring exchange
    let (_, best_exchange) = scored.first()?;

    // Extract intent summary from user message
    Some(extract_intent_summary(&best_exchange.user_message, 150))
}

/// Extract the core intent from a user message.
///
/// Filters out system context lines and XML-like blocks,
/// takes the first sentence, and truncates at word boundary if too long.
///
/// # Arguments
/// - `user_message`: The full user message text
/// - `max_chars`: Maximum characters for the result
///
/// # Returns
/// Cleaned intent string
pub fn extract_intent_summary(user_message: &str, max_chars: usize) -> String {
    // First pass: remove XML-like blocks (<tag>content</tag>)
    // This handles system-reminder, context, and other injected blocks
    let mut depth: u32 = 0;
    let mut in_block = false;
    let mut cleaned_lines = Vec::new();

    for line in user_message.lines() {
        let trimmed = line.trim();

        // Check for opening tag
        if trimmed.starts_with('<') && !trimmed.starts_with("</") {
            depth += 1;
            in_block = true;
            continue;
        }

        // Check for closing tag
        if trimmed.starts_with("</") {
            depth = depth.saturating_sub(1);
            in_block = depth > 0;
            continue;
        }

        // Skip content inside blocks
        if in_block {
            continue;
        }

        // Skip other common context markers
        if trimmed.starts_with("```")
            || trimmed.starts_with("---")
            || trimmed.starts_with("Context:")
            || trimmed.starts_with("Note:")
            || trimmed.is_empty()
        {
            continue;
        }

        cleaned_lines.push(trimmed);
        if cleaned_lines.len() >= 3 {
            break; // Take at most first 3 meaningful lines
        }
    }

    let cleaned = cleaned_lines.join(" ");

    if cleaned.is_empty() {
        return String::new();
    }

    // Find first sentence (ends with . ! or ?)
    let first_sentence = cleaned
        .split_inclusive(&['.', '!', '?'][..])
        .next()
        .unwrap_or(&cleaned)
        .trim();

    // Truncate at word boundary if needed
    if first_sentence.len() <= max_chars {
        return first_sentence.to_string();
    }

    // Find last space before max_chars
    let truncated = &first_sentence[..max_chars];
    if let Some(last_space) = truncated.rfind(' ') {
        format!("{}...", &truncated[..last_space])
    } else {
        format!("{}...", truncated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diachron_core::Operation;

    #[test]
    fn test_save_and_query() {
        let db = Database::open(PathBuf::from(":memory:")).unwrap();

        let event = CaptureEvent {
            tool_name: "Write".to_string(),
            file_path: Some("test.txt".to_string()),
            operation: Operation::Create,
            diff_summary: Some("+10 lines".to_string()),
            raw_input: None,
            metadata: None,
            git_commit_sha: None,
            command_category: None,
        };

        // Third parameter is now embedding (None = no embedding)
        let id = db.save_event(&event, Some("test-session"), None).unwrap();
        assert!(id > 0);

        let events = db.query_events(None, None, 10).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].tool_name, "Write");
    }

    #[test]
    fn test_save_exchange() {
        let db = Database::open(PathBuf::from(":memory:")).unwrap();

        let exchange = Exchange {
            id: "test-exchange-001".to_string(),
            timestamp: "2026-01-10T12:00:00Z".to_string(),
            project: Some("test-project".to_string()),
            session_id: Some("session-123".to_string()),
            user_message: "How do I implement authentication?".to_string(),
            assistant_message: "You can use OAuth2 or JWT...".to_string(),
            tool_calls: Some(r#"["Read", "Write"]"#.to_string()),
            archive_path: Some("/path/to/archive.jsonl".to_string()),
            line_start: Some(100),
            line_end: Some(150),
            embedding: None,
            summary: Some("Discussion about auth implementation".to_string()),
            git_branch: Some("feat/auth".to_string()),
            cwd: Some("/home/user/project".to_string()),
        };

        // Save without embedding
        db.save_exchange(&exchange, None).unwrap();

        // Verify count
        assert_eq!(db.exchange_count().unwrap(), 1);

        // Save with embedding (384-dim vector)
        let embedding = vec![0.1f32; 384];
        let exchange2 = Exchange {
            id: "test-exchange-002".to_string(),
            ..exchange.clone()
        };
        db.save_exchange(&exchange2, Some(&embedding)).unwrap();

        assert_eq!(db.exchange_count().unwrap(), 2);

        // Test INSERT OR REPLACE (re-save same ID should not increase count)
        db.save_exchange(&exchange, None).unwrap();
        assert_eq!(db.exchange_count().unwrap(), 2);
    }

    #[test]
    fn test_parse_time_filter() {
        assert!(parse_time_filter("1h").is_some());
        assert!(parse_time_filter("2d").is_some());
        assert!(parse_time_filter("yesterday").is_some());
        assert!(parse_time_filter("2024-01-01").is_some());
        assert!(parse_time_filter("invalid").is_none());
    }

    // ============================================================================
    // INTENT EXTRACTION TESTS (v0.5)
    // ============================================================================

    #[test]
    fn test_extract_intent_summary_simple() {
        let msg = "Fix the login button bug on the dashboard.";
        let result = extract_intent_summary(msg, 150);
        assert_eq!(result, "Fix the login button bug on the dashboard.");
    }

    #[test]
    fn test_extract_intent_summary_filters_context() {
        let msg = "<system-reminder>\nThis is context\n</system-reminder>\nFix the auth flow.";
        let result = extract_intent_summary(msg, 150);
        assert_eq!(result, "Fix the auth flow.");
    }

    #[test]
    fn test_extract_intent_summary_truncates_at_word() {
        let msg = "This is a very long message that should be truncated at a word boundary.";
        let result = extract_intent_summary(msg, 30);
        assert!(result.ends_with("..."));
        assert!(result.len() <= 33); // 30 + "..."
    }

    #[test]
    fn test_extract_intent_summary_takes_first_sentence() {
        let msg = "Add user authentication. Also update the tests. And refactor the config.";
        let result = extract_intent_summary(msg, 150);
        assert_eq!(result, "Add user authentication.");
    }

    #[test]
    fn test_score_intent_match_file_mention() {
        let exchange = Exchange {
            id: "ex-1".to_string(),
            timestamp: "2026-01-10T12:00:00Z".to_string(),
            project: None,
            session_id: Some("sess-1".to_string()),
            user_message: "Fix the bug in handlers.rs please".to_string(),
            assistant_message: "I'll fix it".to_string(),
            tool_calls: Some(r#"["Edit"]"#.to_string()),
            archive_path: None,
            line_start: None,
            line_end: None,
            embedding: None,
            summary: None,
            git_branch: Some("main".to_string()),
            cwd: None,
        };

        let event = StoredEvent {
            id: 1,
            timestamp: "2026-01-10T12:05:00Z".to_string(),
            timestamp_display: None,
            session_id: Some("sess-1".to_string()),
            tool_name: "Edit".to_string(),
            file_path: Some("/project/src/handlers.rs".to_string()),
            operation: Some("modify".to_string()),
            diff_summary: None,
            raw_input: None,
            ai_summary: None,
            git_commit_sha: None,
            metadata: Some(r#"{"git_branch": "main"}"#.to_string()),
        };

        let score = score_intent_match(&exchange, &event);
        // +3 for file mention (handlers.rs), +2 for tool match (Edit), +1 for branch match
        assert_eq!(score, 6);
    }

    #[test]
    fn test_score_intent_match_no_matches() {
        let exchange = Exchange {
            id: "ex-1".to_string(),
            timestamp: "2026-01-10T12:00:00Z".to_string(),
            project: None,
            session_id: Some("sess-1".to_string()),
            user_message: "How do I implement caching?".to_string(),
            assistant_message: "You can use Redis...".to_string(),
            tool_calls: Some(r#"["Read"]"#.to_string()),
            archive_path: None,
            line_start: None,
            line_end: None,
            embedding: None,
            summary: None,
            git_branch: Some("develop".to_string()),
            cwd: None,
        };

        let event = StoredEvent {
            id: 1,
            timestamp: "2026-01-10T12:05:00Z".to_string(),
            timestamp_display: None,
            session_id: Some("sess-1".to_string()),
            tool_name: "Write".to_string(),
            file_path: Some("/project/src/auth.rs".to_string()),
            operation: Some("create".to_string()),
            diff_summary: None,
            raw_input: None,
            ai_summary: None,
            git_commit_sha: None,
            metadata: Some(r#"{"git_branch": "main"}"#.to_string()),
        };

        let score = score_intent_match(&exchange, &event);
        assert_eq!(score, 0);
    }

    #[test]
    fn test_find_intent_for_event_same_session() {
        let db = Database::open(PathBuf::from(":memory:")).unwrap();

        // Save an exchange first
        let exchange = Exchange {
            id: "ex-intent-1".to_string(),
            timestamp: "2026-01-10T12:00:00Z".to_string(),
            project: Some("test".to_string()),
            session_id: Some("session-intent".to_string()),
            user_message: "Fix the 401 errors on page refresh.".to_string(),
            assistant_message: "I'll update the token refresh logic.".to_string(),
            tool_calls: Some(r#"["Edit"]"#.to_string()),
            archive_path: None,
            line_start: None,
            line_end: None,
            embedding: None,
            summary: None,
            git_branch: None,
            cwd: None,
        };
        db.save_exchange(&exchange, None).unwrap();

        // Create an event in the same session, after the exchange
        let event = StoredEvent {
            id: 1,
            timestamp: "2026-01-10T12:05:00Z".to_string(),
            timestamp_display: None,
            session_id: Some("session-intent".to_string()),
            tool_name: "Edit".to_string(),
            file_path: Some("/src/auth/token.rs".to_string()),
            operation: Some("modify".to_string()),
            diff_summary: None,
            raw_input: None,
            ai_summary: None,
            git_commit_sha: None,
            metadata: None,
        };

        // Find intent
        let conn = db.conn.lock().unwrap();
        let intent = find_intent_for_event(&conn, &event, 5);

        assert!(intent.is_some());
        assert_eq!(intent.unwrap(), "Fix the 401 errors on page refresh.");
    }

    #[test]
    fn test_find_intent_no_session_returns_none() {
        let db = Database::open(PathBuf::from(":memory:")).unwrap();

        // Event without session_id
        let event = StoredEvent {
            id: 1,
            timestamp: "2026-01-10T12:05:00Z".to_string(),
            timestamp_display: None,
            session_id: None, // No session
            tool_name: "Edit".to_string(),
            file_path: Some("/src/main.rs".to_string()),
            operation: Some("modify".to_string()),
            diff_summary: None,
            raw_input: None,
            ai_summary: None,
            git_commit_sha: None,
            metadata: None,
        };

        let conn = db.conn.lock().unwrap();
        let intent = find_intent_for_event(&conn, &event, 5);

        assert!(intent.is_none());
    }

    #[test]
    fn test_find_intent_no_exchanges_returns_none() {
        let db = Database::open(PathBuf::from(":memory:")).unwrap();

        // Event with session but no exchanges in DB
        let event = StoredEvent {
            id: 1,
            timestamp: "2026-01-10T12:05:00Z".to_string(),
            timestamp_display: None,
            session_id: Some("orphan-session".to_string()),
            tool_name: "Edit".to_string(),
            file_path: Some("/src/main.rs".to_string()),
            operation: Some("modify".to_string()),
            diff_summary: None,
            raw_input: None,
            ai_summary: None,
            git_commit_sha: None,
            metadata: None,
        };

        let conn = db.conn.lock().unwrap();
        let intent = find_intent_for_event(&conn, &event, 5);

        assert!(intent.is_none());
    }
}
