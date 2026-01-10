//! Conversation summarization via Anthropic API
//!
//! Generates concise summaries of conversation exchanges for better
//! semantic search and retrieval.
//!
//! Credential resolution order:
//! 1. Config file `api_key` (if set in ~/.diachron/config.toml)
//! 2. `ANTHROPIC_API_KEY` environment variable
//! 3. Claude Code's internal credentials (future)

use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;
use tracing::{debug, warn};

/// Default model for summarization (fast + cheap)
const DEFAULT_MODEL: &str = "claude-3-haiku-20240307";
const MAX_TOKENS: u32 = 300;
const API_BASE_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";

#[derive(Error, Debug)]
pub enum SummarizationError {
    #[error("No API key available")]
    NoApiKey,
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("API error: {0}")]
    Api(String),
    #[error("Config error: {0}")]
    Config(String),
}

/// Configuration for summarization
#[derive(Debug, Clone, Deserialize)]
pub struct SummarizationConfig {
    /// API key override (optional)
    pub api_key: Option<String>,
    /// Model to use (default: claude-3-haiku)
    #[serde(default = "default_model")]
    pub model: String,
    /// Max tokens for summary (default: 300)
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    /// Whether summarization is enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

impl Default for SummarizationConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            model: default_model(),
            max_tokens: default_max_tokens(),
            enabled: default_enabled(),
        }
    }
}

fn default_model() -> String {
    DEFAULT_MODEL.to_string()
}

fn default_max_tokens() -> u32 {
    MAX_TOKENS
}

fn default_enabled() -> bool {
    true
}

/// Anthropic API request structure
#[derive(Serialize)]
struct ApiRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<ApiMessage>,
}

#[derive(Serialize)]
struct ApiMessage {
    role: String,
    content: String,
}

/// Anthropic API response structure
#[derive(Deserialize)]
struct ApiResponse {
    content: Vec<ContentBlock>,
    #[serde(default)]
    error: Option<ApiError>,
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    content_type: String,
    text: Option<String>,
}

#[derive(Deserialize)]
struct ApiError {
    message: String,
}

/// Summarizer for conversation exchanges
pub struct Summarizer {
    client: reqwest::blocking::Client,
    config: SummarizationConfig,
    api_key: Option<String>,
}

impl Summarizer {
    /// Create a new summarizer with config from file or defaults
    pub fn new(config_path: &Path) -> Self {
        let config = Self::load_config(config_path).unwrap_or_default();
        let api_key = Self::resolve_api_key(&config);

        if api_key.is_none() {
            warn!("No Anthropic API key found. Set ANTHROPIC_API_KEY env var or add to ~/.diachron/config.toml");
        }

        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            config,
            api_key,
        }
    }

    /// Load configuration from TOML file
    fn load_config(config_path: &Path) -> Option<SummarizationConfig> {
        let config_file = config_path.join("config.toml");
        if !config_file.exists() {
            return None;
        }

        let content = std::fs::read_to_string(&config_file).ok()?;

        #[derive(Deserialize)]
        struct ConfigFile {
            #[serde(default)]
            summarization: SummarizationConfig,
        }

        let parsed: ConfigFile = toml::from_str(&content).ok()?;
        Some(parsed.summarization)
    }

    /// Resolve API key from config or environment
    fn resolve_api_key(config: &SummarizationConfig) -> Option<String> {
        // 1. Check config file
        if let Some(ref key) = config.api_key {
            if !key.is_empty() {
                debug!("Using API key from config file");
                return Some(key.clone());
            }
        }

        // 2. Check environment variable
        if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
            if !key.is_empty() {
                debug!("Using API key from ANTHROPIC_API_KEY env var");
                return Some(key);
            }
        }

        // 3. Future: Try Claude Code's internal credentials
        // This would require integration with the Claude Code SDK

        None
    }

    /// Check if summarization is available (has API key and enabled)
    pub fn is_available(&self) -> bool {
        self.api_key.is_some() && self.config.enabled
    }

    /// Summarize a conversation exchange
    ///
    /// Returns a concise 1-2 sentence summary of the exchange.
    pub fn summarize(
        &self,
        user_message: &str,
        assistant_message: &str,
    ) -> Result<String, SummarizationError> {
        let api_key = self.api_key.as_ref().ok_or(SummarizationError::NoApiKey)?;

        // Truncate long messages to stay within context limits
        let user_truncated = truncate_to_chars(user_message, 2000);
        let assistant_truncated = truncate_to_chars(assistant_message, 2000);

        let prompt = format!(
            "Summarize this Claude Code conversation exchange in 1-2 concise sentences. \
            Focus on what was accomplished or discussed.\n\n\
            User: {}\n\n\
            Assistant: {}",
            user_truncated, assistant_truncated
        );

        let request = ApiRequest {
            model: self.config.model.clone(),
            max_tokens: self.config.max_tokens,
            messages: vec![ApiMessage {
                role: "user".to_string(),
                content: prompt,
            }],
        };

        let response = self
            .client
            .post(API_BASE_URL)
            .header("x-api-key", api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&request)
            .send()?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(SummarizationError::Api(format!(
                "HTTP {}: {}",
                status, body
            )));
        }

        let api_response: ApiResponse = response.json()?;

        if let Some(error) = api_response.error {
            return Err(SummarizationError::Api(error.message));
        }

        // Extract text from response
        let summary = api_response
            .content
            .into_iter()
            .filter(|c| c.content_type == "text")
            .filter_map(|c| c.text)
            .collect::<Vec<_>>()
            .join(" ");

        if summary.is_empty() {
            return Err(SummarizationError::Api("Empty response".to_string()));
        }

        Ok(summary)
    }

    /// Batch summarize multiple exchanges
    ///
    /// Returns a vector of (exchange_id, summary) tuples.
    /// Exchanges that fail to summarize are skipped with a warning.
    pub fn summarize_batch(
        &self,
        exchanges: Vec<(String, String, String)>, // (id, user_msg, assistant_msg)
    ) -> Vec<(String, String)> {
        let mut results = Vec::new();

        for (id, user_msg, assistant_msg) in exchanges {
            match self.summarize(&user_msg, &assistant_msg) {
                Ok(summary) => {
                    results.push((id, summary));
                }
                Err(e) => {
                    warn!("Failed to summarize exchange {}: {}", id, e);
                }
            }

            // Simple rate limiting: 100ms between requests
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        results
    }
}

/// Truncate string to approximately n characters at a word boundary
fn truncate_to_chars(s: &str, max_chars: usize) -> &str {
    if s.len() <= max_chars {
        return s;
    }

    // Find a safe UTF-8 boundary
    let mut end = max_chars;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }

    // Try to break at a word boundary
    if let Some(space_pos) = s[..end].rfind(char::is_whitespace) {
        &s[..space_pos]
    } else {
        &s[..end]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_to_chars() {
        assert_eq!(truncate_to_chars("hello world", 20), "hello world");
        assert_eq!(truncate_to_chars("hello world", 8), "hello");
        assert_eq!(truncate_to_chars("hello world", 5), "hello");
    }

    #[test]
    fn test_default_config() {
        let config = SummarizationConfig::default();
        assert_eq!(config.model, DEFAULT_MODEL);
        assert_eq!(config.max_tokens, MAX_TOKENS);
        assert!(config.enabled);
        assert!(config.api_key.is_none());
    }

    #[test]
    fn test_resolve_api_key_from_env() {
        // Save original env var
        let original = std::env::var("ANTHROPIC_API_KEY").ok();

        // Set test key
        std::env::set_var("ANTHROPIC_API_KEY", "test-key-123");
        let config = SummarizationConfig::default();
        let key = Summarizer::resolve_api_key(&config);
        assert_eq!(key, Some("test-key-123".to_string()));

        // Restore original
        match original {
            Some(val) => std::env::set_var("ANTHROPIC_API_KEY", val),
            None => std::env::remove_var("ANTHROPIC_API_KEY"),
        }
    }
}
