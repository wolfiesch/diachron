//! Database operations for the daemon
//!
//! Handles event storage and queries for the unified database.

use std::path::PathBuf;

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use tracing::debug;

use diachron_core::{CaptureEvent, StoredEvent};

/// Database handle for the daemon
pub struct Database {
    /// Path to the database file
    path: PathBuf,
}

impl Database {
    /// Open or create a database at the given path
    pub fn open(path: PathBuf) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        // Initialize schema
        let conn = Connection::open(&path).context("Failed to open database")?;
        diachron_core::schema::init_schema(&conn).context("Failed to initialize schema")?;

        Ok(Self { path })
    }

    /// Get a new connection (connections are not thread-safe)
    fn conn(&self) -> rusqlite::Result<Connection> {
        Connection::open(&self.path)
    }

    /// Save a capture event to the database
    pub fn save_event(
        &self,
        event: &CaptureEvent,
        session_id: Option<&str>,
        project_path: Option<&str>,
    ) -> rusqlite::Result<i64> {
        let conn = self.conn()?;

        let timestamp = chrono::Local::now();
        let timestamp_iso = timestamp.format("%Y-%m-%dT%H:%M:%S%.3f").to_string();

        // Determine PST/PDT
        let tz_name = if timestamp.format("%Z").to_string().contains("DT") {
            "PDT"
        } else {
            "PST"
        };
        let timestamp_display = timestamp.format(&format!("%m/%d/%Y %I:%M %p {}", tz_name)).to_string();

        // Build metadata JSON
        let metadata = serde_json::json!({
            "command_category": event.command_category.as_ref().map(|c| c.as_str()),
            "project_path": project_path,
        });

        conn.execute(
            "INSERT INTO events (
                timestamp, timestamp_display, session_id, tool_name, file_path,
                operation, diff_summary, raw_input, git_commit_sha, metadata
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
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
            ],
        )?;

        Ok(conn.last_insert_rowid())
    }

    /// Query events with optional filters
    pub fn query_events(
        &self,
        since: Option<&str>,
        file_filter: Option<&str>,
        limit: usize,
    ) -> rusqlite::Result<Vec<StoredEvent>> {
        let conn = self.conn()?;

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

    /// Get event count
    pub fn event_count(&self) -> rusqlite::Result<u64> {
        let conn = self.conn()?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))?;
        Ok(count as u64)
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

        let id = db.save_event(&event, Some("test-session"), None).unwrap();
        assert!(id > 0);

        let events = db.query_events(None, None, 10).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].tool_name, "Write");
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
