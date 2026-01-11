//! Integration tests for Diachron v0.3 features
//!
//! These tests verify the complete flow:
//! - Hash chain integrity from event creation to verification
//! - PR correlation with mock commits
//! - Evidence pack generation and rendering

use diachron_core::{
    compute_event_hash, create_checkpoint, generate_evidence_pack, get_last_event_hash,
    render_markdown_narrative, verify_chain, ChainVerificationResult, EventHashInput,
    VerificationStatus, GENESIS_HASH,
};
use rusqlite::Connection;
use std::collections::HashSet;

/// Create an in-memory database with v4 schema for testing
fn create_test_db() -> Connection {
    let conn = Connection::open_in_memory().expect("Failed to create in-memory database");

    conn.execute_batch(
        "
        CREATE TABLE events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
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
            metadata TEXT,
            prev_hash BLOB,
            event_hash BLOB,
            content_hash BLOB,
            context_hash BLOB
        );
        CREATE INDEX idx_events_hash ON events(event_hash);
        CREATE INDEX idx_events_timestamp ON events(timestamp);
        CREATE INDEX idx_events_session ON events(session_id);

        CREATE TABLE chain_checkpoints (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            date TEXT NOT NULL,
            event_count INTEGER NOT NULL,
            final_hash BLOB NOT NULL,
            signature BLOB,
            created_at TEXT NOT NULL
        );

        CREATE TABLE schema_version (version INTEGER PRIMARY KEY);
        INSERT INTO schema_version VALUES (4);
        ",
    )
    .expect("Failed to create schema");

    conn
}

/// Insert a test event with hash chain
fn insert_test_event(
    conn: &Connection,
    timestamp: &str,
    tool_name: &str,
    file_path: Option<&str>,
    operation: &str,
    session_id: Option<&str>,
    git_commit_sha: Option<&str>,
) -> i64 {
    // Get next ID (same logic as daemon)
    let next_id: i64 = conn
        .query_row("SELECT COALESCE(MAX(id), 0) + 1 FROM events", [], |row| {
            row.get(0)
        })
        .unwrap_or(1);

    // Get previous hash
    let prev_hash = get_last_event_hash(conn).unwrap_or(GENESIS_HASH);

    // Create hash input
    let hash_input = EventHashInput {
        id: next_id,
        timestamp: timestamp.to_string(),
        tool_name: tool_name.to_string(),
        file_path: file_path.map(String::from),
        operation: operation.to_string(),
        diff_summary: Some("+10 lines".to_string()),
        raw_input: None,
        session_id: session_id.map(String::from),
        git_commit_sha: git_commit_sha.map(String::from),
        metadata: None,
    };

    // Compute hash
    let event_hash = compute_event_hash(&hash_input, &prev_hash);

    // Insert event
    conn.execute(
        "INSERT INTO events (timestamp, tool_name, file_path, operation, diff_summary,
                             session_id, git_commit_sha, prev_hash, event_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            timestamp,
            tool_name,
            file_path,
            operation,
            "+10 lines",
            session_id,
            git_commit_sha,
            prev_hash.as_slice(),
            event_hash.as_slice(),
        ],
    )
    .expect("Failed to insert event");

    next_id
}

#[test]
fn test_hash_chain_integrity() {
    let conn = create_test_db();

    // Insert a sequence of events
    insert_test_event(
        &conn,
        "2026-01-11T00:00:00.000",
        "Write",
        Some("src/auth.rs"),
        "create",
        Some("session-1"),
        None,
    );
    insert_test_event(
        &conn,
        "2026-01-11T00:01:00.000",
        "Edit",
        Some("src/auth.rs"),
        "modify",
        Some("session-1"),
        None,
    );
    insert_test_event(
        &conn,
        "2026-01-11T00:02:00.000",
        "Bash",
        None,
        "test",
        Some("session-1"),
        Some("abc123"),
    );

    // Verify chain
    let result = verify_chain(&conn).expect("Failed to verify chain");

    assert!(result.valid, "Hash chain should be valid");
    assert_eq!(result.events_checked, 3, "Should have checked 3 events");
    assert!(result.break_point.is_none(), "Should have no break point");
}

#[test]
fn test_hash_chain_detects_tampering() {
    let conn = create_test_db();

    // Insert events
    insert_test_event(
        &conn,
        "2026-01-11T00:00:00.000",
        "Write",
        Some("src/main.rs"),
        "create",
        Some("session-1"),
        None,
    );
    insert_test_event(
        &conn,
        "2026-01-11T00:01:00.000",
        "Edit",
        Some("src/main.rs"),
        "modify",
        Some("session-1"),
        None,
    );

    // Tamper with the first event
    conn.execute(
        "UPDATE events SET tool_name = 'TamperedWrite' WHERE id = 1",
        [],
    )
    .expect("Failed to tamper");

    // Verify chain - should detect tampering
    let result = verify_chain(&conn).expect("Failed to verify chain");

    assert!(!result.valid, "Hash chain should be invalid after tampering");
    assert!(
        result.break_point.is_some(),
        "Should have detected break point"
    );

    if let Some(break_point) = result.break_point {
        assert_eq!(break_point.event_id, 1, "Break should be at event 1");
    }
}

#[test]
fn test_checkpoint_creation() {
    let conn = create_test_db();

    // Insert events
    insert_test_event(
        &conn,
        "2026-01-11T00:00:00.000",
        "Write",
        Some("src/lib.rs"),
        "create",
        Some("session-1"),
        None,
    );
    insert_test_event(
        &conn,
        "2026-01-11T00:01:00.000",
        "Edit",
        Some("src/lib.rs"),
        "modify",
        Some("session-1"),
        None,
    );

    // Create checkpoint
    let checkpoint = create_checkpoint(&conn, "2026-01-11").expect("Failed to create checkpoint");

    assert_eq!(checkpoint.date, "2026-01-11");
    assert_eq!(checkpoint.event_count, 2);
    assert_ne!(checkpoint.final_hash, GENESIS_HASH);

    // Verify checkpoint was stored
    let stored_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM chain_checkpoints", [], |row| {
            row.get(0)
        })
        .expect("Failed to query checkpoints");

    assert_eq!(stored_count, 1, "Should have one checkpoint");
}

#[test]
fn test_pr_correlation_direct_match() {
    use diachron_core::pr_correlation::{correlate_events_to_pr, MatchConfidence};

    let conn = create_test_db();

    // Insert events with git_commit_sha (direct match)
    insert_test_event(
        &conn,
        "2026-01-11T00:00:00.000",
        "Write",
        Some("src/auth.rs"),
        "create",
        Some("session-1"),
        Some("abc123def456"),
    );
    insert_test_event(
        &conn,
        "2026-01-11T00:01:00.000",
        "Edit",
        Some("src/auth.rs"),
        "modify",
        Some("session-1"),
        Some("abc123def456"),
    );

    // Correlate to PR
    let evidence = correlate_events_to_pr(
        &conn,
        142,
        &["abc123def456".to_string()],
        "feat/auth",
        "2026-01-10T00:00:00.000",
        "2026-01-12T00:00:00.000",
    )
    .expect("Failed to correlate");

    assert_eq!(evidence.pr_id, 142);
    assert_eq!(evidence.commits.len(), 1);
    assert_eq!(evidence.commits[0].sha, "abc123def456");
    assert_eq!(evidence.commits[0].confidence, MatchConfidence::High);
    assert_eq!(evidence.commits[0].events.len(), 2);
    assert_eq!(evidence.coverage_pct, 100.0);
}

#[test]
fn test_pr_correlation_session_match() {
    use diachron_core::pr_correlation::{correlate_events_to_pr, MatchConfidence};

    let conn = create_test_db();

    // Insert events - first has git_commit_sha
    insert_test_event(
        &conn,
        "2026-01-11T00:00:00.000",
        "Write",
        Some("src/auth.rs"),
        "create",
        Some("session-42"),
        Some("commit123"),
    );
    // Second event same session, no commit sha
    insert_test_event(
        &conn,
        "2026-01-11T00:01:00.000",
        "Edit",
        Some("src/auth.rs"),
        "modify",
        Some("session-42"),
        None,
    );

    let evidence = correlate_events_to_pr(
        &conn,
        100,
        &["commit123".to_string()],
        "feat/auth",
        "2026-01-10T00:00:00.000",
        "2026-01-12T00:00:00.000",
    )
    .expect("Failed to correlate");

    // Both events should match via session
    assert_eq!(evidence.commits[0].events.len(), 2);
    assert!(
        evidence.commits[0].confidence == MatchConfidence::High
            || evidence.commits[0].confidence == MatchConfidence::Medium
    );
}

#[test]
fn test_evidence_pack_generation() {
    use diachron_core::pr_correlation::{correlate_events_to_pr, MatchConfidence, PRSummary};

    let conn = create_test_db();

    // Insert test events
    insert_test_event(
        &conn,
        "2026-01-11T00:00:00.000",
        "Write",
        Some("src/auth.rs"),
        "create",
        Some("session-1"),
        Some("abc123"),
    );
    insert_test_event(
        &conn,
        "2026-01-11T00:01:00.000",
        "Edit",
        Some("src/auth.rs"),
        "modify",
        Some("session-1"),
        Some("abc123"),
    );

    // Correlate
    let evidence = correlate_events_to_pr(
        &conn,
        42,
        &["abc123".to_string()],
        "fix/auth",
        "2026-01-10T00:00:00.000",
        "2026-01-12T00:00:00.000",
    )
    .expect("Failed to correlate");

    // Generate pack
    let pack = generate_evidence_pack(evidence, None, Some("Fix auth refresh".to_string()));

    assert_eq!(pack.pr_id, 42);
    assert_eq!(pack.intent, Some("Fix auth refresh".to_string()));
    assert!(!pack.commits.is_empty());

    // Render markdown
    let markdown = render_markdown_narrative(&pack);

    assert!(markdown.contains("## PR #42"));
    assert!(markdown.contains("Fix auth refresh"));
    assert!(markdown.contains("Evidence Trail"));
    assert!(markdown.contains("Verification"));
}

#[test]
fn test_markdown_rendering_with_verification() {
    use diachron_core::evidence_pack::{EvidencePack, VerificationStatus};
    use diachron_core::pr_correlation::{CommitEvidence, MatchConfidence, PRSummary};
    use diachron_core::types::StoredEvent;

    let pack = EvidencePack {
        pr_id: 123,
        generated_at: "2026-01-11T00:00:00Z".to_string(),
        diachron_version: "0.3.0".to_string(),
        summary: PRSummary {
            files_changed: 3,
            lines_added: 100,
            lines_removed: 20,
            tool_operations: 5,
            sessions: 2,
        },
        commits: vec![CommitEvidence {
            sha: "deadbeef12345678".to_string(),
            message: Some("feat: add OAuth2 login".to_string()),
            events: vec![StoredEvent {
                id: 1,
                timestamp: "2026-01-11T00:00:00".to_string(),
                timestamp_display: None,
                session_id: Some("session-1".to_string()),
                tool_name: "Write".to_string(),
                file_path: Some("src/auth/oauth.rs".to_string()),
                operation: Some("create".to_string()),
                diff_summary: Some("+50 lines".to_string()),
                raw_input: None,
                ai_summary: None,
                git_commit_sha: Some("deadbeef12345678".to_string()),
                metadata: None,
            }],
            confidence: MatchConfidence::High,
        }],
        verification: VerificationStatus {
            chain_verified: true,
            tests_executed: true,
            build_succeeded: false,
            human_reviewed: false,
        },
        intent: Some("Add OAuth2 login flow".to_string()),
        coverage_pct: 95.5,
        unmatched_count: 1,
    };

    let md = render_markdown_narrative(&pack);

    // Check header
    assert!(md.contains("## PR #123"));

    // Check intent
    assert!(md.contains("> Add OAuth2 login flow"));

    // Check summary
    assert!(md.contains("**Files modified**: 3"));
    assert!(md.contains("+100 / -20"));
    assert!(md.contains("**Tool operations**: 5"));

    // Check evidence trail
    assert!(md.contains("**Coverage**: 95.5%"));
    assert!(md.contains("(1 unmatched)"));
    assert!(md.contains("Commit `deadbee`")); // Short SHA
    assert!(md.contains("feat: add OAuth2 login"));
    assert!(md.contains("(HIGH)"));

    // Check verification checkboxes
    assert!(md.contains("[x] Hash chain integrity"));
    assert!(md.contains("[x] Tests executed"));
    assert!(md.contains("[ ] Build succeeded"));
    assert!(md.contains("[ ] Human review"));

    // Check footer
    assert!(md.contains("Diachron"));
    assert!(md.contains("v0.3.0"));
}

#[test]
fn test_fingerprint_matching() {
    use diachron_core::fingerprint::{compute_fingerprint, cosine_similarity, match_fingerprint};

    // Create fingerprints for similar content
    let fp1 = compute_fingerprint("fn hello() { println!(\"Hello\"); }", None, None);
    let fp2 = compute_fingerprint("fn hello() { println!(\"Hello\"); }", None, None);
    let fp3 = compute_fingerprint("fn goodbye() { println!(\"Bye\"); }", None, None);

    // Identical content should have same hash
    assert_eq!(fp1.content_hash, fp2.content_hash);

    // Different content should have different hash
    assert_ne!(fp1.content_hash, fp3.content_hash);

    // Test matching
    let candidates = vec![(1, fp1.clone()), (2, fp3.clone())];

    let match_result = match_fingerprint(&fp2, &candidates, 0.9);
    assert!(match_result.is_some());

    let m = match_result.unwrap();
    assert_eq!(m.event_id, 1); // Should match fp1
    assert_eq!(m.confidence, 1.0); // Exact match
}

#[test]
fn test_json_export() {
    use diachron_core::evidence_pack::{export_json, EvidencePack, VerificationStatus};
    use diachron_core::pr_correlation::PRSummary;

    let pack = EvidencePack {
        pr_id: 1,
        generated_at: "2026-01-11T00:00:00Z".to_string(),
        diachron_version: "0.3.0".to_string(),
        summary: PRSummary {
            files_changed: 1,
            lines_added: 10,
            lines_removed: 0,
            tool_operations: 1,
            sessions: 1,
        },
        commits: vec![],
        verification: VerificationStatus::default(),
        intent: None,
        coverage_pct: 100.0,
        unmatched_count: 0,
    };

    let json = export_json(&pack).expect("Failed to export JSON");

    assert!(json.contains("\"pr_id\": 1"));
    assert!(json.contains("\"diachron_version\": \"0.3.0\""));
    assert!(json.contains("\"coverage_pct\": 100.0"));
}
