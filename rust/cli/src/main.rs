//! Diachron CLI
//!
//! Thin client that communicates with the daemon via Unix socket.
//! Designed for <5ms execution time.
//!
//! Commands:
//! - diachron timeline [--since "1h"] [--file src/]
//! - diachron capture <json>         # Called by hook
//! - diachron memory search "query"
//! - diachron memory index
//! - diachron daemon start|stop|status
//! - diachron doctor

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use diachron_core::{IpcMessage, IpcResponse};

#[derive(Parser)]
#[command(name = "diachron")]
#[command(about = "Provenance tracking and memory for AI-assisted development")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// View change timeline
    Timeline {
        /// Show events since this time (e.g., "1h", "yesterday", "2024-01-01")
        #[arg(long)]
        since: Option<String>,

        /// Filter by file path
        #[arg(long)]
        file: Option<String>,

        /// Maximum number of events to show
        #[arg(long, default_value = "20")]
        limit: usize,

        /// Output format: text, json, csv, markdown
        #[arg(long, default_value = "text")]
        format: String,
    },

    /// Capture an event (called by hook)
    Capture {
        /// JSON event data
        json: String,
    },

    /// Memory operations
    Memory {
        #[command(subcommand)]
        command: MemoryCommands,
    },

    /// Daemon management
    Daemon {
        #[command(subcommand)]
        command: DaemonCommands,
    },

    /// Search across events and memory
    Search {
        /// Search query
        query: String,

        /// Maximum results
        #[arg(long, default_value = "10")]
        limit: usize,

        /// Filter by source: event, exchange, or all
        #[arg(long, value_name = "TYPE")]
        r#type: Option<String>,

        /// Filter by time (e.g., "1h", "7d", "2024-01-01")
        #[arg(long)]
        since: Option<String>,

        /// Filter by project name
        #[arg(long)]
        project: Option<String>,

        /// Output format: text, json, csv, markdown
        #[arg(long, default_value = "text")]
        format: String,
    },

    /// Run diagnostics
    Doctor,

    /// Configuration management
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },
}

#[derive(Subcommand)]
enum ConfigCommands {
    /// List all configuration settings
    List,

    /// Get a configuration value
    Get {
        /// Config key (e.g., "summarization.enabled")
        key: String,
    },

    /// Set a configuration value
    Set {
        /// Config key (e.g., "summarization.enabled")
        key: String,

        /// Value to set
        value: String,
    },

    /// Open configuration file in editor
    Edit,
}

#[derive(Subcommand)]
enum MemoryCommands {
    /// Search conversation memory
    Search {
        /// Search query
        query: String,

        /// Maximum results
        #[arg(long, default_value = "10")]
        limit: usize,
    },

    /// Index pending conversations
    Index,

    /// Summarize exchanges (requires Anthropic API key)
    Summarize {
        /// Maximum exchanges to summarize
        #[arg(long, default_value = "100")]
        limit: usize,
    },

    /// Show memory statistics
    Status,
}

#[derive(Subcommand)]
enum DaemonCommands {
    /// Start the daemon
    Start,

    /// Stop the daemon
    Stop,

    /// Check daemon status
    Status,
}

fn socket_path() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".diachron/diachron.sock"))
        .unwrap_or_else(|| PathBuf::from("/tmp/.diachron/diachron.sock"))
}

fn send_message(msg: &IpcMessage) -> Result<IpcResponse> {
    let path = socket_path();

    let mut stream = UnixStream::connect(&path)
        .with_context(|| format!("Failed to connect to daemon at {:?}", path))?;

    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;

    let json = serde_json::to_string(msg)? + "\n";
    stream.write_all(json.as_bytes())?;

    let mut reader = BufReader::new(stream);
    let mut response = String::new();
    reader.read_line(&mut response)?;

    let response: IpcResponse = serde_json::from_str(&response)?;
    Ok(response)
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Timeline {
            since,
            file,
            limit,
            format,
        } => {
            let msg = IpcMessage::Timeline {
                since,
                file_filter: file,
                limit,
            };

            match send_message(&msg) {
                Ok(IpcResponse::Events(events)) => {
                    if events.is_empty() {
                        if format == "text" {
                            println!("No events found");
                        } else if format == "json" {
                            println!("[]");
                        }
                        // CSV/markdown: just output headers with no data
                    } else {
                        match format.as_str() {
                            "json" => {
                                println!("{}", serde_json::to_string_pretty(&events).unwrap());
                            }
                            "csv" => {
                                println!("timestamp,tool_name,file_path,operation,session_id");
                                for event in events {
                                    println!(
                                        "\"{}\",\"{}\",\"{}\",\"{}\",\"{}\"",
                                        event.timestamp,
                                        event.tool_name,
                                        event.file_path.as_deref().unwrap_or(""),
                                        event.operation.as_deref().unwrap_or(""),
                                        event.session_id.as_deref().unwrap_or("")
                                    );
                                }
                            }
                            "markdown" | "md" => {
                                println!("| Timestamp | Tool | File | Operation |");
                                println!("|-----------|------|------|-----------|");
                                for event in events {
                                    println!(
                                        "| {} | {} | {} | {} |",
                                        event
                                            .timestamp_display
                                            .as_deref()
                                            .unwrap_or(&event.timestamp),
                                        event.tool_name,
                                        event.file_path.as_deref().unwrap_or("-"),
                                        event.operation.as_deref().unwrap_or("-")
                                    );
                                }
                            }
                            _ => {
                                // Default: text format
                                for event in events {
                                    println!(
                                        "{} {} {}",
                                        event
                                            .timestamp_display
                                            .as_deref()
                                            .unwrap_or(&event.timestamp),
                                        event.tool_name,
                                        event.file_path.as_deref().unwrap_or("-")
                                    );
                                }
                            }
                        }
                    }
                }
                Ok(IpcResponse::Error(e)) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
                Ok(_) => {
                    eprintln!("Unexpected response");
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!("Failed to communicate with daemon: {}", e);
                    eprintln!("Is the daemon running? Try: diachron daemon start");
                    std::process::exit(1);
                }
            }
        }

        Commands::Capture { json } => {
            let event: diachron_core::CaptureEvent =
                serde_json::from_str(&json).context("Invalid event JSON")?;

            let msg = IpcMessage::Capture(event);

            match send_message(&msg) {
                Ok(IpcResponse::Ok) => {}
                Ok(IpcResponse::Error(e)) => {
                    eprintln!("Capture error: {}", e);
                    std::process::exit(1);
                }
                Ok(_) => {}
                Err(e) => {
                    // Silently fail for hook - don't break the user's workflow
                    eprintln!("Warning: {}", e);
                }
            }
        }

        Commands::Memory { command } => match command {
            MemoryCommands::Search { query, limit } => {
                let msg = IpcMessage::Search {
                    query,
                    limit,
                    source_filter: Some(diachron_core::SearchSource::Exchange),
                    since: None,
                    project: None,
                };

                match send_message(&msg) {
                    Ok(IpcResponse::SearchResults(results)) => {
                        if results.is_empty() {
                            println!("No results found");
                        } else {
                            for result in results {
                                println!(
                                    "[{:.2}] {} - {}",
                                    result.score, result.timestamp, result.snippet
                                );
                            }
                        }
                    }
                    Ok(IpcResponse::Error(e)) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                    Ok(_) => {}
                    Err(e) => {
                        eprintln!("Failed: {}", e);
                        std::process::exit(1);
                    }
                }
            }

            MemoryCommands::Index => {
                let msg = IpcMessage::IndexConversations;
                match send_message(&msg) {
                    Ok(IpcResponse::Ok) => {
                        println!("Indexing started");
                    }
                    Ok(IpcResponse::Error(e)) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                    Ok(_) => {}
                    Err(e) => {
                        eprintln!("Failed: {}", e);
                        std::process::exit(1);
                    }
                }
            }

            MemoryCommands::Status => {
                let msg = IpcMessage::DoctorInfo;
                match send_message(&msg) {
                    Ok(IpcResponse::Doctor(info)) => {
                        println!("Memory Status");
                        println!("=============\n");

                        println!("Conversation Index:");
                        println!("  Exchanges indexed: {}", info.exchanges_count);
                        println!("  Vector embeddings: {} vectors", info.exchanges_index_count);
                        println!(
                            "  Index size: {:.1} MB",
                            info.exchanges_index_size_bytes as f64 / 1024.0 / 1024.0
                        );

                        println!("\nCode Events:");
                        println!("  Events captured: {}", info.events_count);
                        println!("  Vector embeddings: {} vectors", info.events_index_count);
                        println!(
                            "  Index size: {:.1} KB",
                            info.events_index_size_bytes as f64 / 1024.0
                        );

                        println!("\nStorage:");
                        println!(
                            "  Database: {:.1} MB",
                            info.database_size_bytes as f64 / 1024.0 / 1024.0
                        );
                        println!(
                            "  Total index: {:.1} MB",
                            (info.events_index_size_bytes + info.exchanges_index_size_bytes) as f64
                                / 1024.0
                                / 1024.0
                        );

                        println!("\nEmbedding Model:");
                        if info.model_loaded {
                            println!("  Status: ✓ loaded");
                        } else {
                            println!("  Status: ✗ not loaded (will load on first search)");
                        }
                        if info.model_size_bytes > 0 {
                            println!(
                                "  Size: {:.1} MB",
                                info.model_size_bytes as f64 / 1024.0 / 1024.0
                            );
                        }

                        println!("\nDaemon:");
                        println!("  Uptime: {}s", info.uptime_secs);
                        println!(
                            "  Memory (RSS): {:.1} MB",
                            info.memory_rss_bytes as f64 / 1024.0 / 1024.0
                        );
                    }
                    Ok(IpcResponse::Pong {
                        uptime_secs,
                        events_count,
                    }) => {
                        // Fallback for older daemon without DoctorInfo
                        println!("Memory Status (limited - daemon too old)");
                        println!("=========================================\n");
                        println!("Events captured: {}", events_count);
                        println!("Uptime: {}s", uptime_secs);
                        println!("\nUpgrade daemon for full stats: diachron daemon stop && diachron daemon start");
                    }
                    Ok(IpcResponse::Error(e)) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                    Ok(_) => {
                        eprintln!("Unexpected response");
                        std::process::exit(1);
                    }
                    Err(e) => {
                        eprintln!("Failed to communicate with daemon: {}", e);
                        eprintln!("Is the daemon running? Try: diachron daemon start");
                        std::process::exit(1);
                    }
                }
            }

            MemoryCommands::Summarize { limit } => {
                let msg = IpcMessage::SummarizeExchanges { limit };
                // Use longer timeout for summarization (can take a while)
                let path = socket_path();
                let mut stream = std::os::unix::net::UnixStream::connect(&path)
                    .context("Failed to connect to daemon")?;
                stream.set_read_timeout(Some(Duration::from_secs(300)))?; // 5 min timeout
                stream.set_write_timeout(Some(Duration::from_secs(5)))?;

                let json = serde_json::to_string(&msg)? + "\n";
                use std::io::Write;
                stream.write_all(json.as_bytes())?;

                let mut reader = std::io::BufReader::new(stream);
                let mut response = String::new();
                use std::io::BufRead;
                reader.read_line(&mut response)?;

                let response: IpcResponse = serde_json::from_str(&response)?;
                match response {
                    IpcResponse::SummarizeStats {
                        summarized,
                        skipped,
                        errors,
                    } => {
                        println!("Summarization complete:");
                        println!("  Summarized: {}", summarized);
                        println!("  Skipped: {}", skipped);
                        println!("  Errors: {}", errors);
                    }
                    IpcResponse::Error(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                    _ => {
                        eprintln!("Unexpected response");
                        std::process::exit(1);
                    }
                }
            }
        },

        Commands::Daemon { command } => match command {
            DaemonCommands::Start => {
                // Check if already running
                let socket = socket_path();
                if socket.exists() {
                    if let Ok(IpcResponse::Pong { .. }) = send_message(&IpcMessage::Ping) {
                        println!("Daemon is already running");
                        return Ok(());
                    }
                    // Stale socket file - remove it
                    let _ = std::fs::remove_file(&socket);
                }

                // Find daemon binary (same directory as CLI)
                let daemon_path = std::env::current_exe()?
                    .parent()
                    .map(|p| p.join("diachrond"))
                    .context("Could not determine executable directory")?;

                if !daemon_path.exists() {
                    eprintln!("Daemon binary not found at {:?}", daemon_path);
                    eprintln!("Hint: Build with 'cargo build --release' first");
                    std::process::exit(1);
                }

                // Create logs directory
                let diachron_home = dirs::home_dir()
                    .map(|h| h.join(".diachron"))
                    .unwrap_or_else(|| PathBuf::from("/tmp/.diachron"));
                let logs_dir = diachron_home.join("logs");
                std::fs::create_dir_all(&logs_dir).ok();

                // Start daemon process
                use std::process::{Command, Stdio};
                let log_file = std::fs::File::create(logs_dir.join("daemon.log"))
                    .context("Failed to create log file")?;
                let err_file = std::fs::File::create(logs_dir.join("daemon.err"))
                    .context("Failed to create error log file")?;

                let child = Command::new(&daemon_path)
                    .stdout(Stdio::from(log_file))
                    .stderr(Stdio::from(err_file))
                    .spawn()
                    .context("Failed to start daemon")?;

                // Write PID file
                let pid_file = diachron_home.join("daemon.pid");
                std::fs::write(&pid_file, child.id().to_string())
                    .context("Failed to write PID file")?;

                println!("Daemon started with PID {}", child.id());
                println!("Logs: {}", logs_dir.display());

                // Wait a moment and verify it's running
                std::thread::sleep(Duration::from_millis(500));
                if let Ok(IpcResponse::Pong { .. }) = send_message(&IpcMessage::Ping) {
                    println!("Daemon is running and responding");
                } else {
                    eprintln!("Warning: Daemon started but not responding yet");
                    eprintln!("Check logs: {}", logs_dir.join("daemon.err").display());
                }
            }

            DaemonCommands::Stop => {
                let msg = IpcMessage::Shutdown;
                match send_message(&msg) {
                    Ok(IpcResponse::Ok) => {
                        println!("Daemon stopped");
                    }
                    Ok(IpcResponse::Error(e)) => {
                        eprintln!("Error: {}", e);
                    }
                    Ok(_) => {}
                    Err(_) => {
                        println!("Daemon is not running");
                    }
                }
            }

            DaemonCommands::Status => {
                let msg = IpcMessage::Ping;
                match send_message(&msg) {
                    Ok(IpcResponse::Pong {
                        uptime_secs,
                        events_count,
                    }) => {
                        println!("Daemon: Running");
                        println!("Uptime: {}s", uptime_secs);
                        println!("Events captured: {}", events_count);
                    }
                    Ok(IpcResponse::Error(e)) => {
                        eprintln!("Daemon error: {}", e);
                    }
                    Ok(_) => {}
                    Err(_) => {
                        println!("Daemon: Not running");
                    }
                }
            }
        },

        Commands::Search {
            query,
            limit,
            r#type,
            since,
            project,
            format,
        } => {
            let source_filter = r#type.and_then(|t| match t.as_str() {
                "event" => Some(diachron_core::SearchSource::Event),
                "exchange" => Some(diachron_core::SearchSource::Exchange),
                _ => None,
            });

            let msg = IpcMessage::Search {
                query,
                limit,
                source_filter,
                since,
                project,
            };

            match send_message(&msg) {
                Ok(IpcResponse::SearchResults(results)) => {
                    if results.is_empty() {
                        if format == "text" {
                            println!("No results found");
                        } else if format == "json" {
                            println!("[]");
                        }
                    } else {
                        match format.as_str() {
                            "json" => {
                                println!("{}", serde_json::to_string_pretty(&results).unwrap());
                            }
                            "csv" => {
                                println!("score,source,timestamp,project,snippet");
                                for result in results {
                                    let source_str = match result.source {
                                        diachron_core::SearchSource::Event => "event",
                                        diachron_core::SearchSource::Exchange => "exchange",
                                    };
                                    // Escape quotes in snippet for CSV
                                    let snippet_escaped = result.snippet.replace('"', "\"\"");
                                    println!(
                                        "{:.4},\"{}\",\"{}\",\"{}\",\"{}\"",
                                        result.score,
                                        source_str,
                                        result.timestamp,
                                        result.project.as_deref().unwrap_or(""),
                                        snippet_escaped
                                    );
                                }
                            }
                            "markdown" | "md" => {
                                println!("| Score | Source | Timestamp | Project | Snippet |");
                                println!("|-------|--------|-----------|---------|---------|");
                                for result in results {
                                    let source_str = match result.source {
                                        diachron_core::SearchSource::Event => "Event",
                                        diachron_core::SearchSource::Exchange => "Exchange",
                                    };
                                    // Escape pipes in snippet for markdown
                                    let snippet_escaped =
                                        result.snippet.replace('|', "\\|").replace('\n', " ");
                                    println!(
                                        "| {:.2} | {} | {} | {} | {} |",
                                        result.score,
                                        source_str,
                                        result.timestamp,
                                        result.project.as_deref().unwrap_or("-"),
                                        snippet_escaped
                                    );
                                }
                            }
                            _ => {
                                // Default: text format
                                for result in results {
                                    let source_str = match result.source {
                                        diachron_core::SearchSource::Event => "Event",
                                        diachron_core::SearchSource::Exchange => "Exchange",
                                    };
                                    let proj_str = result.project.as_deref().unwrap_or("-");
                                    println!(
                                        "[{:.2}] {} {} ({}) - {}",
                                        result.score,
                                        source_str,
                                        result.timestamp,
                                        proj_str,
                                        result.snippet
                                    );
                                }
                            }
                        }
                    }
                }
                Ok(IpcResponse::Error(e)) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
                Ok(_) => {}
                Err(e) => {
                    eprintln!("Failed: {}", e);
                    std::process::exit(1);
                }
            }
        }

        Commands::Doctor => {
            println!("Diachron Diagnostics");
            println!("====================\n");

            // Check socket
            let path = socket_path();
            println!("Socket: {:?}", path);
            if path.exists() {
                println!("  Status: ✓ exists");
            } else {
                println!("  Status: ✗ not found");
            }

            // Get comprehensive diagnostics from daemon
            println!("\nDaemon:");
            let msg = IpcMessage::DoctorInfo;
            match send_message(&msg) {
                Ok(IpcResponse::Doctor(info)) => {
                    println!("  Status: ✓ running");
                    println!("  Uptime: {}s", info.uptime_secs);
                    println!("  Memory: {:.1} MB (RSS)", info.memory_rss_bytes as f64 / 1024.0 / 1024.0);

                    println!("\nDatabase:");
                    println!("  Events: {}", info.events_count);
                    println!("  Exchanges: {}", info.exchanges_count);
                    println!("  Size: {:.1} MB", info.database_size_bytes as f64 / 1024.0 / 1024.0);

                    println!("\nVector Indexes:");
                    println!("  Events: {} vectors ({:.1} KB)",
                        info.events_index_count,
                        info.events_index_size_bytes as f64 / 1024.0
                    );
                    println!("  Exchanges: {} vectors ({:.1} MB)",
                        info.exchanges_index_count,
                        info.exchanges_index_size_bytes as f64 / 1024.0 / 1024.0
                    );

                    println!("\nEmbedding Model:");
                    if info.model_loaded {
                        println!("  Status: ✓ loaded");
                    } else {
                        println!("  Status: ✗ not loaded");
                    }
                    if info.model_size_bytes > 0 {
                        println!("  Size: {:.1} MB", info.model_size_bytes as f64 / 1024.0 / 1024.0);
                    } else {
                        println!("  Size: not found (run search to trigger download)");
                    }
                }
                Ok(IpcResponse::Pong { uptime_secs, events_count }) => {
                    // Fallback if daemon doesn't support DoctorInfo yet
                    println!("  Status: ✓ running (legacy)");
                    println!("  Uptime: {}s", uptime_secs);
                    println!("  Events: {}", events_count);
                }
                Ok(IpcResponse::Error(e)) => {
                    println!("  Status: ✗ error: {}", e);
                }
                Ok(_) => {
                    println!("  Status: ? unexpected response");
                }
                Err(e) => {
                    println!("  Status: ✗ not running");
                    println!("  Error: {}", e);
                    println!("  Hint: Start with 'diachron daemon start'");
                }
            }

            // Check hook binary
            let hook_path = dirs::home_dir()
                .map(|h| h.join(".claude/skills/diachron/rust/target/release/diachron-hook"));
            if let Some(path) = hook_path {
                println!("\nHook Binary:");
                if path.exists() {
                    if let Ok(meta) = std::fs::metadata(&path) {
                        println!("  Status: ✓ {:?}", path);
                        println!("  Size: {:.1} MB", meta.len() as f64 / 1024.0 / 1024.0);
                    } else {
                        println!("  Status: ✓ {:?}", path);
                    }
                } else {
                    println!("  Status: ✗ not found");
                    println!("  Path: {:?}", path);
                }
            }

            println!("\n--- End Diagnostics ---");
        }

        Commands::Config { command } => {
            let diachron_home = dirs::home_dir()
                .map(|h| h.join(".diachron"))
                .unwrap_or_else(|| PathBuf::from("/tmp/.diachron"));
            let config_path = diachron_home.join("config.toml");

            match command {
                ConfigCommands::List => {
                    println!("Configuration: {:?}\n", config_path);

                    if config_path.exists() {
                        let content = std::fs::read_to_string(&config_path)
                            .context("Failed to read config file")?;
                        println!("{}", content);
                    } else {
                        println!("No config file found. Using defaults.\n");
                        println!("Default settings:");
                        println!("  [summarization]");
                        println!("  enabled = true");
                        println!("  model = \"claude-3-haiku-20240307\"");
                        println!("  max_tokens = 300");
                        println!("\nCreate config with: diachron config set <key> <value>");
                    }
                }

                ConfigCommands::Get { key } => {
                    if !config_path.exists() {
                        eprintln!("Config file not found. Using defaults.");
                        // Print default for known keys
                        match key.as_str() {
                            "summarization.enabled" => println!("true"),
                            "summarization.model" => println!("claude-3-haiku-20240307"),
                            "summarization.max_tokens" => println!("300"),
                            _ => eprintln!("Unknown key: {}", key),
                        }
                        return Ok(());
                    }

                    let content = std::fs::read_to_string(&config_path)
                        .context("Failed to read config file")?;
                    let config: toml::Value = toml::from_str(&content)
                        .context("Failed to parse config file")?;

                    // Navigate nested keys like "summarization.enabled"
                    let parts: Vec<&str> = key.split('.').collect();
                    let mut current = &config;
                    for part in &parts {
                        match current.get(part) {
                            Some(v) => current = v,
                            None => {
                                eprintln!("Key not found: {}", key);
                                std::process::exit(1);
                            }
                        }
                    }
                    println!("{}", current);
                }

                ConfigCommands::Set { key, value } => {
                    // Ensure config directory exists
                    std::fs::create_dir_all(&diachron_home).ok();

                    // Load existing config or start fresh
                    let mut config: toml::map::Map<String, toml::Value> = if config_path.exists() {
                        let content = std::fs::read_to_string(&config_path)
                            .context("Failed to read config file")?;
                        match toml::from_str(&content) {
                            Ok(toml::Value::Table(t)) => t,
                            _ => toml::map::Map::new(),
                        }
                    } else {
                        toml::map::Map::new()
                    };

                    // Navigate to set nested key like "summarization.enabled"
                    let parts: Vec<&str> = key.split('.').collect();
                    if parts.len() == 1 {
                        // Top-level key
                        config.insert(key.clone(), parse_toml_value(&value));
                    } else if parts.len() == 2 {
                        // Nested key (e.g., "summarization.enabled")
                        let section = parts[0];
                        let subkey = parts[1];

                        let section_table = config
                            .entry(section.to_string())
                            .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));

                        if let toml::Value::Table(ref mut t) = section_table {
                            t.insert(subkey.to_string(), parse_toml_value(&value));
                        }
                    } else {
                        eprintln!("Keys deeper than 2 levels not supported");
                        std::process::exit(1);
                    }

                    // Write back
                    let new_content = toml::to_string_pretty(&toml::Value::Table(config))
                        .context("Failed to serialize config")?;
                    std::fs::write(&config_path, new_content)
                        .context("Failed to write config file")?;

                    println!("Set {} = {}", key, value);
                    println!("Restart daemon for changes to take effect: diachron daemon stop && diachron daemon start");
                }

                ConfigCommands::Edit => {
                    // Create default config if it doesn't exist
                    if !config_path.exists() {
                        std::fs::create_dir_all(&diachron_home).ok();
                        let default_config = r#"# Diachron Configuration

[summarization]
# API key for Anthropic (optional - uses ANTHROPIC_API_KEY env var if not set)
# api_key = "sk-ant-..."

# Model for summarization
model = "claude-3-haiku-20240307"

# Maximum tokens for summaries
max_tokens = 300

# Enable/disable summarization
enabled = true
"#;
                        std::fs::write(&config_path, default_config)
                            .context("Failed to create config file")?;
                    }

                    // Open in editor
                    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "nano".to_string());
                    let status = std::process::Command::new(&editor)
                        .arg(&config_path)
                        .status()
                        .context("Failed to open editor")?;

                    if status.success() {
                        println!("Config saved. Restart daemon for changes to take effect.");
                    }
                }
            }
        }
    }

    Ok(())
}

/// Parse a string value into appropriate TOML type
fn parse_toml_value(s: &str) -> toml::Value {
    // Try boolean
    if s == "true" {
        return toml::Value::Boolean(true);
    }
    if s == "false" {
        return toml::Value::Boolean(false);
    }
    // Try integer
    if let Ok(n) = s.parse::<i64>() {
        return toml::Value::Integer(n);
    }
    // Try float
    if let Ok(f) = s.parse::<f64>() {
        return toml::Value::Float(f);
    }
    // Default to string
    toml::Value::String(s.to_string())
}
