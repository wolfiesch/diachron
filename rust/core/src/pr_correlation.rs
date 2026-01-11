//! PR correlation: linking events to commits to pull requests
//!
//! This module correlates captured Diachron events with git commits
//! and pull requests to build evidence trails.
//!
//! # Correlation Strategy
//!
//! 1. **Direct match**: Event has `git_commit_sha` matching a PR commit (HIGH confidence)
//! 2. **Session match**: Events in same session as a commit event (MEDIUM confidence)
//! 3. **Time match**: Events within 5min before commit, same branch (LOW confidence)
//!
//! # Usage
//!
//! ```rust,ignore
//! use diachron_core::pr_correlation::{correlate_events_to_pr, PREvidence};
//!
//! let evidence = correlate_events_to_pr(
//!     &conn,
//!     &["abc123", "def456"],  // Commit SHAs from PR
//!     "feat/auth",            // Branch name
//!     (start_time, end_time), // Time window
//! )?;
//!
//! println!("Coverage: {:.1}%", evidence.coverage_pct);
//! ```

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::types::StoredEvent;

/// Time window for event-commit matching (in seconds)
pub const DEFAULT_TIME_WINDOW_SECS: i64 = 300; // 5 minutes

/// Evidence gathered for a pull request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PREvidence {
    /// PR identifier (number)
    pub pr_id: u64,
    /// Branch name
    pub branch: String,
    /// Evidence grouped by commit
    pub commits: Vec<CommitEvidence>,
    /// Events that couldn't be matched to any commit
    pub unmatched_events: Vec<StoredEvent>,
    /// Percentage of events successfully matched to commits
    pub coverage_pct: f32,
    /// Total number of events considered
    pub total_events: u64,
}

/// Evidence for a single commit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitEvidence {
    /// Git commit SHA
    pub sha: String,
    /// Commit message (if available)
    pub message: Option<String>,
    /// Events linked to this commit
    pub events: Vec<StoredEvent>,
    /// Confidence of the event-commit linkage
    pub confidence: MatchConfidence,
}

/// Confidence level of event-commit matching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatchConfidence {
    /// Direct `git_commit_sha` linkage
    High,
    /// Session-based correlation
    Medium,
    /// Time-window correlation
    Low,
}

impl MatchConfidence {
    /// Return string representation for display.
    pub fn as_str(&self) -> &'static str {
        match self {
            MatchConfidence::High => "HIGH",
            MatchConfidence::Medium => "MEDIUM",
            MatchConfidence::Low => "LOW",
        }
    }
}

/// Correlate events to pull request commits.
///
/// # Arguments
///
/// * `conn` - Database connection
/// * `pr_id` - Pull request number
/// * `pr_commits` - List of commit SHAs in the PR
/// * `branch` - Branch name for the PR
/// * `start_time` - Start of time window (ISO timestamp)
/// * `end_time` - End of time window (ISO timestamp)
///
/// # Returns
///
/// Evidence pack with correlated events
pub fn correlate_events_to_pr(
    conn: &Connection,
    pr_id: u64,
    pr_commits: &[String],
    branch: &str,
    start_time: &str,
    end_time: &str,
) -> Result<PREvidence, rusqlite::Error> {
    let mut commit_evidence: Vec<CommitEvidence> = Vec::new();
    let mut unmatched_events: Vec<StoredEvent> = Vec::new();
    let mut matched_event_ids: std::collections::HashSet<i64> = std::collections::HashSet::new();

    // 1. Query all events in the time window
    let all_events = query_events_in_window(conn, start_time, end_time)?;
    let total_events = all_events.len() as u64;

    // 2. For each commit, find matching events
    for commit_sha in pr_commits {
        let mut commit_events: Vec<StoredEvent> = Vec::new();
        let mut confidence = MatchConfidence::Low;

        // 2a. HIGH confidence: Direct git_commit_sha match
        let direct_matches: Vec<StoredEvent> = all_events
            .iter()
            .filter(|e| {
                e.git_commit_sha
                    .as_ref()
                    .map(|sha| sha == commit_sha)
                    .unwrap_or(false)
            })
            .cloned()
            .collect();

        if !direct_matches.is_empty() {
            confidence = MatchConfidence::High;
            for event in &direct_matches {
                if matched_event_ids.insert(event.id) {
                    commit_events.push(event.clone());
                }
            }
        }

        // 2b. MEDIUM confidence: Same session as commit event
        // Find the session_id of the commit event
        if let Some(commit_event) = direct_matches.first() {
            if let Some(ref session_id) = commit_event.session_id {
                let session_matches: Vec<StoredEvent> = all_events
                    .iter()
                    .filter(|e| {
                        e.session_id
                            .as_ref()
                            .map(|sid| sid == session_id)
                            .unwrap_or(false)
                            && !matched_event_ids.contains(&e.id)
                    })
                    .cloned()
                    .collect();

                for event in session_matches {
                    if matched_event_ids.insert(event.id) {
                        commit_events.push(event);
                        if confidence == MatchConfidence::Low {
                            confidence = MatchConfidence::Medium;
                        }
                    }
                }
            }
        }

        // 2c. LOW confidence: Time-based matching
        // Find commit timestamp and match events within window
        let commit_timestamp = get_commit_timestamp(conn, commit_sha);
        if let Some(commit_ts) = commit_timestamp {
            let time_matches: Vec<StoredEvent> = all_events
                .iter()
                .filter(|e| {
                    !matched_event_ids.contains(&e.id)
                        && is_within_time_window(&e.timestamp, &commit_ts, DEFAULT_TIME_WINDOW_SECS)
                        && matches_branch(e, branch)
                })
                .cloned()
                .collect();

            for event in time_matches {
                if matched_event_ids.insert(event.id) {
                    commit_events.push(event);
                }
            }
        }

        if !commit_events.is_empty() {
            commit_evidence.push(CommitEvidence {
                sha: commit_sha.clone(),
                message: get_commit_message(conn, commit_sha),
                events: commit_events,
                confidence,
            });
        }
    }

    // 3. Collect unmatched events
    for event in all_events {
        if !matched_event_ids.contains(&event.id) {
            unmatched_events.push(event);
        }
    }

    // 4. Calculate coverage
    let matched_count = matched_event_ids.len() as f32;
    let coverage_pct = if total_events > 0 {
        (matched_count / total_events as f32) * 100.0
    } else {
        100.0
    };

    Ok(PREvidence {
        pr_id,
        branch: branch.to_string(),
        commits: commit_evidence,
        unmatched_events,
        coverage_pct,
        total_events,
    })
}

/// Query events within a time window.
fn query_events_in_window(
    conn: &Connection,
    start_time: &str,
    end_time: &str,
) -> Result<Vec<StoredEvent>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, timestamp, timestamp_display, session_id, tool_name, file_path,
                operation, diff_summary, raw_input, ai_summary, git_commit_sha, metadata
         FROM events
         WHERE timestamp >= ?1 AND timestamp <= ?2
         ORDER BY timestamp ASC",
    )?;

    let events = stmt
        .query_map([start_time, end_time], |row| {
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

/// Get commit timestamp from event with matching SHA.
fn get_commit_timestamp(conn: &Connection, commit_sha: &str) -> Option<String> {
    conn.query_row(
        "SELECT timestamp FROM events WHERE git_commit_sha = ?1 LIMIT 1",
        [commit_sha],
        |row| row.get(0),
    )
    .ok()
}

/// Get commit message from event metadata.
fn get_commit_message(conn: &Connection, commit_sha: &str) -> Option<String> {
    let metadata: Option<String> = conn
        .query_row(
            "SELECT metadata FROM events WHERE git_commit_sha = ?1 LIMIT 1",
            [commit_sha],
            |row| row.get(0),
        )
        .ok()?;

    // Try to parse commit message from metadata
    metadata.and_then(|m| {
        serde_json::from_str::<serde_json::Value>(&m)
            .ok()
            .and_then(|v| v.get("commit_message").and_then(|m| m.as_str().map(String::from)))
    })
}

/// Check if event timestamp is within window of commit timestamp.
fn is_within_time_window(event_ts: &str, commit_ts: &str, window_secs: i64) -> bool {
    use chrono::NaiveDateTime;

    // Try parsing as NaiveDateTime (without timezone) first, then fallback to RFC3339
    let parse_timestamp = |ts: &str| -> Option<i64> {
        // Try various formats without timezone
        NaiveDateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%S%.3f")
            .or_else(|_| NaiveDateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%S"))
            .map(|dt| dt.and_utc().timestamp())
            .ok()
            .or_else(|| {
                // Try RFC3339 with timezone
                chrono::DateTime::parse_from_rfc3339(ts)
                    .map(|dt| dt.timestamp())
                    .ok()
            })
    };

    let event_secs = parse_timestamp(event_ts);
    let commit_secs = parse_timestamp(commit_ts);

    match (event_secs, commit_secs) {
        (Some(e), Some(c)) => {
            let diff = (c - e).abs();
            diff <= window_secs && e <= c // Event must be before or at commit
        }
        _ => false,
    }
}

/// Check if event metadata contains matching branch.
fn matches_branch(event: &StoredEvent, branch: &str) -> bool {
    event.metadata.as_ref().map_or(true, |m| {
        serde_json::from_str::<serde_json::Value>(m)
            .ok()
            .map_or(true, |v| {
                v.get("git_branch")
                    .and_then(|b| b.as_str())
                    .map_or(true, |b| b == branch || b.ends_with(branch))
            })
    })
}

/// Summary statistics for PR evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PRSummary {
    /// Number of files changed
    pub files_changed: usize,
    /// Total lines added
    pub lines_added: usize,
    /// Total lines removed
    pub lines_removed: usize,
    /// Number of tool operations
    pub tool_operations: usize,
    /// Unique sessions involved
    pub sessions: usize,
}

impl PREvidence {
    /// Generate summary statistics from evidence.
    pub fn summary(&self) -> PRSummary {
        let mut files: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut sessions: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut lines_added = 0;
        let mut lines_removed = 0;
        let mut tool_operations = 0;

        for commit in &self.commits {
            for event in &commit.events {
                tool_operations += 1;

                if let Some(ref path) = event.file_path {
                    files.insert(path.clone());
                }

                if let Some(ref session_id) = event.session_id {
                    sessions.insert(session_id.clone());
                }

                // Parse diff summary for line counts
                if let Some(ref diff) = event.diff_summary {
                    if let Some(added) = parse_line_count(diff, "+") {
                        lines_added += added;
                    }
                    if let Some(removed) = parse_line_count(diff, "-") {
                        lines_removed += removed;
                    }
                }
            }
        }

        PRSummary {
            files_changed: files.len(),
            lines_added,
            lines_removed,
            tool_operations,
            sessions: sessions.len(),
        }
    }
}

/// Parse line count from diff summary (e.g., "+45 lines" or "-10 lines").
fn parse_line_count(diff: &str, prefix: &str) -> Option<usize> {
    diff.split(',')
        .find(|s| s.trim().starts_with(prefix))
        .and_then(|s| {
            s.trim()
                .trim_start_matches(prefix)
                .split_whitespace()
                .next()
                .and_then(|n| n.parse().ok())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_match_confidence_as_str() {
        assert_eq!(MatchConfidence::High.as_str(), "HIGH");
        assert_eq!(MatchConfidence::Medium.as_str(), "MEDIUM");
        assert_eq!(MatchConfidence::Low.as_str(), "LOW");
    }

    #[test]
    fn test_parse_line_count() {
        assert_eq!(parse_line_count("+45 lines, -10 lines", "+"), Some(45));
        assert_eq!(parse_line_count("+45 lines, -10 lines", "-"), Some(10));
        assert_eq!(parse_line_count("+100 lines", "+"), Some(100));
        assert_eq!(parse_line_count("no changes", "+"), None);
    }

    #[test]
    fn test_is_within_time_window() {
        // Events within 5 minutes
        let event_ts = "2026-01-11T00:00:00.000";
        let commit_ts = "2026-01-11T00:04:00.000"; // 4 minutes later

        assert!(is_within_time_window(event_ts, commit_ts, 300));

        // Events too far apart
        let event_ts2 = "2026-01-11T00:00:00.000";
        let commit_ts2 = "2026-01-11T00:10:00.000"; // 10 minutes later

        assert!(!is_within_time_window(event_ts2, commit_ts2, 300));
    }

    #[test]
    fn test_matches_branch() {
        let event_with_branch = StoredEvent {
            id: 1,
            timestamp: "2026-01-11T00:00:00".to_string(),
            timestamp_display: None,
            session_id: None,
            tool_name: "Write".to_string(),
            file_path: None,
            operation: None,
            diff_summary: None,
            raw_input: None,
            ai_summary: None,
            git_commit_sha: None,
            metadata: Some(r#"{"git_branch": "feat/auth"}"#.to_string()),
        };

        assert!(matches_branch(&event_with_branch, "feat/auth"));
        assert!(!matches_branch(&event_with_branch, "main"));

        // Event without metadata should match any branch (permissive)
        let event_no_meta = StoredEvent {
            metadata: None,
            ..event_with_branch.clone()
        };

        assert!(matches_branch(&event_no_meta, "any/branch"));
    }
}
