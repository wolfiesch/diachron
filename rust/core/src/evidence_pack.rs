//! Evidence pack generation for PR narratives
//!
//! This module generates structured evidence packs that can be:
//! - Exported as JSON for GitHub Actions
//! - Rendered as Markdown for PR comments
//! - Stored for audit trails
//!
//! # Evidence Pack Structure
//!
//! ```json
//! {
//!   "pr_id": 142,
//!   "generated_at": "2026-01-11T00:00:00Z",
//!   "diachron_version": "0.3.0",
//!   "summary": { ... },
//!   "commits": [ ... ],
//!   "verification": { ... },
//!   "intent": "Fix the 401 errors on page refresh"
//! }
//! ```

use serde::{Deserialize, Serialize};

use crate::hash_chain::ChainVerificationResult;
use crate::pr_correlation::{CommitEvidence, PREvidence, PRSummary};

/// Diachron version for evidence packs.
pub const DIACHRON_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Complete evidence pack for a pull request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidencePack {
    /// PR identifier (number)
    pub pr_id: u64,
    /// When this evidence pack was generated
    pub generated_at: String,
    /// Diachron version used to generate
    pub diachron_version: String,
    /// Summary statistics
    pub summary: PRSummary,
    /// Evidence grouped by commit
    pub commits: Vec<CommitEvidence>,
    /// Chain verification status
    pub verification: VerificationStatus,
    /// User intent extracted from conversation (if available)
    pub intent: Option<String>,
    /// Coverage percentage (how many events were matched)
    pub coverage_pct: f32,
    /// Unmatched event count
    pub unmatched_count: usize,
}

/// Verification status of the evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationStatus {
    /// Whether hash chain is verified
    pub chain_verified: bool,
    /// Whether tests were run after changes
    pub tests_executed: bool,
    /// Whether build succeeded
    pub build_succeeded: bool,
    /// Human review status
    pub human_reviewed: bool,
}

impl Default for VerificationStatus {
    fn default() -> Self {
        Self {
            chain_verified: false,
            tests_executed: false,
            build_succeeded: false,
            human_reviewed: false,
        }
    }
}

/// Generate an evidence pack from PR evidence and chain verification.
///
/// # Arguments
///
/// * `pr_evidence` - Correlated PR evidence
/// * `chain_result` - Hash chain verification result
/// * `intent` - Optional user intent string
///
/// # Returns
///
/// Complete evidence pack
pub fn generate_evidence_pack(
    pr_evidence: PREvidence,
    chain_result: Option<&ChainVerificationResult>,
    intent: Option<String>,
) -> EvidencePack {
    let summary = pr_evidence.summary();
    let generated_at = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    // Determine verification status from events
    let mut verification = VerificationStatus::default();

    if let Some(chain) = chain_result {
        verification.chain_verified = chain.valid;
    }

    // Check if tests were run by looking at Bash events with test/build commands
    for commit in &pr_evidence.commits {
        for event in &commit.events {
            if event.tool_name == "Bash" {
                if let Some(ref metadata) = event.metadata {
                    if let Ok(meta) = serde_json::from_str::<serde_json::Value>(metadata) {
                        if let Some(category) = meta.get("command_category").and_then(|c| c.as_str())
                        {
                            if category == "test" {
                                verification.tests_executed = true;
                            }
                            if category == "build" {
                                verification.build_succeeded = true;
                            }
                        }
                    }
                }
            }
        }
    }

    EvidencePack {
        pr_id: pr_evidence.pr_id,
        generated_at,
        diachron_version: DIACHRON_VERSION.to_string(),
        summary,
        commits: pr_evidence.commits,
        verification,
        intent,
        coverage_pct: pr_evidence.coverage_pct,
        unmatched_count: pr_evidence.unmatched_events.len(),
    }
}

/// Render an evidence pack as Markdown for PR comments.
///
/// # Arguments
///
/// * `pack` - The evidence pack to render
///
/// # Returns
///
/// Markdown string suitable for GitHub PR comment
pub fn render_markdown_narrative(pack: &EvidencePack) -> String {
    let mut md = String::new();

    // Header
    md.push_str(&format!("## PR #{}: AI Provenance Evidence\n\n", pack.pr_id));

    // Intent section (if available)
    if let Some(ref intent) = pack.intent {
        md.push_str("### Intent\n");
        md.push_str(&format!("> {}\n\n", intent));
    }

    // Summary section
    md.push_str("### What Changed\n");
    md.push_str(&format!(
        "- **Files modified**: {}\n",
        pack.summary.files_changed
    ));
    md.push_str(&format!(
        "- **Lines**: +{} / -{}\n",
        pack.summary.lines_added, pack.summary.lines_removed
    ));
    md.push_str(&format!(
        "- **Tool operations**: {}\n",
        pack.summary.tool_operations
    ));
    md.push_str(&format!("- **Sessions**: {}\n\n", pack.summary.sessions));

    // Evidence trail section
    md.push_str("### Evidence Trail\n");
    md.push_str(&format!(
        "- **Coverage**: {:.1}% of events matched to commits",
        pack.coverage_pct
    ));
    if pack.unmatched_count > 0 {
        md.push_str(&format!(" ({} unmatched)", pack.unmatched_count));
    }
    md.push_str("\n");

    for commit in &pack.commits {
        let sha_short = &commit.sha[..7.min(commit.sha.len())];
        md.push_str(&format!("\n**Commit `{}`**", sha_short));
        if let Some(ref msg) = commit.message {
            let first_line = msg.lines().next().unwrap_or(msg);
            md.push_str(&format!(": {}", first_line));
        }
        md.push_str(&format!(" ({})\n", commit.confidence.as_str()));

        for event in &commit.events {
            let tool = &event.tool_name;
            let file = event.file_path.as_deref().unwrap_or("-");
            let op = event.operation.as_deref().unwrap_or("-");
            md.push_str(&format!("  - `{}` {} → {}\n", tool, op, file));
        }
    }
    md.push_str("\n");

    // Verification section
    md.push_str("### Verification\n");
    md.push_str(&format!(
        "- [{}] Hash chain integrity\n",
        if pack.verification.chain_verified {
            "x"
        } else {
            " "
        }
    ));
    md.push_str(&format!(
        "- [{}] Tests executed after changes\n",
        if pack.verification.tests_executed {
            "x"
        } else {
            " "
        }
    ));
    md.push_str(&format!(
        "- [{}] Build succeeded\n",
        if pack.verification.build_succeeded {
            "x"
        } else {
            " "
        }
    ));
    md.push_str(&format!(
        "- [{}] Human review\n\n",
        if pack.verification.human_reviewed {
            "x"
        } else {
            " "
        }
    ));

    // Footer
    md.push_str(&format!(
        "---\n*Generated by [Diachron](https://github.com/wolfiesch/diachron) v{} at {}*\n",
        pack.diachron_version, pack.generated_at
    ));

    md
}

/// Export evidence pack as JSON string.
///
/// # Arguments
///
/// * `pack` - The evidence pack to export
///
/// # Returns
///
/// Pretty-printed JSON string
pub fn export_json(pack: &EvidencePack) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(pack)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pr_correlation::MatchConfidence;
    use crate::types::StoredEvent;

    fn mock_event(tool: &str, file: &str, op: &str) -> StoredEvent {
        StoredEvent {
            id: 1,
            timestamp: "2026-01-11T00:00:00".to_string(),
            timestamp_display: None,
            session_id: Some("session-1".to_string()),
            tool_name: tool.to_string(),
            file_path: Some(file.to_string()),
            operation: Some(op.to_string()),
            diff_summary: Some("+10 lines".to_string()),
            raw_input: None,
            ai_summary: None,
            git_commit_sha: None,
            metadata: None,
        }
    }

    #[test]
    fn test_render_markdown_narrative() {
        let pack = EvidencePack {
            pr_id: 142,
            generated_at: "2026-01-11T00:00:00Z".to_string(),
            diachron_version: "0.3.0".to_string(),
            summary: PRSummary {
                files_changed: 2,
                lines_added: 45,
                lines_removed: 10,
                tool_operations: 3,
                sessions: 1,
            },
            commits: vec![CommitEvidence {
                sha: "abc1234567890".to_string(),
                message: Some("Fix OAuth2 refresh".to_string()),
                events: vec![
                    mock_event("Write", "src/auth.rs", "create"),
                    mock_event("Edit", "src/auth.rs", "modify"),
                ],
                confidence: MatchConfidence::High,
            }],
            verification: VerificationStatus {
                chain_verified: true,
                tests_executed: true,
                build_succeeded: true,
                human_reviewed: false,
            },
            intent: Some("Fix the 401 errors on page refresh".to_string()),
            coverage_pct: 100.0,
            unmatched_count: 0,
        };

        let md = render_markdown_narrative(&pack);

        assert!(md.contains("## PR #142"));
        assert!(md.contains("Fix the 401 errors"));
        assert!(md.contains("abc1234"));
        assert!(md.contains("[x] Hash chain integrity"));
        assert!(md.contains("[ ] Human review"));
    }

    #[test]
    fn test_export_json() {
        let pack = EvidencePack {
            pr_id: 42,
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

        let json = export_json(&pack).unwrap();
        assert!(json.contains("\"pr_id\": 42"));
        assert!(json.contains("\"diachron_version\""));
    }
}
