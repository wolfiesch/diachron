//! Vector Index for semantic search
//!
//! Uses usearch with HNSW algorithm for fast approximate nearest neighbor search.
//! ~10μs per search at 230k vectors (vs 100-300ms for sqlite-vec).

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use thiserror::Error;
use tracing::{debug, info};
use usearch::{Index, IndexOptions, MetricKind, ScalarKind};

/// Embedding dimension (384 for all-MiniLM-L6-v2)
pub const EMBEDDING_DIM: usize = 384;

#[derive(Error, Debug)]
pub enum VectorError {
    #[error("Index error: {0}")]
    IndexError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    SerdeError(#[from] serde_json::Error),

    #[error("ID not found: {0}")]
    NotFound(String),
}

pub type Result<T> = std::result::Result<T, VectorError>;

/// Search result from vector index
#[derive(Debug, Clone)]
pub struct VectorSearchResult {
    /// The ID of the matching item
    pub id: String,
    /// Similarity score (0-1, higher is better)
    pub score: f32,
}

/// HNSW-based vector index
pub struct VectorIndex {
    index: Index,
    /// Map from usearch internal key to our string ID
    id_map: HashMap<u64, String>,
    /// Reverse map from string ID to usearch key
    key_map: HashMap<String, u64>,
    /// Counter for generating unique keys
    next_key: AtomicU64,
    /// Dimension of embeddings
    dim: usize,
}

impl VectorIndex {
    /// Create a new empty vector index
    pub fn new(dim: usize) -> Result<Self> {
        let options = IndexOptions {
            dimensions: dim,
            metric: MetricKind::Cos, // Cosine similarity
            quantization: ScalarKind::F32,
            connectivity: 16,       // M parameter for HNSW
            expansion_add: 128,     // ef_construction
            expansion_search: 64,   // ef_search
            multi: false,           // Single vector per key
        };

        let index = Index::new(&options).map_err(|e| VectorError::IndexError(e.to_string()))?;

        Ok(Self {
            index,
            id_map: HashMap::new(),
            key_map: HashMap::new(),
            next_key: AtomicU64::new(0),
            dim,
        })
    }

    /// Create a new vector index with default dimension (384)
    pub fn new_default() -> Result<Self> {
        Self::new(EMBEDDING_DIM)
    }

    /// Add a vector with the given ID
    ///
    /// If the ID already exists, it will be updated.
    pub fn add(&mut self, id: &str, embedding: &[f32]) -> Result<()> {
        assert_eq!(
            embedding.len(),
            self.dim,
            "Embedding dimension mismatch: expected {}, got {}",
            self.dim,
            embedding.len()
        );

        // Check if ID already exists
        if let Some(&existing_key) = self.key_map.get(id) {
            // Remove old entry
            self.index
                .remove(existing_key)
                .map_err(|e| VectorError::IndexError(e.to_string()))?;
        }

        // Generate new key
        let key = self.next_key.fetch_add(1, Ordering::SeqCst);

        // Reserve capacity if needed (usearch requires this before adding)
        let current_capacity = self.index.capacity();
        if current_capacity <= self.id_map.len() {
            // Reserve more capacity (double or at least 16)
            let new_capacity = (current_capacity * 2).max(16);
            self.index
                .reserve(new_capacity)
                .map_err(|e| VectorError::IndexError(format!("Failed to reserve capacity: {}", e)))?;
        }

        // Add to index
        self.index
            .add(key, embedding)
            .map_err(|e| VectorError::IndexError(e.to_string()))?;

        // Update maps
        self.id_map.insert(key, id.to_string());
        self.key_map.insert(id.to_string(), key);

        debug!("Added vector for ID: {} (key: {})", id, key);
        Ok(())
    }

    /// Search for the k most similar vectors
    ///
    /// Returns results sorted by similarity (highest first).
    pub fn search(&self, query: &[f32], k: usize) -> Result<Vec<VectorSearchResult>> {
        assert_eq!(
            query.len(),
            self.dim,
            "Query dimension mismatch: expected {}, got {}",
            self.dim,
            query.len()
        );

        if self.is_empty() {
            return Ok(vec![]);
        }

        let matches = self
            .index
            .search(query, k)
            .map_err(|e| VectorError::IndexError(e.to_string()))?;

        let results: Vec<VectorSearchResult> = matches
            .keys
            .iter()
            .zip(matches.distances.iter())
            .filter_map(|(&key, &distance)| {
                self.id_map.get(&key).map(|id| VectorSearchResult {
                    id: id.clone(),
                    // Convert cosine distance to similarity (1 - distance for normalized vectors)
                    score: 1.0 - distance,
                })
            })
            .collect();

        debug!("Search returned {} results", results.len());
        Ok(results)
    }

    /// Remove a vector by ID
    pub fn remove(&mut self, id: &str) -> Result<()> {
        if let Some(key) = self.key_map.remove(id) {
            self.index
                .remove(key)
                .map_err(|e| VectorError::IndexError(e.to_string()))?;
            self.id_map.remove(&key);
            debug!("Removed vector for ID: {}", id);
            Ok(())
        } else {
            Err(VectorError::NotFound(id.to_string()))
        }
    }

    /// Check if the index contains an ID
    pub fn contains(&self, id: &str) -> bool {
        self.key_map.contains_key(id)
    }

    /// Get the number of vectors in the index
    pub fn len(&self) -> usize {
        self.id_map.len()
    }

    /// Check if the index is empty
    pub fn is_empty(&self) -> bool {
        self.id_map.is_empty()
    }

    /// Get the embedding dimension
    pub fn dim(&self) -> usize {
        self.dim
    }

    /// Save the index to disk
    ///
    /// Saves both the usearch index and the ID mappings.
    pub fn save(&self, path: &Path) -> Result<()> {
        // Create directory if needed
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Save usearch index
        let index_path = path.with_extension("usearch");
        self.index
            .save(index_path.to_str().ok_or_else(|| VectorError::IndexError("Invalid path".into()))?)
            .map_err(|e| VectorError::IndexError(e.to_string()))?;

        // Save ID mappings as JSON
        let meta = IndexMetadata {
            id_map: self.id_map.clone(),
            key_map: self.key_map.clone(),
            next_key: self.next_key.load(Ordering::SeqCst),
            dim: self.dim,
        };
        let meta_path = path.with_extension("json");
        let meta_json = serde_json::to_string_pretty(&meta)?;
        fs::write(&meta_path, meta_json)?;

        info!(
            "Saved vector index: {} vectors to {:?}",
            self.len(),
            index_path
        );
        Ok(())
    }

    /// Load an index from disk
    pub fn load(path: &Path) -> Result<Self> {
        // Load metadata first to get dimensions
        let meta_path = path.with_extension("json");
        let meta_json = fs::read_to_string(&meta_path)?;
        let meta: IndexMetadata = serde_json::from_str(&meta_json)?;

        // Create index with correct options
        let options = IndexOptions {
            dimensions: meta.dim,
            metric: MetricKind::Cos,
            quantization: ScalarKind::F32,
            connectivity: 16,
            expansion_add: 128,
            expansion_search: 64,
            multi: false,
        };

        let index =
            Index::new(&options).map_err(|e| VectorError::IndexError(e.to_string()))?;

        // Load usearch index
        let index_path = path.with_extension("usearch");
        index
            .load(index_path.to_str().ok_or_else(|| VectorError::IndexError("Invalid path".into()))?)
            .map_err(|e| VectorError::IndexError(e.to_string()))?;

        info!("Loaded vector index: {} vectors from {:?}", meta.id_map.len(), index_path);

        Ok(Self {
            index,
            id_map: meta.id_map,
            key_map: meta.key_map,
            next_key: AtomicU64::new(meta.next_key),
            dim: meta.dim,
        })
    }

    /// Check if saved index files exist
    pub fn exists(path: &Path) -> bool {
        path.with_extension("usearch").exists() && path.with_extension("json").exists()
    }
}

/// Metadata for persisting ID mappings
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct IndexMetadata {
    id_map: HashMap<u64, String>,
    key_map: HashMap<String, u64>,
    next_key: u64,
    dim: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_embedding(seed: f32) -> Vec<f32> {
        // Create a simple normalized embedding for testing
        let mut embedding = vec![seed; EMBEDDING_DIM];
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        for x in &mut embedding {
            *x /= norm;
        }
        embedding
    }

    #[test]
    fn test_add_and_search() {
        let mut index = VectorIndex::new_default().unwrap();

        // Add some vectors
        index.add("doc1", &create_test_embedding(1.0)).unwrap();
        index.add("doc2", &create_test_embedding(2.0)).unwrap();
        index.add("doc3", &create_test_embedding(3.0)).unwrap();

        assert_eq!(index.len(), 3);

        // Search for similar
        let results = index.search(&create_test_embedding(1.5), 2).unwrap();
        assert_eq!(results.len(), 2);

        // First result should have highest similarity
        assert!(results[0].score >= results[1].score);
    }

    #[test]
    fn test_remove() {
        let mut index = VectorIndex::new_default().unwrap();

        index.add("doc1", &create_test_embedding(1.0)).unwrap();
        index.add("doc2", &create_test_embedding(2.0)).unwrap();

        assert_eq!(index.len(), 2);
        assert!(index.contains("doc1"));

        index.remove("doc1").unwrap();

        assert_eq!(index.len(), 1);
        assert!(!index.contains("doc1"));
        assert!(index.contains("doc2"));
    }

    #[test]
    fn test_empty_search() {
        let index = VectorIndex::new_default().unwrap();
        let results = index.search(&create_test_embedding(1.0), 10).unwrap();
        assert!(results.is_empty());
    }
}
