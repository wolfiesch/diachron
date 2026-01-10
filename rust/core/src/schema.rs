//! Database schema and migrations for Diachron
//!
//! Manages the unified SQLite database with:
//! - events: Code change tracking (existing Diachron functionality)
//! - exchanges: Conversation memory (migrated from episodic-memory)
//! - FTS5 indexes for full-text search

use rusqlite::Connection;

use crate::error::Result;

/// Current schema version
pub const SCHEMA_VERSION: i32 = 2;

/// Initialize or migrate the database schema
pub fn init_schema(conn: &Connection) -> Result<()> {
    let version = get_schema_version(conn)?;

    if version < 1 {
        migrate_v1(conn)?;
    }
    if version < 2 {
        migrate_v2(conn)?;
    }

    Ok(())
}

fn get_schema_version(conn: &Connection) -> Result<i32> {
    // Create version table if not exists
    conn.execute(
        "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER PRIMARY KEY)",
        [],
    )?;

    let version: i32 = conn
        .query_row("SELECT COALESCE(MAX(version), 0) FROM schema_version", [], |row| {
            row.get(0)
        })?;

    Ok(version)
}

fn set_schema_version(conn: &Connection, version: i32) -> Result<()> {
    conn.execute("INSERT INTO schema_version (version) VALUES (?1)", [version])?;
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
