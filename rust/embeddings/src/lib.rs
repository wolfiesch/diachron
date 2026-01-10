//! Diachron Embeddings
//!
//! ONNX-based embedding engine for semantic search.
//!
//! Model: all-MiniLM-L6-v2 (22M params, 384-dim output)
//!
//! Features:
//! - Automatic model download from HuggingFace Hub
//! - BERT tokenization with truncation
//! - Mean pooling with L2 normalization
//! - Batch embedding support

mod download;

use std::path::{Path, PathBuf};

use ndarray::Array2;
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Tensor;
use thiserror::Error;
use tokenizers::Tokenizer;
use tracing::{debug, info};

pub use download::{ensure_model_exists, ModelPaths};

/// Embedding dimension for all-MiniLM-L6-v2
pub const EMBEDDING_DIM: usize = 384;

/// Maximum sequence length (BERT limit)
pub const MAX_SEQ_LENGTH: usize = 512;

/// Maximum text length before truncation (chars)
pub const MAX_TEXT_LENGTH: usize = 2000;

#[derive(Error, Debug)]
pub enum EmbeddingError {
    #[error("Model not found at {path}")]
    ModelNotFound { path: String },

    #[error("Model download failed: {0}")]
    DownloadFailed(String),

    #[error("ONNX runtime error: {0}")]
    OnnxError(#[from] ort::Error),

    #[error("Tokenizer error: {0}")]
    TokenizerError(String),

    #[error("Inference failed: {0}")]
    InferenceFailed(String),
}

pub type Result<T> = std::result::Result<T, EmbeddingError>;

/// Embedding engine configuration
#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
    /// Path to the ONNX model file
    pub model_path: PathBuf,

    /// Path to the tokenizer.json file
    pub tokenizer_path: PathBuf,

    /// Dimension of output embeddings (384 for MiniLM)
    pub embedding_dim: usize,

    /// Maximum sequence length
    pub max_length: usize,

    /// Maximum text length before truncation
    pub max_text_length: usize,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        let model_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join(".diachron")
            .join("models")
            .join("all-MiniLM-L6-v2");

        Self {
            model_path: model_dir.join("model.onnx"),
            tokenizer_path: model_dir.join("tokenizer.json"),
            embedding_dim: EMBEDDING_DIM,
            max_length: MAX_SEQ_LENGTH,
            max_text_length: MAX_TEXT_LENGTH,
        }
    }
}

impl EmbeddingConfig {
    /// Create config from model paths
    pub fn from_paths(paths: &ModelPaths) -> Self {
        Self {
            model_path: paths.model_path.clone(),
            tokenizer_path: paths.tokenizer_path.clone(),
            ..Default::default()
        }
    }
}

/// Embedding engine using ONNX Runtime
pub struct EmbeddingEngine {
    session: Session,
    tokenizer: Tokenizer,
    config: EmbeddingConfig,
}

impl EmbeddingEngine {
    /// Create a new embedding engine
    ///
    /// Loads the ONNX model and tokenizer from the specified paths.
    /// Use `ensure_model_exists()` to download the model first if needed.
    pub fn new(config: EmbeddingConfig) -> Result<Self> {
        info!("Loading embedding model from {:?}", config.model_path);

        if !config.model_path.exists() {
            return Err(EmbeddingError::ModelNotFound {
                path: config.model_path.display().to_string(),
            });
        }

        if !config.tokenizer_path.exists() {
            return Err(EmbeddingError::ModelNotFound {
                path: config.tokenizer_path.display().to_string(),
            });
        }

        // Load ONNX session with optimizations
        let session = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_intra_threads(4)?
            .commit_from_file(&config.model_path)?;

        info!("ONNX session loaded successfully");

        // Load tokenizer
        let tokenizer = Tokenizer::from_file(&config.tokenizer_path)
            .map_err(|e| EmbeddingError::TokenizerError(e.to_string()))?;

        info!("Tokenizer loaded successfully");

        Ok(Self {
            session,
            tokenizer,
            config,
        })
    }

    /// Create a new embedding engine with default paths
    ///
    /// Downloads the model if not present.
    pub fn new_default() -> Result<Self> {
        let paths = ensure_model_exists()?;
        let config = EmbeddingConfig::from_paths(&paths);
        Self::new(config)
    }

    /// Generate embedding for a single text
    ///
    /// Returns a 384-dimensional normalized float vector.
    pub fn embed(&mut self, text: &str) -> Result<Vec<f32>> {
        let embeddings = self.embed_batch(&[text])?;
        Ok(embeddings.into_iter().next().unwrap())
    }

    /// Generate embeddings for multiple texts (batch)
    ///
    /// More efficient than calling embed() multiple times.
    pub fn embed_batch(&mut self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        // Truncate texts to max length
        let truncated: Vec<&str> = texts
            .iter()
            .map(|t| {
                if t.len() > self.config.max_text_length {
                    &t[..self.config.max_text_length]
                } else {
                    *t
                }
            })
            .collect();

        debug!("Embedding {} texts", truncated.len());

        // Tokenize all texts
        let encodings = self
            .tokenizer
            .encode_batch(truncated.clone(), true)
            .map_err(|e| EmbeddingError::TokenizerError(e.to_string()))?;

        // Find max length in batch
        let max_len = encodings
            .iter()
            .map(|e| e.get_ids().len())
            .max()
            .unwrap_or(0)
            .min(self.config.max_length);

        let batch_size = encodings.len();

        // Create input tensors with padding
        let mut input_ids = Array2::<i64>::zeros((batch_size, max_len));
        let mut attention_mask = Array2::<i64>::zeros((batch_size, max_len));
        let mut token_type_ids = Array2::<i64>::zeros((batch_size, max_len));

        for (i, encoding) in encodings.iter().enumerate() {
            let ids = encoding.get_ids();
            let mask = encoding.get_attention_mask();
            let types = encoding.get_type_ids();

            let len = ids.len().min(max_len);

            for j in 0..len {
                input_ids[[i, j]] = ids[j] as i64;
                attention_mask[[i, j]] = mask[j] as i64;
                token_type_ids[[i, j]] = types[j] as i64;
            }
        }

        // Convert ndarray to Vec for ort compatibility
        let shape = [batch_size, max_len];
        let input_ids_vec: Vec<i64> = input_ids.into_raw_vec_and_offset().0;
        let attention_mask_vec: Vec<i64> = attention_mask.clone().into_raw_vec_and_offset().0;
        let token_type_ids_vec: Vec<i64> = token_type_ids.into_raw_vec_and_offset().0;

        // Create ONNX input tensors using tuple format (shape, data)
        let input_ids_tensor = Tensor::from_array((shape, input_ids_vec))?;
        let attention_mask_tensor = Tensor::from_array((shape, attention_mask_vec))?;
        let token_type_ids_tensor = Tensor::from_array((shape, token_type_ids_vec))?;

        // Run ONNX inference
        let outputs = self.session.run(ort::inputs![
            "input_ids" => input_ids_tensor,
            "attention_mask" => attention_mask_tensor,
            "token_type_ids" => token_type_ids_tensor,
        ])?;

        // Extract last hidden state (batch_size, seq_len, hidden_size)
        let (shape, hidden_state_data) = outputs[0]
            .try_extract_tensor::<f32>()?;

        let hidden_size = shape[2] as usize;
        let seq_len_out = shape[1] as usize;

        debug!(
            "Output shape: batch={}, seq={}, hidden={}",
            shape[0], shape[1], hidden_size
        );

        // Mean pooling with attention mask
        let mut embeddings = Vec::with_capacity(batch_size);

        for i in 0..batch_size {
            let embedding = mean_pool_from_flat(
                hidden_state_data,
                &attention_mask,
                i,
                seq_len_out,
                hidden_size,
            );

            // L2 normalize
            let normalized = l2_normalize(&embedding);
            embeddings.push(normalized);
        }

        Ok(embeddings)
    }

    /// Get the embedding dimension
    pub fn dim(&self) -> usize {
        self.config.embedding_dim
    }

    /// Get the model path
    pub fn model_path(&self) -> &Path {
        &self.config.model_path
    }
}

/// Mean pooling for a single item in the batch from flat tensor data
///
/// The hidden_state is a flat array of shape [batch, seq_len, hidden_size]
fn mean_pool_from_flat(
    hidden_state: &[f32],
    attention_mask: &Array2<i64>,
    batch_idx: usize,
    seq_len: usize,
    hidden_size: usize,
) -> Vec<f32> {
    let mut sum = vec![0.0f32; hidden_size];
    let mut count = 0.0f32;

    let batch_offset = batch_idx * seq_len * hidden_size;

    for j in 0..seq_len {
        let mask = attention_mask[[batch_idx, j]] as f32;
        if mask > 0.0 {
            let seq_offset = batch_offset + j * hidden_size;
            for k in 0..hidden_size {
                sum[k] += hidden_state[seq_offset + k] * mask;
            }
            count += mask;
        }
    }

    // Average
    if count > 0.0 {
        for val in &mut sum {
            *val /= count;
        }
    }

    sum
}

/// L2 normalize a vector
fn l2_normalize(vec: &[f32]) -> Vec<f32> {
    let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm > 0.0 {
        vec.iter().map(|x| x / norm).collect()
    } else {
        vec.to_vec()
    }
}

/// Compute cosine similarity between two embeddings
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "Embedding dimensions must match");

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();

    // If vectors are already L2 normalized, dot product is cosine similarity
    dot
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_l2_normalize() {
        let vec = vec![3.0, 4.0];
        let normalized = l2_normalize(&vec);

        // 3-4-5 triangle
        assert!((normalized[0] - 0.6).abs() < 0.001);
        assert!((normalized[1] - 0.8).abs() < 0.001);

        // Check norm is 1
        let norm: f32 = normalized.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let a = l2_normalize(&vec![1.0, 2.0, 3.0]);
        let b = a.clone();
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = l2_normalize(&vec![1.0, 0.0]);
        let b = l2_normalize(&vec![0.0, 1.0]);
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 0.001);
    }
}
