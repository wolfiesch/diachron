//! Database operations for the daemon
//!
//! Handles event storage and queries for the unified database.
//!
//! Uses a mutex-wrapped connection for thread-safe access in async context.

use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use tracing::debug;

use diachron_core::{CaptureEvent, Exchange, StoredEvent};

/// Database handle for the daemon.
///
/// The connection is wrapped in a `Mutex` because `rusqlite::Connection`
/// is not `Send`/`Sync`, but we share it across async tasks.
pub struct Database {
    /// Path to the database file
    path: PathBuf,
    /// Thread-safe connection wrapper
    conn: Mutex<Connection>,
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

        Ok(Self {
            path,
            conn: Mutex::new(conn),
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

    /// Save a capture event to the database.
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
        conn.execute(
            "INSERT INTO events (
                timestamp, timestamp_display, session_id, tool_name, file_path,
                operation, diff_summary, raw_input, git_commit_sha, metadata, embedding
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
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
}
