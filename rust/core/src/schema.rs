//! Database schema and migrations for Diachron
//!
//! Manages the unified SQLite database with:
//! - events: Code change tracking (existing Diachron functionality)
//! - exchanges: Conversation memory (migrated from episodic-memory)
//! - FTS5 indexes for full-text search

use rusqlite::Connection;

use crate::error::Result;

/// Current schema version.
pub const SCHEMA_VERSION: i32 = 4;

/// Initialize or migrate the database schema.
///
/// # Arguments
/// - `conn`: Open SQLite connection for the database.
///
/// # Errors
/// Returns `Error` if schema queries or migrations fail.
pub fn init_schema(conn: &Connection) -> Result<()> {
    let version = get_schema_version(conn)?;

    if version < 1 {
        migrate_v1(conn)?;
    }
    if version < 2 {
        migrate_v2(conn)?;
    }
    if version < 3 {
        migrate_v3(conn)?;
    }
    if version < 4 {
        migrate_v4(conn)?;
    }

    Ok(())
}

fn get_schema_version(conn: &Connection) -> Result<i32> {
    // Create version table if not exists
    conn.execute(
        "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER PRIMARY KEY)",
        [],
    )?;

    let version: i32 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_version",
        [],
        |row| row.get(0),
    )?;

    Ok(version)
}

fn set_schema_version(conn: &Connection, version: i32) -> Result<()> {
    conn.execute(
        "INSERT INTO schema_version (version) VALUES (?1)",
        [version],
    )?;
    Ok(())
}

/// V1: Original events table (existing Diachron schema)
fn migrate_v1(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS events (
            id INTEGER PRIMARY KEY,
            timestamp TEXT NOT NULL,
            timestamp_display TEXT,
            session_id TEXT,
            tool_name TEXT NOT NULL,
            file_path TEXT,
            operation TEXT,
            diff_summary TEXT,
            raw_input TEXT,
            ai_summary TEXT,
            git_commit_sha TEXT,
            parent_event_id INTEGER,
            metadata TEXT,
            embedding BLOB,
            FOREIGN KEY (parent_event_id) REFERENCES events(id)
        );

        CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(timestamp);
        CREATE INDEX IF NOT EXISTS idx_events_file_path ON events(file_path);
        CREATE INDEX IF NOT EXISTS idx_events_session_id ON events(session_id);
        CREATE INDEX IF NOT EXISTS idx_events_tool_name ON events(tool_name);",
    )?;

    set_schema_version(conn, 1)?;
    Ok(())
}

/// V2: Add exchanges table for conversation memory
fn migrate_v2(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS exchanges (
            id TEXT PRIMARY KEY,
            timestamp TEXT NOT NULL,
            project TEXT,
            session_id TEXT,
            user_message TEXT,
            assistant_message TEXT,
            tool_calls TEXT,
            archive_path TEXT,
            line_start INTEGER,
            line_end INTEGER,
            embedding BLOB,
            summary TEXT,
            git_branch TEXT,
            cwd TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_exchanges_timestamp ON exchanges(timestamp DESC);
        CREATE INDEX IF NOT EXISTS idx_exchanges_project ON exchanges(project);
        CREATE INDEX IF NOT EXISTS idx_exchanges_session_id ON exchanges(session_id);

        -- Full-text search for events
        CREATE VIRTUAL TABLE IF NOT EXISTS events_fts USING fts5(
            tool_name,
            operation,
            diff_summary,
            raw_input,
            content=events,
            content_rowid=id
        );

        -- Full-text search for exchanges
        CREATE VIRTUAL TABLE IF NOT EXISTS exchanges_fts USING fts5(
            user_message,
            assistant_message,
            summary,
            content=exchanges,
            content_rowid=rowid
        );",
    )?;

    set_schema_version(conn, 2)?;
    Ok(())
}

/// V3: Add FTS triggers and project_path column
fn migrate_v3(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "-- Add project_path column to events if not exists
        ALTER TABLE events ADD COLUMN project_path TEXT;

        -- Create FTS sync triggers for events
        CREATE TRIGGER IF NOT EXISTS events_fts_insert AFTER INSERT ON events BEGIN
            INSERT INTO events_fts(rowid, tool_name, operation, diff_summary, raw_input)
            VALUES (new.id, new.tool_name, new.operation, new.diff_summary, new.raw_input);
        END;

        CREATE TRIGGER IF NOT EXISTS events_fts_update AFTER UPDATE ON events BEGIN
            DELETE FROM events_fts WHERE rowid = old.id;
            INSERT INTO events_fts(rowid, tool_name, operation, diff_summary, raw_input)
            VALUES (new.id, new.tool_name, new.operation, new.diff_summary, new.raw_input);
        END;

        CREATE TRIGGER IF NOT EXISTS events_fts_delete AFTER DELETE ON events BEGIN
            DELETE FROM events_fts WHERE rowid = old.id;
        END;

        -- Create FTS sync triggers for exchanges
        CREATE TRIGGER IF NOT EXISTS exchanges_fts_insert AFTER INSERT ON exchanges BEGIN
            INSERT INTO exchanges_fts(rowid, user_message, assistant_message, summary)
            VALUES (new.rowid, new.user_message, new.assistant_message, new.summary);
        END;

        CREATE TRIGGER IF NOT EXISTS exchanges_fts_update AFTER UPDATE ON exchanges BEGIN
            DELETE FROM exchanges_fts WHERE rowid = old.rowid;
            INSERT INTO exchanges_fts(rowid, user_message, assistant_message, summary)
            VALUES (new.rowid, new.user_message, new.assistant_message, new.summary);
        END;

        CREATE TRIGGER IF NOT EXISTS exchanges_fts_delete AFTER DELETE ON exchanges BEGIN
            DELETE FROM exchanges_fts WHERE rowid = old.rowid;
        END;

        -- Create project_path index
        CREATE INDEX IF NOT EXISTS idx_events_project_path ON events(project_path);",
    )?;

    set_schema_version(conn, 3)?;
    Ok(())
}

/// V4: Add hash-chain tamper evidence and content fingerprinting
///
/// This migration adds:
/// - Hash chain columns (prev_hash, event_hash) for tamper detection
/// - Content fingerprint columns (content_hash, context_hash) for stable blame
/// - Chain checkpoints table for daily integrity snapshots
fn migrate_v4(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "-- Hash chain columns for tamper-evidence
        ALTER TABLE events ADD COLUMN prev_hash BLOB;
        ALTER TABLE events ADD COLUMN event_hash BLOB;
        CREATE INDEX IF NOT EXISTS idx_events_hash ON events(event_hash);

        -- Content fingerprint columns for stable blame
        ALTER TABLE events ADD COLUMN content_hash BLOB;
        ALTER TABLE events ADD COLUMN context_hash BLOB;
        CREATE INDEX IF NOT EXISTS idx_events_content_hash ON events(content_hash);

        -- Chain checkpoints table for daily integrity snapshots
        CREATE TABLE IF NOT EXISTS chain_checkpoints (
            id INTEGER PRIMARY KEY,
            date TEXT NOT NULL,
            event_count INTEGER NOT NULL,
            final_hash BLOB NOT NULL,
            signature BLOB,
            created_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_checkpoints_date ON chain_checkpoints(date);",
    )?;

    set_schema_version(conn, 4)?;
    Ok(())
}

/// Full-text search for events.
///
/// # Arguments
/// - `conn`: Open SQLite connection for the database.
/// - `query`: FTS5 query string.
/// - `limit`: Maximum number of results to return.
///
/// # Returns
/// Vector of search results ordered by BM25 score.
///
/// # Errors
/// Returns `Error` if query preparation or execution fails.
pub fn fts_search_events(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> Result<Vec<FtsSearchResult>> {
    let mut stmt = conn.prepare(
        "SELECT e.id, e.timestamp, e.file_path, e.tool_name,
                snippet(events_fts, 2, '<b>', '</b>', '...', 32) as snippet,
                bm25(events_fts) as score
         FROM events_fts
         JOIN events e ON events_fts.rowid = e.id
         WHERE events_fts MATCH ?1
         ORDER BY bm25(events_fts)
         LIMIT ?2",
    )?;

    let results = stmt
        .query_map([query, &limit.to_string()], |row| {
            Ok(FtsSearchResult {
                id: row.get::<_, i64>(0)?.to_string(),
                timestamp: row.get(1)?,
                context: row.get::<_, Option<String>>(2)?,
                source_type: "event".to_string(),
                snippet: row.get(4)?,
                score: row.get(5)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(results)
}

/// Full-text search for exchanges.
///
/// # Arguments
/// - `conn`: Open SQLite connection for the database.
/// - `query`: FTS5 query string.
/// - `limit`: Maximum number of results to return.
///
/// # Returns
/// Vector of search results ordered by BM25 score.
///
/// # Errors
/// Returns `Error` if query preparation or execution fails.
pub fn fts_search_exchanges(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> Result<Vec<FtsSearchResult>> {
    let mut stmt = conn.prepare(
        "SELECT e.id, e.timestamp, e.project,
                snippet(exchanges_fts, 0, '<b>', '</b>', '...', 64) as snippet,
                bm25(exchanges_fts) as score
         FROM exchanges_fts
         JOIN exchanges e ON exchanges_fts.rowid = e.rowid
         WHERE exchanges_fts MATCH ?1
         ORDER BY bm25(exchanges_fts)
         LIMIT ?2",
    )?;

    let results = stmt
        .query_map([query, &limit.to_string()], |row| {
            Ok(FtsSearchResult {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                context: row.get::<_, Option<String>>(2)?,
                source_type: "exchange".to_string(),
                snippet: row.get(3)?,
                score: row.get(4)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(results)
}

/// Result from FTS search.
///
/// # Fields
/// - `id`: Event or exchange identifier.
/// - `timestamp`: ISO timestamp string.
/// - `context`: File path (events) or project name (exchanges).
/// - `source_type`: Source label ("event" or "exchange").
/// - `snippet`: Highlighted snippet for display.
/// - `score`: BM25 score (lower is better).
#[derive(Debug, Clone)]
pub struct FtsSearchResult {
    pub id: String,
    pub timestamp: String,
    pub context: Option<String>, // file_path for events, project for exchanges
    pub source_type: String,     // "event" or "exchange"
    pub snippet: String,
    pub score: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_init() {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();

        // Verify tables exist
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table'")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        assert!(tables.contains(&"events".to_string()));
        assert!(tables.contains(&"exchanges".to_string()));
        assert!(tables.contains(&"schema_version".to_string()));
    }
}
