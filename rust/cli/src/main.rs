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
    },

    /// Run diagnostics
    Doctor,
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
        Commands::Timeline { since, file, limit } => {
            let msg = IpcMessage::Timeline {
                since,
                file_filter: file,
                limit,
            };

            match send_message(&msg) {
                Ok(IpcResponse::Events(events)) => {
                    if events.is_empty() {
                        println!("No events found");
                    } else {
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
                println!("Memory status: Not implemented yet");
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
                        println!("No results found");
                    } else {
                        for result in results {
                            // Format source as colored indicator
                            let source_str = match result.source {
                                diachron_core::SearchSource::Event => "Event",
                                diachron_core::SearchSource::Exchange => "Exchange",
                            };
                            // Show project if available
                            let proj_str = result.project.as_deref().unwrap_or("-");
                            println!(
                                "[{:.2}] {} {} ({}) - {}",
                                result.score, source_str, result.timestamp, proj_str, result.snippet
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
    }

    Ok(())
}
