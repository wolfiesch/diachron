//! Content fingerprinting for stable blame across refactors
//!
//! This module provides content-based identification of code changes that
//! survives refactoring operations (renames, moves, minor edits).
//!
//! # Fingerprint Components
//!
//! 1. **Content Hash**: SHA256 of normalized content (whitespace-normalized)
//! 2. **Context Hash**: SHA256 of surrounding context (±5 lines)
//! 3. **Semantic Signature**: Embedding vector for semantic similarity matching
//!
//! # Matching Strategy
//!
//! 1. Try exact content_hash match (fastest, most precise)
//! 2. Fall back to context_hash match (handles minor edits)
//! 3. Fall back to semantic similarity (handles refactors)

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Default context size (lines before and after the change)
pub const DEFAULT_CONTEXT_LINES: usize = 5;

/// Similarity threshold for semantic matching (cosine similarity)
pub const DEFAULT_SIMILARITY_THRESHOLD: f32 = 0.85;

/// A content fingerprint for identifying code changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HunkFingerprint {
    /// SHA256 hash of normalized content
    pub content_hash: [u8; 32],
    /// SHA256 hash of surrounding context (±5 lines)
    pub context_hash: [u8; 32],
    /// Semantic embedding vector (384-dim all-MiniLM-L6-v2)
    pub semantic_sig: Option<Vec<f32>>,
}

/// Result of fingerprint matching.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FingerprintMatch {
    /// ID of the matched event
    pub event_id: i64,
    /// Confidence of the match
    pub confidence: MatchConfidence,
    /// Similarity score (0.0 - 1.0)
    pub similarity: f32,
    /// Which matching method succeeded
    pub match_type: MatchType,
}

/// Confidence level of a fingerprint match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatchConfidence {
    /// Exact content hash match
    High,
    /// Context hash match (handles minor edits)
    Medium,
    /// Semantic similarity match (handles refactors)
    Low,
}

/// Type of match that succeeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatchType {
    /// Exact content hash match
    ContentHash,
    /// Context hash match
    ContextHash,
    /// Semantic similarity match
    SemanticSimilarity,
}

/// Normalize content for consistent hashing.
///
/// Removes trailing whitespace, normalizes line endings,
/// and optionally removes comments and blank lines.
fn normalize_content(content: &str) -> String {
    content
        .lines()
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Compute SHA256 hash of a string.
fn sha256_hash(data: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data.as_bytes());
    hasher.finalize().into()
}

/// Compute a fingerprint for a code change.
///
/// # Arguments
///
/// * `content` - The changed content (e.g., added/modified lines)
/// * `context` - Optional surrounding context (lines before and after)
/// * `embedding` - Optional pre-computed embedding vector
///
/// # Returns
///
/// A `HunkFingerprint` containing content hash, context hash, and semantic signature
pub fn compute_fingerprint(
    content: &str,
    context: Option<&str>,
    embedding: Option<Vec<f32>>,
) -> HunkFingerprint {
    // Normalize and hash content
    let normalized = normalize_content(content);
    let content_hash = sha256_hash(&normalized);

    // Compute context hash
    let context_hash = match context {
        Some(ctx) => {
            let normalized_ctx = normalize_content(ctx);
            sha256_hash(&normalized_ctx)
        }
        None => [0u8; 32], // No context available
    };

    HunkFingerprint {
        content_hash,
        context_hash,
        semantic_sig: embedding,
    }
}

/// Extract context lines around a target line.
///
/// # Arguments
///
/// * `file_content` - Full file content
/// * `target_line` - Line number to get context for (0-indexed)
/// * `context_lines` - Number of lines before and after
///
/// # Returns
///
/// String containing the context lines
pub fn extract_context(
    file_content: &str,
    target_line: usize,
    context_lines: usize,
) -> String {
    let lines: Vec<&str> = file_content.lines().collect();
    let total_lines = lines.len();

    if target_line >= total_lines {
        return String::new();
    }

    let start = target_line.saturating_sub(context_lines);
    let end = (target_line + context_lines + 1).min(total_lines);

    lines[start..end].join("\n")
}

/// Compute cosine similarity between two embedding vectors.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot / (norm_a * norm_b)
}

/// Match a fingerprint against stored fingerprints.
///
/// # Arguments
///
/// * `current` - The fingerprint to match
/// * `candidates` - Stored fingerprints to match against (with event IDs)
/// * `threshold` - Minimum similarity for semantic matching
///
/// # Returns
///
/// Best match if found above threshold
pub fn match_fingerprint(
    current: &HunkFingerprint,
    candidates: &[(i64, HunkFingerprint)],
    threshold: f32,
) -> Option<FingerprintMatch> {
    let mut best_match: Option<FingerprintMatch> = None;

    for (event_id, candidate) in candidates {
        // 1. Try exact content hash match (highest confidence)
        if current.content_hash == candidate.content_hash {
            return Some(FingerprintMatch {
                event_id: *event_id,
                confidence: MatchConfidence::High,
                similarity: 1.0,
                match_type: MatchType::ContentHash,
            });
        }

        // 2. Try context hash match (medium confidence)
        if current.context_hash != [0u8; 32]
            && candidate.context_hash != [0u8; 32]
            && current.context_hash == candidate.context_hash
        {
            let this_match = FingerprintMatch {
                event_id: *event_id,
                confidence: MatchConfidence::Medium,
                similarity: 0.95, // High but not exact
                match_type: MatchType::ContextHash,
            };

            if best_match.is_none()
                || best_match.as_ref().unwrap().confidence == MatchConfidence::Low
            {
                best_match = Some(this_match);
            }
        }

        // 3. Try semantic similarity (low confidence but survives refactors)
        if let (Some(ref curr_emb), Some(ref cand_emb)) =
            (&current.semantic_sig, &candidate.semantic_sig)
        {
            let similarity = cosine_similarity(curr_emb, cand_emb);
            if similarity >= threshold {
                let this_match = FingerprintMatch {
                    event_id: *event_id,
                    confidence: MatchConfidence::Low,
                    similarity,
                    match_type: MatchType::SemanticSimilarity,
                };

                // Only update if we don't have a better match
                if best_match.is_none() {
                    best_match = Some(this_match);
                } else if let Some(ref existing) = best_match {
                    if existing.confidence == MatchConfidence::Low
                        && similarity > existing.similarity
                    {
                        best_match = Some(this_match);
                    }
                }
            }
        }
    }

    best_match
}

/// Convert fingerprint hashes to hex strings for display.
pub fn format_fingerprint(fp: &HunkFingerprint) -> String {
    format!(
        "content:{} context:{}",
        hex::encode(&fp.content_hash[..8]),
        hex::encode(&fp.context_hash[..8])
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_content() {
        // Note: normalize_content strips trailing whitespace per-line and joins with \n
        // It does NOT preserve trailing newlines from the original content
        let content = "  hello world  \n  foo bar  \n";
        let normalized = normalize_content(content);
        assert_eq!(normalized, "  hello world\n  foo bar");
    }

    #[test]
    fn test_compute_fingerprint_deterministic() {
        let content = "function add(a, b) {\n  return a + b;\n}";
        let fp1 = compute_fingerprint(content, None, None);
        let fp2 = compute_fingerprint(content, None, None);

        assert_eq!(fp1.content_hash, fp2.content_hash);
    }

    #[test]
    fn test_compute_fingerprint_different_content() {
        let content1 = "function add(a, b) { return a + b; }";
        let content2 = "function subtract(a, b) { return a - b; }";

        let fp1 = compute_fingerprint(content1, None, None);
        let fp2 = compute_fingerprint(content2, None, None);

        assert_ne!(fp1.content_hash, fp2.content_hash);
    }

    #[test]
    fn test_extract_context() {
        let file_content = "line 0\nline 1\nline 2\nline 3\nline 4\nline 5\nline 6";

        let context = extract_context(file_content, 3, 2);
        assert_eq!(context, "line 1\nline 2\nline 3\nline 4\nline 5");
    }

    #[test]
    fn test_extract_context_edge_start() {
        let file_content = "line 0\nline 1\nline 2";

        let context = extract_context(file_content, 0, 2);
        assert_eq!(context, "line 0\nline 1\nline 2");
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];

        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 0.0001);

        let c = vec![0.0, 1.0, 0.0];
        let sim2 = cosine_similarity(&a, &c);
        assert!((sim2 - 0.0).abs() < 0.0001);
    }

    #[test]
    fn test_match_fingerprint_exact() {
        let content = "hello world";
        let fp = compute_fingerprint(content, None, None);

        let candidates = vec![(42, fp.clone())];

        let result = match_fingerprint(&fp, &candidates, 0.8);

        assert!(result.is_some());
        let m = result.unwrap();
        assert_eq!(m.event_id, 42);
        assert_eq!(m.confidence, MatchConfidence::High);
        assert_eq!(m.match_type, MatchType::ContentHash);
    }

    #[test]
    fn test_match_fingerprint_semantic() {
        let fp1 = HunkFingerprint {
            content_hash: [1u8; 32],
            context_hash: [2u8; 32],
            semantic_sig: Some(vec![0.5, 0.5, 0.5]),
        };

        let fp2 = HunkFingerprint {
            content_hash: [3u8; 32], // Different
            context_hash: [4u8; 32], // Different
            semantic_sig: Some(vec![0.5, 0.5, 0.5]), // Same semantic
        };

        let candidates = vec![(99, fp2)];

        let result = match_fingerprint(&fp1, &candidates, 0.8);

        assert!(result.is_some());
        let m = result.unwrap();
        assert_eq!(m.event_id, 99);
        assert_eq!(m.confidence, MatchConfidence::Low);
        assert_eq!(m.match_type, MatchType::SemanticSimilarity);
    }
}
