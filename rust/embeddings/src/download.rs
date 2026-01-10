//! Model download functionality
//!
//! Downloads the all-MiniLM-L6-v2 model from HuggingFace Hub.

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use tracing::info;

use crate::{EmbeddingError, Result};

/// URLs for model files on HuggingFace Hub
const MODEL_URL: &str = "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/onnx/model.onnx";
const TOKENIZER_URL: &str = "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/tokenizer.json";

/// Paths to model files
#[derive(Debug, Clone)]
pub struct ModelPaths {
    pub model_path: PathBuf,
    pub tokenizer_path: PathBuf,
    pub model_dir: PathBuf,
}

impl ModelPaths {
    /// Get the default model directory
    pub fn default_dir() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join(".diachron")
            .join("models")
            .join("all-MiniLM-L6-v2")
    }

    /// Create paths for the default directory
    pub fn default() -> Self {
        let model_dir = Self::default_dir();
        Self {
            model_path: model_dir.join("model.onnx"),
            tokenizer_path: model_dir.join("tokenizer.json"),
            model_dir,
        }
    }

    /// Check if all required files exist
    pub fn exists(&self) -> bool {
        self.model_path.exists() && self.tokenizer_path.exists()
    }
}

/// Ensure the model exists, downloading if necessary
///
/// Returns the paths to the model files.
pub fn ensure_model_exists() -> Result<ModelPaths> {
    let paths = ModelPaths::default();

    if paths.exists() {
        info!("Model already exists at {:?}", paths.model_dir);
        return Ok(paths);
    }

    info!("Model not found, downloading...");
    download_model(&paths)?;

    Ok(paths)
}

/// Download the model files from HuggingFace Hub
fn download_model(paths: &ModelPaths) -> Result<()> {
    // Create model directory
    fs::create_dir_all(&paths.model_dir).map_err(|e| {
        EmbeddingError::DownloadFailed(format!("Failed to create model directory: {}", e))
    })?;

    // Download model.onnx (~90MB)
    info!("Downloading model.onnx (this may take a minute)...");
    download_file(MODEL_URL, &paths.model_path)?;

    // Download tokenizer.json (~700KB)
    info!("Downloading tokenizer.json...");
    download_file(TOKENIZER_URL, &paths.tokenizer_path)?;

    info!("Model download complete!");
    Ok(())
}

/// Download a single file
fn download_file(url: &str, dest: &PathBuf) -> Result<()> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(300)) // 5 minute timeout for large files
        .build()
        .map_err(|e| EmbeddingError::DownloadFailed(format!("Failed to create HTTP client: {}", e)))?;

    let response = client
        .get(url)
        .send()
        .map_err(|e| EmbeddingError::DownloadFailed(format!("HTTP request failed: {}", e)))?;

    if !response.status().is_success() {
        return Err(EmbeddingError::DownloadFailed(format!(
            "HTTP error: {}",
            response.status()
        )));
    }

    let bytes = response
        .bytes()
        .map_err(|e| EmbeddingError::DownloadFailed(format!("Failed to read response: {}", e)))?;

    let mut file = fs::File::create(dest).map_err(|e| {
        EmbeddingError::DownloadFailed(format!("Failed to create file: {}", e))
    })?;

    file.write_all(&bytes).map_err(|e| {
        EmbeddingError::DownloadFailed(format!("Failed to write file: {}", e))
    })?;

    let size_mb = bytes.len() as f64 / 1024.0 / 1024.0;
    info!("Downloaded {} ({:.1} MB)", dest.display(), size_mb);

    Ok(())
}

/// Check if the model is downloaded
pub fn is_model_downloaded() -> bool {
    ModelPaths::default().exists()
}

/// Get the size of the downloaded model in bytes
pub fn model_size() -> Option<u64> {
    let paths = ModelPaths::default();
    if !paths.exists() {
        return None;
    }

    let model_size = fs::metadata(&paths.model_path).ok()?.len();
    let tokenizer_size = fs::metadata(&paths.tokenizer_path).ok()?.len();

    Some(model_size + tokenizer_size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_paths_default() {
        let paths = ModelPaths::default();
        assert!(paths.model_path.ends_with("model.onnx"));
        assert!(paths.tokenizer_path.ends_with("tokenizer.json"));
    }
}
