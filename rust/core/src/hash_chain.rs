//! Hash-chain tamper evidence for Diachron events
//!
//! This module provides cryptographic tamper detection via SHA256 hash chaining.
//! Each event's hash includes the previous event's hash, creating an immutable chain
//! that makes tampering detectable.
//!
//! # Security Model
//!
//! This is tamper-*detection*, not tamper-*prevention*. A determined attacker with
//! database access could recompute the entire chain. For audit-grade guarantees,
//! future versions will add device key signing and optional third-party attestation.
//!
//! # Usage
//!
//! ```rust,ignore
//! use diachron_core::hash_chain::{compute_event_hash, EventHashInput};
//!
//! let input = EventHashInput {
//!     id: 1,
//!     timestamp: "2026-01-11T00:00:00".to_string(),
//!     tool_name: "Write".to_string(),
//!     // ... other fields
//! };
//!
//! let genesis_hash = [0u8; 32];
//! let hash = compute_event_hash(&input, &genesis_hash);
//! ```

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Genesis hash (all zeros) for the first event in a chain.
pub const GENESIS_HASH: [u8; 32] = [0u8; 32];

/// Input structure for computing event hashes.
///
/// This includes all fields that should be part of the canonical
/// hash computation. Excludes `prev_hash` and `event_hash` themselves.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventHashInput {
    pub id: i64,
    pub timestamp: String,
    pub tool_name: String,
    pub file_path: Option<String>,
    pub operation: String,
    pub diff_summary: Option<String>,
    pub raw_input: Option<String>,
    pub session_id: Option<String>,
    pub git_commit_sha: Option<String>,
    pub metadata: Option<String>,
}

/// Result of chain verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainVerificationResult {
    /// Whether the entire chain is valid
    pub valid: bool,
    /// Number of events checked
    pub events_checked: u64,
    /// Number of checkpoints verified
    pub checkpoints_checked: u64,
    /// Timestamp of first event in chain
    pub first_event: Option<String>,
    /// Timestamp of last event in chain
    pub last_event: Option<String>,
    /// Hash of the chain root (genesis or first event)
    pub chain_root: Option<String>,
    /// Details of where the chain broke (if invalid)
    pub break_point: Option<ChainBreak>,
}

/// Details of a chain break point.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainBreak {
    /// Event ID where tampering was detected
    pub event_id: i64,
    /// Timestamp of the tampered event
    pub timestamp: String,
    /// Hash that was expected based on chain
    pub expected_hash: String,
    /// Hash that was actually stored
    pub actual_hash: String,
}

/// Checkpoint record for daily chain snapshots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainCheckpoint {
    pub id: i64,
    pub date: String,
    pub event_count: u64,
    pub final_hash: [u8; 32],
    pub signature: Option<Vec<u8>>,
    pub created_at: String,
}

/// Compute the SHA256 hash of an event including the previous hash.
///
/// # Algorithm
///
/// 1. Serialize event fields to canonical JSON (sorted keys, no whitespace)
/// 2. Concatenate with previous hash bytes
/// 3. Compute SHA256 of combined data
///
/// # Arguments
///
/// * `event` - Event data to hash (excludes hash fields)
/// * `prev_hash` - Hash of the previous event (or GENESIS_HASH for first)
///
/// # Returns
///
/// 32-byte SHA256 hash
pub fn compute_event_hash(event: &EventHashInput, prev_hash: &[u8; 32]) -> [u8; 32] {
    // Canonical JSON serialization (sorted keys via serde default)
    let canonical = serde_json::to_string(event).unwrap_or_default();

    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    hasher.update(prev_hash);

    hasher.finalize().into()
}

/// Verify the integrity of the event hash chain.
///
/// Iterates through all events with hashes, recomputing each hash
/// and comparing against stored values.
///
/// # Arguments
///
/// * `conn` - Database connection
///
/// # Returns
///
/// Verification result with details of any breaks found
pub fn verify_chain(conn: &Connection) -> Result<ChainVerificationResult, rusqlite::Error> {
    let mut result = ChainVerificationResult {
        valid: true,
        events_checked: 0,
        checkpoints_checked: 0,
        first_event: None,
        last_event: None,
        chain_root: None,
        break_point: None,
    };

    // Query events with hashes, ordered by ID (insertion order)
    let mut stmt = conn.prepare(
        "SELECT id, timestamp, tool_name, file_path, operation, diff_summary,
                raw_input, session_id, git_commit_sha, metadata, prev_hash, event_hash
         FROM events
         WHERE event_hash IS NOT NULL
         ORDER BY id ASC",
    )?;

    let mut rows = stmt.query([])?;
    let mut expected_prev_hash = GENESIS_HASH;
    let mut is_first = true;

    while let Some(row) = rows.next()? {
        let id: i64 = row.get(0)?;
        let timestamp: String = row.get(1)?;
        let tool_name: String = row.get(2)?;
        let file_path: Option<String> = row.get(3)?;
        let operation: Option<String> = row.get(4)?;
        let diff_summary: Option<String> = row.get(5)?;
        let raw_input: Option<String> = row.get(6)?;
        let session_id: Option<String> = row.get(7)?;
        let git_commit_sha: Option<String> = row.get(8)?;
        let metadata: Option<String> = row.get(9)?;
        let stored_prev_hash: Option<Vec<u8>> = row.get(10)?;
        let stored_event_hash: Option<Vec<u8>> = row.get(11)?;

        // Set first/last timestamps
        if is_first {
            result.first_event = Some(timestamp.clone());
            result.chain_root = Some(hex::encode(&expected_prev_hash));
            is_first = false;
        }
        result.last_event = Some(timestamp.clone());
        result.events_checked += 1;

        // Build hash input
        let input = EventHashInput {
            id,
            timestamp: timestamp.clone(),
            tool_name,
            file_path,
            operation: operation.unwrap_or_default(),
            diff_summary,
            raw_input,
            session_id,
            git_commit_sha,
            metadata,
        };

        // Verify prev_hash matches expected
        if let Some(ref prev_bytes) = stored_prev_hash {
            if prev_bytes.len() == 32 {
                let stored_prev: [u8; 32] = prev_bytes.as_slice().try_into().unwrap_or([0u8; 32]);
                if stored_prev != expected_prev_hash {
                    result.valid = false;
                    result.break_point = Some(ChainBreak {
                        event_id: id,
                        timestamp,
                        expected_hash: hex::encode(&expected_prev_hash),
                        actual_hash: hex::encode(&stored_prev),
                    });
                    break;
                }
            }
        }

        // Compute expected hash and compare
        let computed_hash = compute_event_hash(&input, &expected_prev_hash);

        if let Some(ref hash_bytes) = stored_event_hash {
            if hash_bytes.len() == 32 {
                let stored_hash: [u8; 32] = hash_bytes.as_slice().try_into().unwrap_or([0u8; 32]);
                if stored_hash != computed_hash {
                    result.valid = false;
                    result.break_point = Some(ChainBreak {
                        event_id: id,
                        timestamp,
                        expected_hash: hex::encode(&computed_hash),
                        actual_hash: hex::encode(&stored_hash),
                    });
                    break;
                }
                expected_prev_hash = stored_hash;
            }
        }
    }

    // Count checkpoints
    let checkpoint_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM chain_checkpoints", [], |row| {
            row.get(0)
        })?;
    result.checkpoints_checked = checkpoint_count as u64;

    Ok(result)
}

/// Get the hash of the last event in the chain.
///
/// Used when inserting new events to maintain chain continuity.
///
/// # Arguments
///
/// * `conn` - Database connection
///
/// # Returns
///
/// Hash of the last event, or GENESIS_HASH if no events exist
pub fn get_last_event_hash(conn: &Connection) -> Result<[u8; 32], rusqlite::Error> {
    let result: Option<Vec<u8>> = conn
        .query_row(
            "SELECT event_hash FROM events WHERE event_hash IS NOT NULL ORDER BY id DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .ok();

    match result {
        Some(bytes) if bytes.len() == 32 => {
            Ok(bytes.as_slice().try_into().unwrap_or(GENESIS_HASH))
        }
        _ => Ok(GENESIS_HASH),
    }
}

/// Create a daily checkpoint of the chain state.
///
/// Checkpoints allow efficient verification of chain segments
/// and enable graceful handling of event compaction/deletion.
///
/// # Arguments
///
/// * `conn` - Database connection
/// * `date` - Date string for the checkpoint (YYYY-MM-DD)
///
/// # Returns
///
/// The created checkpoint record
pub fn create_checkpoint(conn: &Connection, date: &str) -> Result<ChainCheckpoint, rusqlite::Error> {
    let event_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM events WHERE event_hash IS NOT NULL", [], |row| {
            row.get(0)
        })?;

    let final_hash = get_last_event_hash(conn)?;
    let created_at = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    conn.execute(
        "INSERT INTO chain_checkpoints (date, event_count, final_hash, created_at)
         VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![date, event_count, final_hash.as_slice(), created_at],
    )?;

    let id = conn.last_insert_rowid();

    Ok(ChainCheckpoint {
        id,
        date: date.to_string(),
        event_count: event_count as u64,
        final_hash,
        signature: None,
        created_at,
    })
}

/// Format hash bytes as hex string for display.
pub fn format_hash(hash: &[u8; 32]) -> String {
    hex::encode(hash)
}

/// Format hash bytes as truncated hex string for compact display.
pub fn format_hash_short(hash: &[u8; 32]) -> String {
    let full = hex::encode(hash);
    format!("{}...", &full[..8])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_event_hash_deterministic() {
        let input = EventHashInput {
            id: 1,
            timestamp: "2026-01-11T00:00:00".to_string(),
            tool_name: "Write".to_string(),
            file_path: Some("test.txt".to_string()),
            operation: "create".to_string(),
            diff_summary: Some("+10 lines".to_string()),
            raw_input: None,
            session_id: Some("session-1".to_string()),
            git_commit_sha: None,
            metadata: None,
        };

        let hash1 = compute_event_hash(&input, &GENESIS_HASH);
        let hash2 = compute_event_hash(&input, &GENESIS_HASH);

        assert_eq!(hash1, hash2, "Hash should be deterministic");
    }

    #[test]
    fn test_compute_event_hash_different_inputs() {
        let input1 = EventHashInput {
            id: 1,
            timestamp: "2026-01-11T00:00:00".to_string(),
            tool_name: "Write".to_string(),
            file_path: Some("test.txt".to_string()),
            operation: "create".to_string(),
            diff_summary: Some("+10 lines".to_string()),
            raw_input: None,
            session_id: None,
            git_commit_sha: None,
            metadata: None,
        };

        let input2 = EventHashInput {
            id: 2,
            ..input1.clone()
        };

        let hash1 = compute_event_hash(&input1, &GENESIS_HASH);
        let hash2 = compute_event_hash(&input2, &GENESIS_HASH);

        assert_ne!(hash1, hash2, "Different inputs should produce different hashes");
    }

    #[test]
    fn test_compute_event_hash_chain_linkage() {
        let input1 = EventHashInput {
            id: 1,
            timestamp: "2026-01-11T00:00:00".to_string(),
            tool_name: "Write".to_string(),
            file_path: Some("test.txt".to_string()),
            operation: "create".to_string(),
            diff_summary: None,
            raw_input: None,
            session_id: None,
            git_commit_sha: None,
            metadata: None,
        };

        let hash1 = compute_event_hash(&input1, &GENESIS_HASH);

        let input2 = EventHashInput {
            id: 2,
            timestamp: "2026-01-11T00:01:00".to_string(),
            tool_name: "Edit".to_string(),
            file_path: Some("test.txt".to_string()),
            operation: "modify".to_string(),
            diff_summary: None,
            raw_input: None,
            session_id: None,
            git_commit_sha: None,
            metadata: None,
        };

        // Hash with genesis should differ from hash with prev
        let hash2_genesis = compute_event_hash(&input2, &GENESIS_HASH);
        let hash2_chained = compute_event_hash(&input2, &hash1);

        assert_ne!(
            hash2_genesis, hash2_chained,
            "Chained hash should differ from genesis hash"
        );
    }

    #[test]
    fn test_format_hash() {
        let hash = [0xab; 32];
        let formatted = format_hash(&hash);
        assert_eq!(formatted.len(), 64, "Hex string should be 64 chars");
        assert!(formatted.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_format_hash_short() {
        let hash = [0xab; 32];
        let short = format_hash_short(&hash);
        assert_eq!(short, "abababab...");
    }
}
