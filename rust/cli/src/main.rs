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
                                event.timestamp_display.as_deref().unwrap_or(&event.timestamp),
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
        },

        Commands::Daemon { command } => match command {
            DaemonCommands::Start => {
                println!("Starting daemon...");
                // TODO: Fork and exec diachrond
                println!("Daemon start not yet implemented");
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

        Commands::Search { query, limit, r#type } => {
            let source_filter = r#type.and_then(|t| match t.as_str() {
                "event" => Some(diachron_core::SearchSource::Event),
                "exchange" => Some(diachron_core::SearchSource::Exchange),
                _ => None,
            });

            let msg = IpcMessage::Search {
                query,
                limit,
                source_filter,
            };

            match send_message(&msg) {
                Ok(IpcResponse::SearchResults(results)) => {
                    if results.is_empty() {
                        println!("No results found");
                    } else {
                        for result in results {
                            println!(
                                "[{:.2}] {:?} {} - {}",
                                result.score, result.source, result.timestamp, result.snippet
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
            println!("Socket path: {:?}", path);
            if path.exists() {
                println!("Socket: ✓ exists");
            } else {
                println!("Socket: ✗ not found");
            }

            // Check daemon
            println!("\nDaemon status:");
            let msg = IpcMessage::Ping;
            match send_message(&msg) {
                Ok(IpcResponse::Pong { uptime_secs, events_count }) => {
                    println!("  Status: ✓ running");
                    println!("  Uptime: {}s", uptime_secs);
                    println!("  Events: {}", events_count);
                }
                Ok(_) => {
                    println!("  Status: ? unexpected response");
                }
                Err(e) => {
                    println!("  Status: ✗ not running ({})", e);
                }
            }

            // Check v1 hook
            let hook_path = dirs::home_dir()
                .map(|h| h.join(".claude/skills/diachron/rust/target/release/diachron-hook"));
            if let Some(path) = hook_path {
                println!("\nV1 Hook:");
                if path.exists() {
                    println!("  Binary: ✓ {:?}", path);
                } else {
                    println!("  Binary: ✗ not found");
                }
            }
        }
    }

    Ok(())
}
