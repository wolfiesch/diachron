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

use diachron_core::{verify_chain, IpcMessage, IpcResponse};

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

        /// Watch for new events in real-time (Ctrl+C to stop)
        #[arg(long)]
        watch: bool,
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

        /// Context injection mode: output formatted for session start injection
        /// Produces summarized, token-limited output suitable for additionalContext
        #[arg(long)]
        context_mode: bool,
    },

    /// Run diagnostics
    Doctor,

    /// Configuration management
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },

    /// Verify hash-chain integrity
    Verify,

    /// Export evidence pack for a PR
    ExportEvidence {
        /// Output file path (default: diachron.evidence.json)
        #[arg(long, default_value = "diachron.evidence.json")]
        output: String,

        /// PR number (if not specified, uses current branch's PR)
        #[arg(long)]
        pr: Option<u64>,

        /// Branch name (defaults to current branch)
        #[arg(long)]
        branch: Option<String>,

        /// Time window start (e.g., "7d", "2024-01-01")
        #[arg(long, default_value = "7d")]
        since: String,
    },

    /// Post PR narrative comment via gh CLI
    PrComment {
        /// PR number
        #[arg(long)]
        pr: u64,

        /// Evidence file path (default: diachron.evidence.json)
        #[arg(long, default_value = "diachron.evidence.json")]
        evidence: String,
    },

    /// Semantic blame for a file:line
    Blame {
        /// File and line (e.g., src/auth.rs:42)
        target: String,

        /// Output format: text, json
        #[arg(long, default_value = "text")]
        format: String,

        /// Blame mode: strict (HIGH only), best-effort, inferred
        #[arg(long, default_value = "strict")]
        mode: String,
    },

    /// Run database maintenance (VACUUM, ANALYZE, prune old data)
    Maintenance {
        /// Prune events/exchanges older than N days (0 = no pruning)
        #[arg(long, default_value = "0")]
        retention_days: u32,
    },

    /// Web dashboard management
    Dashboard {
        #[command(subcommand)]
        command: DashboardCommands,
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

    /// Enable daemon auto-start at login
    AutostartEnable,

    /// Disable daemon auto-start at login
    AutostartDisable,

    /// Check auto-start status
    AutostartStatus,
}

#[derive(Subcommand)]
enum DashboardCommands {
    /// Start the web dashboard
    Start {
        /// Port for the dashboard server (default: 3947)
        #[arg(long, default_value = "3947")]
        port: u16,

        /// Don't open browser automatically
        #[arg(long)]
        no_browser: bool,
    },

    /// Stop the web dashboard
    Stop,

    /// Check dashboard status
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

// ============================================================================
// Auto-start Management (launchd for macOS, systemd for Linux)
// ============================================================================

fn get_launchd_plist_path() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join("Library/LaunchAgents/com.diachron.daemon.plist"))
        .unwrap_or_else(|| PathBuf::from("/tmp/com.diachron.daemon.plist"))
}

fn get_systemd_service_path() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".config/systemd/user/diachron.service"))
        .unwrap_or_else(|| PathBuf::from("/tmp/diachron.service"))
}

fn get_install_dir() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".claude/skills/diachron"))
        .unwrap_or_else(|| PathBuf::from("/tmp/diachron"))
}

fn enable_autostart() -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        enable_launchd()?;
    }

    #[cfg(target_os = "linux")]
    {
        enable_systemd()?;
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        eprintln!("Auto-start is only supported on macOS and Linux");
        std::process::exit(1);
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn enable_launchd() -> Result<()> {
    use std::process::Command;

    let plist_template = get_install_dir().join("install/com.diachron.daemon.plist");
    let plist_dst = get_launchd_plist_path();

    if !plist_template.exists() {
        anyhow::bail!("Plist template not found at {:?}", plist_template);
    }

    // Create LaunchAgents directory
    if let Some(parent) = plist_dst.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Create logs directory
    if let Some(home) = dirs::home_dir() {
        std::fs::create_dir_all(home.join(".diachron/logs"))?;
    }

    // Read template and expand $HOME
    let template = std::fs::read_to_string(&plist_template)?;
    let home_str = dirs::home_dir()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|| "/tmp".to_string());
    let expanded = template.replace("$HOME", &home_str);

    // Write expanded plist
    std::fs::write(&plist_dst, expanded)?;

    // Unload if already loaded
    let _ = Command::new("launchctl")
        .args(["unload", &plist_dst.to_string_lossy()])
        .output();

    // Load the service
    let output = Command::new("launchctl")
        .args(["load", &plist_dst.to_string_lossy()])
        .output()?;

    if output.status.success() {
        println!("✅ Auto-start enabled (launchd)");
        println!("   Daemon will start automatically at login");
        println!("   Status: launchctl list | grep diachron");
    } else {
        eprintln!("❌ Failed to load launchd service");
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        std::process::exit(1);
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn enable_systemd() -> Result<()> {
    use std::process::Command;

    let service_template = get_install_dir().join("install/diachron.service");
    let service_dst = get_systemd_service_path();

    if !service_template.exists() {
        anyhow::bail!("Service template not found at {:?}", service_template);
    }

    // Create systemd user directory
    if let Some(parent) = service_dst.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Create logs directory
    if let Some(home) = dirs::home_dir() {
        std::fs::create_dir_all(home.join(".diachron/logs"))?;
    }

    // Copy service file
    std::fs::copy(&service_template, &service_dst)?;

    // Reload systemd
    let _ = Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .output();

    // Enable and start
    let output = Command::new("systemctl")
        .args(["--user", "enable", "--now", "diachron"])
        .output()?;

    if output.status.success() {
        println!("✅ Auto-start enabled (systemd)");
        println!("   Daemon will start automatically at login");
        println!("   Status: systemctl --user status diachron");
    } else {
        eprintln!("❌ Failed to enable systemd service");
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        std::process::exit(1);
    }

    Ok(())
}

fn disable_autostart() -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        disable_launchd()?;
    }

    #[cfg(target_os = "linux")]
    {
        disable_systemd()?;
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        eprintln!("Auto-start is only supported on macOS and Linux");
        std::process::exit(1);
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn disable_launchd() -> Result<()> {
    use std::process::Command;

    let plist_path = get_launchd_plist_path();

    if plist_path.exists() {
        // Unload the service
        let _ = Command::new("launchctl")
            .args(["unload", &plist_path.to_string_lossy()])
            .output();

        // Remove the plist file
        std::fs::remove_file(&plist_path)?;

        println!("✅ Auto-start disabled");
        println!("   Daemon will no longer start at login");
        println!("   Run 'diachron daemon start' to start manually");
    } else {
        println!("ℹ️  Auto-start was not enabled");
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn disable_systemd() -> Result<()> {
    use std::process::Command;

    let service_path = get_systemd_service_path();

    // Disable and stop
    let _ = Command::new("systemctl")
        .args(["--user", "disable", "--now", "diachron"])
        .output();

    if service_path.exists() {
        std::fs::remove_file(&service_path)?;

        // Reload systemd
        let _ = Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .output();
    }

    println!("✅ Auto-start disabled");
    println!("   Daemon will no longer start at login");
    println!("   Run 'diachron daemon start' to start manually");

    Ok(())
}

fn check_autostart_status() -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        check_launchd_status()?;
    }

    #[cfg(target_os = "linux")]
    {
        check_systemd_status()?;
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        println!("Auto-start is only supported on macOS and Linux");
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn check_launchd_status() -> Result<()> {
    use std::process::Command;

    let plist_path = get_launchd_plist_path();

    println!("=== Auto-Start Status (macOS) ===\n");

    if plist_path.exists() {
        println!("Plist installed: ✅ {}", plist_path.display());

        // Check if loaded
        let output = Command::new("launchctl")
            .args(["list"])
            .output()?;

        let list_output = String::from_utf8_lossy(&output.stdout);
        if list_output.contains("com.diachron.daemon") {
            // Parse the line to get PID and status
            for line in list_output.lines() {
                if line.contains("com.diachron.daemon") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 3 {
                        let pid = parts[0];
                        let exit_code = parts[1];
                        if pid == "-" {
                            println!("Service loaded: ✅ (not running, exit code: {})", exit_code);
                        } else {
                            println!("Service running: ✅ PID {} (exit code: {})", pid, exit_code);
                        }
                    }
                    break;
                }
            }
        } else {
            println!("Service loaded: ❌ Not in launchctl list");
            println!("\nTo enable: diachron daemon autostart-enable");
        }
    } else {
        println!("Plist installed: ❌");
        println!("\nTo enable: diachron daemon autostart-enable");
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn check_systemd_status() -> Result<()> {
    use std::process::Command;

    let service_path = get_systemd_service_path();

    println!("=== Auto-Start Status (Linux) ===\n");

    if service_path.exists() {
        println!("Service file: ✅ {}", service_path.display());

        // Check if enabled
        let output = Command::new("systemctl")
            .args(["--user", "is-enabled", "diachron"])
            .output()?;

        let enabled = String::from_utf8_lossy(&output.stdout).trim() == "enabled";
        println!("Service enabled: {}", if enabled { "✅" } else { "❌" });

        // Check if active
        let output = Command::new("systemctl")
            .args(["--user", "is-active", "diachron"])
            .output()?;

        let active = String::from_utf8_lossy(&output.stdout).trim() == "active";
        println!("Service active: {}", if active { "✅" } else { "❌" });

        if !enabled {
            println!("\nTo enable: diachron daemon autostart-enable");
        }
    } else {
        println!("Service file: ❌");
        println!("\nTo enable: diachron daemon autostart-enable");
    }

    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Timeline {
            since,
            file,
            limit,
            format,
            watch,
        } => {
            if watch {
                // Watch mode: poll for new events
                println!("📊 Watching for events... (Ctrl+C to stop)\n");

                let mut last_seen_id: i64 = 0;

                // Get initial events to find the starting point
                let msg = IpcMessage::Timeline {
                    since: since.clone(),
                    file_filter: file.clone(),
                    limit: 1,
                };
                if let Ok(IpcResponse::Events(events)) = send_message(&msg) {
                    if let Some(event) = events.first() {
                        last_seen_id = event.id;
                    }
                }

                loop {
                    // Small sleep to avoid hammering the daemon
                    std::thread::sleep(std::time::Duration::from_millis(500));

                    // Query for recent events
                    let msg = IpcMessage::Timeline {
                        since: Some("5m".to_string()), // Look back 5 minutes
                        file_filter: file.clone(),
                        limit: 50,
                    };

                    match send_message(&msg) {
                        Ok(IpcResponse::Events(events)) => {
                            // Filter to only new events (id > last_seen_id)
                            let new_events: Vec<_> = events
                                .iter()
                                .filter(|e| e.id > last_seen_id)
                                .collect();

                            for event in &new_events {
                                // Update last seen ID
                                if event.id > last_seen_id {
                                    last_seen_id = event.id;
                                }

                                // Print based on format
                                match format.as_str() {
                                    "json" => {
                                        println!("{}", serde_json::to_string(event).unwrap());
                                    }
                                    _ => {
                                        // Colored output for watch mode
                                        let op_icon = match event.operation.as_deref() {
                                            Some("create") => "✨",
                                            Some("modify") => "📝",
                                            Some("delete") => "🗑️",
                                            Some("commit") => "📦",
                                            Some("execute") => "⚡",
                                            _ => "•",
                                        };

                                        let file_display = event
                                            .file_path
                                            .as_ref()
                                            .map(|p| {
                                                // Show just filename + parent for brevity
                                                std::path::Path::new(p)
                                                    .file_name()
                                                    .map(|f| f.to_string_lossy().to_string())
                                                    .unwrap_or_else(|| p.clone())
                                            })
                                            .unwrap_or_else(|| "-".to_string());

                                        let session_short = event
                                            .session_id
                                            .as_ref()
                                            .map(|s| &s[..6.min(s.len())])
                                            .unwrap_or("-");

                                        println!(
                                            "[{}] {} {} {} - Session {}",
                                            event
                                                .timestamp_display
                                                .as_deref()
                                                .unwrap_or(&event.timestamp[11..19]),
                                            op_icon,
                                            event.tool_name,
                                            file_display,
                                            session_short
                                        );

                                        // Show diff summary if available
                                        if let Some(ref diff) = event.diff_summary {
                                            if !diff.is_empty() {
                                                println!("    └─ {}", diff);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Ok(IpcResponse::Error(e)) => {
                            eprintln!("Watch error: {}", e);
                        }
                        Err(e) => {
                            eprintln!("Connection lost: {}. Retrying...", e);
                            std::thread::sleep(std::time::Duration::from_secs(2));
                        }
                        _ => {}
                    }
                }
            } else {
                // Normal (non-watch) mode
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

            DaemonCommands::AutostartEnable => {
                enable_autostart()?;
            }

            DaemonCommands::AutostartDisable => {
                disable_autostart()?;
            }

            DaemonCommands::AutostartStatus => {
                check_autostart_status()?;
            }
        },

        Commands::Dashboard { command } => match command {
            DashboardCommands::Start { port, no_browser } => {
                // Check if daemon is running first
                if let Err(_) = send_message(&IpcMessage::Ping) {
                    eprintln!("⚠️  Daemon is not running. Starting daemon first...");
                    // Try to start daemon
                    let daemon_path = std::env::current_exe()?
                        .parent()
                        .map(|p| p.join("diachrond"))
                        .context("Could not determine executable directory")?;

                    if daemon_path.exists() {
                        let diachron_home = dirs::home_dir()
                            .context("Could not find home directory")?
                            .join(".diachron");
                        std::fs::create_dir_all(&diachron_home)?;
                        let logs_dir = diachron_home.join("logs");
                        std::fs::create_dir_all(&logs_dir)?;

                        std::process::Command::new(&daemon_path)
                            .stdout(std::fs::File::create(logs_dir.join("daemon.out"))?)
                            .stderr(std::fs::File::create(logs_dir.join("daemon.err"))?)
                            .spawn()
                            .context("Failed to start daemon")?;

                        std::thread::sleep(Duration::from_millis(500));
                    } else {
                        eprintln!("❌ Daemon binary not found. Run 'diachron daemon start' first.");
                        std::process::exit(1);
                    }
                }

                // Find dashboard directory
                let dashboard_dir = dirs::home_dir()
                    .context("Could not find home directory")?
                    .join(".claude")
                    .join("skills")
                    .join("diachron")
                    .join("dashboard");

                if !dashboard_dir.exists() {
                    eprintln!("❌ Dashboard not found at {:?}", dashboard_dir);
                    std::process::exit(1);
                }

                // Check if already running by trying to connect
                let check_url = format!("http://localhost:{}/api/health", port);
                if let Ok(response) = reqwest::blocking::get(&check_url) {
                    if response.status().is_success() {
                        println!("✅ Dashboard is already running at http://localhost:{}", port);
                        if !no_browser {
                            let _ = open::that(format!("http://localhost:{}", port));
                        }
                        return Ok(());
                    }
                }

                println!("🚀 Starting Diachron dashboard...");

                // Build if dist/ doesn't exist
                let dist_dir = dashboard_dir.join("dist");
                let proxy_dist = dashboard_dir.join("dist").join("proxy");

                if !dist_dir.exists() || !proxy_dist.exists() {
                    println!("📦 Building dashboard (first run)...");
                    let build_status = std::process::Command::new("npm")
                        .current_dir(&dashboard_dir)
                        .arg("run")
                        .arg("build")
                        .status()
                        .context("Failed to run npm build")?;

                    if !build_status.success() {
                        eprintln!("❌ Build failed. Run 'cd {} && npm install && npm run build' manually", dashboard_dir.display());
                        std::process::exit(1);
                    }
                }

                // Start the proxy server
                let diachron_home = dirs::home_dir()
                    .context("Could not find home directory")?
                    .join(".diachron");
                let pid_file = diachron_home.join("dashboard.pid");
                let log_file = diachron_home.join("logs").join("dashboard.log");
                std::fs::create_dir_all(diachron_home.join("logs"))?;

                let child = std::process::Command::new("node")
                    .current_dir(&dashboard_dir)
                    .arg("dist/proxy/server.js")
                    .env("PORT", port.to_string())
                    .stdout(std::fs::File::create(&log_file)?)
                    .stderr(std::fs::File::create(&log_file)?)
                    .spawn()
                    .context("Failed to start dashboard server")?;

                std::fs::write(&pid_file, child.id().to_string())?;

                // Wait for server to be ready
                std::thread::sleep(Duration::from_secs(2));

                // Verify it's running
                if let Ok(response) = reqwest::blocking::get(&check_url) {
                    if response.status().is_success() {
                        println!("   Proxy: http://localhost:{}", port);

                        // Get daemon stats
                        if let Ok(IpcResponse::Pong { uptime_secs, events_count }) = send_message(&IpcMessage::Ping) {
                            println!("   Daemon: Connected (uptime: {}s, {} events)", uptime_secs, events_count);
                        }

                        if !no_browser {
                            println!("   Opening browser...");
                            let _ = open::that(format!("http://localhost:{}", port));
                        }

                        println!("\n✅ Dashboard running at http://localhost:{}", port);
                        return Ok(());
                    }
                }

                eprintln!("⚠️  Dashboard started but not responding yet");
                eprintln!("   Check logs: {}", log_file.display());
            }

            DashboardCommands::Stop => {
                let diachron_home = dirs::home_dir()
                    .context("Could not find home directory")?
                    .join(".diachron");
                let pid_file = diachron_home.join("dashboard.pid");

                if pid_file.exists() {
                    let pid_str = std::fs::read_to_string(&pid_file)?;
                    if let Ok(pid) = pid_str.trim().parse::<i32>() {
                        // Kill the process
                        #[cfg(unix)]
                        {
                            let _ = std::process::Command::new("kill")
                                .arg(pid.to_string())
                                .status();
                        }
                        #[cfg(not(unix))]
                        {
                            let _ = std::process::Command::new("taskkill")
                                .args(&["/PID", &pid.to_string(), "/F"])
                                .status();
                        }
                    }
                    let _ = std::fs::remove_file(&pid_file);
                    println!("✅ Dashboard stopped");
                } else {
                    println!("Dashboard is not running");
                }
            }

            DashboardCommands::Status => {
                let diachron_home = dirs::home_dir()
                    .context("Could not find home directory")?
                    .join(".diachron");
                let pid_file = diachron_home.join("dashboard.pid");

                // Check if process is running
                let mut dashboard_running = false;
                if pid_file.exists() {
                    // Try to connect
                    if let Ok(response) = reqwest::blocking::get("http://localhost:3947/api/health") {
                        if response.status().is_success() {
                            dashboard_running = true;
                        }
                    }
                }

                if dashboard_running {
                    println!("Dashboard: Running (http://localhost:3947)");
                } else {
                    println!("Dashboard: Not running");
                    if pid_file.exists() {
                        let _ = std::fs::remove_file(&pid_file);
                    }
                }

                // Also show daemon status
                match send_message(&IpcMessage::Ping) {
                    Ok(IpcResponse::Pong { uptime_secs, events_count }) => {
                        let hours = uptime_secs / 3600;
                        let mins = (uptime_secs % 3600) / 60;
                        if hours > 0 {
                            println!("Daemon: Connected (uptime: {}h {}m)", hours, mins);
                        } else {
                            println!("Daemon: Connected (uptime: {}m)", mins);
                        }
                        println!("Events: {}", events_count);
                    }
                    _ => {
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
            context_mode,
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
                        if context_mode {
                            // Silent for context mode - no results means no context to inject
                        } else if format == "text" {
                            println!("No results found");
                        } else if format == "json" {
                            println!("[]");
                        }
                    } else if context_mode {
                        // Context injection mode: format for session start
                        format_context_output(&results);
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

        Commands::Verify => {
            println!("Diachron Hash-Chain Verification");
            println!("=================================\n");

            // Open database directly for read-only verification
            let db_path = dirs::home_dir()
                .map(|h| h.join(".diachron/diachron.db"))
                .context("Could not determine home directory")?;

            if !db_path.exists() {
                eprintln!("Database not found: {:?}", db_path);
                eprintln!("Hint: Run 'diachron daemon start' to initialize");
                std::process::exit(1);
            }

            let conn = rusqlite::Connection::open_with_flags(
                &db_path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
            .context("Failed to open database")?;

            match verify_chain(&conn) {
                Ok(result) => {
                    if result.valid {
                        println!("✅ Chain integrity verified");
                    } else {
                        println!("❌ Chain integrity FAILED");
                    }

                    println!("   Events checked: {}", result.events_checked);
                    println!("   Checkpoints: {}", result.checkpoints_checked);

                    if let Some(ref first) = result.first_event {
                        println!("   First event: {}", first);
                    }
                    if let Some(ref last) = result.last_event {
                        println!("   Last event: {}", last);
                    }
                    if let Some(ref root) = result.chain_root {
                        println!("   Chain root: {}...", &root[..8.min(root.len())]);
                    }

                    if let Some(ref bp) = result.break_point {
                        println!("\n⚠️ Break detected at event #{}", bp.event_id);
                        println!("   Timestamp: {}", bp.timestamp);
                        println!("   Expected hash: {}...", &bp.expected_hash[..16]);
                        println!("   Actual hash: {}...", &bp.actual_hash[..16]);
                        println!("\n   Recommendation: Restore from backup or contact support");
                    }

                    if !result.valid {
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("Verification failed: {}", e);
                    std::process::exit(1);
                }
            }
        }

        Commands::Maintenance { retention_days } => {
            println!("🔧 Running database maintenance...\n");

            let msg = IpcMessage::Maintenance { retention_days };
            match send_message(&msg) {
                Ok(IpcResponse::MaintenanceStats {
                    size_before,
                    size_after,
                    events_pruned,
                    exchanges_pruned,
                    duration_ms,
                }) => {
                    let reduction_pct = if size_before > 0 {
                        (1.0 - size_after as f64 / size_before as f64) * 100.0
                    } else {
                        0.0
                    };

                    println!(
                        "  ├─ VACUUM: {:.1} MB → {:.1} MB ({:.1}% reduction)",
                        size_before as f64 / 1024.0 / 1024.0,
                        size_after as f64 / 1024.0 / 1024.0,
                        reduction_pct
                    );
                    println!("  ├─ ANALYZE: Updated query planner stats");

                    if retention_days > 0 {
                        println!(
                            "  ├─ Old events: {} pruned (retention: {} days)",
                            events_pruned, retention_days
                        );
                        println!(
                            "  └─ Old exchanges: {} pruned (retention: {} days)",
                            exchanges_pruned, retention_days
                        );
                    } else {
                        println!("  └─ Pruning: disabled (use --retention-days to enable)");
                    }

                    println!("\n✅ Maintenance complete (took {:.1}s)", duration_ms as f64 / 1000.0);
                }
                Ok(IpcResponse::Error(e)) => {
                    eprintln!("❌ Maintenance failed: {}", e);
                    std::process::exit(1);
                }
                Ok(_) => {
                    eprintln!("❌ Unexpected response from daemon");
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!("❌ Failed to connect to daemon: {}", e);
                    eprintln!("   Hint: Start the daemon with 'diachron daemon start'");
                    std::process::exit(1);
                }
            }
        }

        Commands::ExportEvidence {
            output,
            pr,
            branch,
            since,
        } => {
            println!("Exporting evidence pack...\n");

            // Get current branch if not specified
            let branch_name = branch.unwrap_or_else(|| {
                std::process::Command::new("git")
                    .args(["branch", "--show-current"])
                    .output()
                    .ok()
                    .and_then(|o| String::from_utf8(o.stdout).ok())
                    .map(|s| s.trim().to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            });

            // Get PR number from branch if not specified
            let pr_id = pr.unwrap_or_else(|| {
                // Try to get PR number from gh CLI
                std::process::Command::new("gh")
                    .args(["pr", "view", "--json", "number", "-q", ".number"])
                    .output()
                    .ok()
                    .and_then(|o| String::from_utf8(o.stdout).ok())
                    .and_then(|s| s.trim().parse().ok())
                    .unwrap_or(0)
            });

            if pr_id == 0 {
                eprintln!("Could not determine PR number. Use --pr flag.");
                std::process::exit(1);
            }

            println!("PR: #{}", pr_id);
            println!("Branch: {}", branch_name);
            println!("Since: {}", since);

            // Get commits from git log (origin/main..HEAD)
            let commits: Vec<String> = std::process::Command::new("git")
                .args(["log", "--format=%H", "origin/main..HEAD"])
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.lines().map(|l| l.to_string()).collect())
                .unwrap_or_default();

            if commits.is_empty() {
                // Fallback: try to get commits from the last week
                let fallback_commits: Vec<String> = std::process::Command::new("git")
                    .args(["log", "--format=%H", "--since=7 days ago", &branch_name])
                    .output()
                    .ok()
                    .and_then(|o| String::from_utf8(o.stdout).ok())
                    .map(|s| s.lines().map(|l| l.to_string()).collect())
                    .unwrap_or_default();

                if fallback_commits.is_empty() {
                    eprintln!("No commits found for branch {}.", branch_name);
                    eprintln!("Make sure you have commits ahead of origin/main or use --since flag.");
                    std::process::exit(1);
                }
                println!("Found {} commits (fallback: last 7 days)", fallback_commits.len());
            } else {
                println!("Found {} commits ahead of origin/main", commits.len());
            }

            // Parse since time
            let (start_time, end_time) = parse_time_range(&since);

            println!("Time range: {} to {}", start_time, end_time);

            // Send correlation request to daemon
            let msg = IpcMessage::CorrelateEvidence {
                pr_id,
                commits: commits.clone(),
                branch: branch_name.clone(),
                start_time,
                end_time,
                intent: None, // TODO: Extract from recent conversation
            };

            match send_message(&msg) {
                Ok(IpcResponse::EvidenceResult(result)) => {
                    // Write evidence pack to file
                    let json = serde_json::to_string_pretty(&result)
                        .context("Failed to serialize evidence pack")?;

                    std::fs::write(&output, &json)
                        .context("Failed to write evidence pack")?;

                    println!("\n✅ Evidence pack written to: {}", output);
                    println!("\nSummary:");
                    println!("  Files changed: {}", result.summary.files_changed);
                    println!("  Lines: +{} / -{}", result.summary.lines_added, result.summary.lines_removed);
                    println!("  Tool operations: {}", result.summary.tool_operations);
                    println!("  Sessions: {}", result.summary.sessions);
                    println!("  Coverage: {:.1}%", result.coverage_pct);

                    if result.verification.chain_verified {
                        println!("  ✓ Hash chain verified");
                    }
                    if result.verification.tests_executed {
                        println!("  ✓ Tests executed");
                    }
                    if result.verification.build_succeeded {
                        println!("  ✓ Build succeeded");
                    }
                }
                Ok(IpcResponse::Error(e)) => {
                    eprintln!("Failed to generate evidence: {}", e);
                    std::process::exit(1);
                }
                Ok(_) => {
                    eprintln!("Unexpected response from daemon");
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!("Failed to communicate with daemon: {}", e);
                    eprintln!("Is the daemon running? Try: diachron daemon start");
                    std::process::exit(1);
                }
            }
        }

        Commands::PrComment { pr, evidence } => {
            println!("Posting PR narrative comment...\n");

            // Read evidence pack
            let evidence_content = std::fs::read_to_string(&evidence)
                .context("Failed to read evidence file")?;

            let pack: serde_json::Value = serde_json::from_str(&evidence_content)
                .context("Failed to parse evidence JSON")?;

            // Build markdown narrative
            let mut md = String::new();

            // Header
            md.push_str(&format!(
                "## PR #{}: AI Provenance Evidence\n\n",
                pack["pr_id"].as_u64().unwrap_or(pr)
            ));

            // Intent section (if available)
            if let Some(intent) = pack["intent"].as_str() {
                if !intent.is_empty() {
                    md.push_str("### Intent\n");
                    md.push_str(&format!("> {}\n\n", intent));
                }
            }

            // Summary section
            md.push_str("### What Changed\n");
            md.push_str(&format!(
                "- **Files modified**: {}\n",
                pack["summary"]["files_changed"].as_u64().unwrap_or(0)
            ));
            md.push_str(&format!(
                "- **Lines**: +{} / -{}\n",
                pack["summary"]["lines_added"].as_u64().unwrap_or(0),
                pack["summary"]["lines_removed"].as_u64().unwrap_or(0)
            ));
            md.push_str(&format!(
                "- **Tool operations**: {}\n",
                pack["summary"]["tool_operations"].as_u64().unwrap_or(0)
            ));
            md.push_str(&format!(
                "- **Sessions**: {}\n\n",
                pack["summary"]["sessions"].as_u64().unwrap_or(0)
            ));

            // Evidence trail section
            md.push_str("### Evidence Trail\n");
            let coverage = pack["coverage_pct"].as_f64().unwrap_or(0.0);
            let unmatched = pack["unmatched_count"].as_u64().unwrap_or(0);
            md.push_str(&format!("- **Coverage**: {:.1}% of events matched to commits", coverage));
            if unmatched > 0 {
                md.push_str(&format!(" ({} unmatched)", unmatched));
            }
            md.push_str("\n");

            // List commits with their events
            if let Some(commits) = pack["commits"].as_array() {
                for commit in commits {
                    let sha = commit["sha"].as_str().unwrap_or("");
                    let sha_short = &sha[..7.min(sha.len())];
                    let confidence = commit["confidence"].as_str().unwrap_or("LOW");

                    md.push_str(&format!("\n**Commit `{}`**", sha_short));
                    if let Some(msg) = commit["message"].as_str() {
                        let first_line = msg.lines().next().unwrap_or(msg);
                        md.push_str(&format!(": {}", first_line));
                    }
                    md.push_str(&format!(" ({})\n", confidence));

                    if let Some(events) = commit["events"].as_array() {
                        for event in events.iter().take(5) {
                            let tool = event["tool_name"].as_str().unwrap_or("-");
                            let file = event["file_path"].as_str().unwrap_or("-");
                            let op = event["operation"].as_str().unwrap_or("-");
                            md.push_str(&format!("  - `{}` {} → {}\n", tool, op, file));
                        }
                        if events.len() > 5 {
                            md.push_str(&format!("  - *...and {} more*\n", events.len() - 5));
                        }
                    }
                }
            }
            md.push_str("\n");

            // Verification section
            md.push_str("### Verification\n");
            md.push_str(&format!(
                "- [{}] Hash chain integrity\n",
                if pack["verification"]["chain_verified"].as_bool().unwrap_or(false) { "x" } else { " " }
            ));
            md.push_str(&format!(
                "- [{}] Tests executed after changes\n",
                if pack["verification"]["tests_executed"].as_bool().unwrap_or(false) { "x" } else { " " }
            ));
            md.push_str(&format!(
                "- [{}] Build succeeded\n",
                if pack["verification"]["build_succeeded"].as_bool().unwrap_or(false) { "x" } else { " " }
            ));
            md.push_str(&format!(
                "- [{}] Human review\n\n",
                if pack["verification"]["human_reviewed"].as_bool().unwrap_or(false) { "x" } else { " " }
            ));

            // Footer
            md.push_str(&format!(
                "---\n*Generated by [Diachron](https://github.com/wolfiesch/diachron) v{} at {}*\n",
                pack["diachron_version"].as_str().unwrap_or(env!("CARGO_PKG_VERSION")),
                pack["generated_at"].as_str().unwrap_or("unknown")
            ));

            // Post via gh CLI
            let status = std::process::Command::new("gh")
                .args(["pr", "comment", &pr.to_string(), "-b", &md])
                .status()
                .context("Failed to run gh CLI")?;

            if status.success() {
                println!("✅ PR comment posted successfully");
                println!("\nPosted content:\n{}", md);
            } else {
                eprintln!("Failed to post PR comment (gh exit code: {:?})", status.code());
                std::process::exit(1);
            }
        }

        Commands::Blame { target, format, mode } => {
            // Parse file:line
            let parts: Vec<&str> = target.rsplitn(2, ':').collect();
            if parts.len() != 2 {
                eprintln!("Invalid target format. Use: file:line (e.g., src/auth.rs:42)");
                std::process::exit(1);
            }

            let line: u32 = parts[0].parse().context("Invalid line number")?;
            let file = parts[1];

            // Read file content to get the line and context
            let file_path = std::path::Path::new(file);
            let (content, context) = if file_path.exists() {
                let file_content = std::fs::read_to_string(file_path)
                    .unwrap_or_default();
                let lines: Vec<&str> = file_content.lines().collect();

                // Get the target line (1-indexed)
                let line_idx = (line as usize).saturating_sub(1);
                let target_line = lines.get(line_idx).unwrap_or(&"").to_string();

                // Get context (±5 lines)
                let start = line_idx.saturating_sub(5);
                let end = (line_idx + 6).min(lines.len());
                let context_lines: String = lines[start..end].join("\n");

                (target_line, context_lines)
            } else {
                // File doesn't exist locally, use empty placeholders
                (String::new(), String::new())
            };

            // Use fingerprint-based blame via daemon
            let msg = IpcMessage::BlameByFingerprint {
                file_path: file.to_string(),
                line_number: line,
                content,
                context,
                mode: mode.clone(),
            };

            match send_message(&msg) {
                Ok(IpcResponse::BlameResult(blame_match)) => {
                    let event = &blame_match.event;

                    if format == "json" {
                        let result = serde_json::json!({
                            "file": file,
                            "line": line,
                            "event_id": event.id,
                            "timestamp": event.timestamp,
                            "tool_name": event.tool_name,
                            "operation": event.operation,
                            "session_id": event.session_id,
                            "diff_summary": event.diff_summary,
                            "confidence": blame_match.confidence.to_uppercase(),
                            "match_type": blame_match.match_type,
                            "similarity": blame_match.similarity,
                            "intent": blame_match.intent
                        });
                        println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    } else {
                        println!("Diachron Blame");
                        println!("==============\n");
                        println!("File: {}:{}", file, line);

                        let confidence_emoji = match blame_match.confidence.as_str() {
                            "high" => "🎯",
                            "medium" => "📊",
                            "low" => "⚠️",
                            _ => "❓",
                        };

                        println!(
                            "\n{} Confidence: {} ({})",
                            confidence_emoji,
                            blame_match.confidence.to_uppercase(),
                            blame_match.match_type
                        );
                        println!(
                            "📍 Source: Claude Code (Session {})",
                            event.session_id.as_deref().unwrap_or("unknown")
                        );
                        println!(
                            "⏰ When: {}",
                            event.timestamp_display.as_deref().unwrap_or(&event.timestamp)
                        );
                        println!(
                            "🔧 Tool: {} ({})",
                            event.tool_name,
                            event.operation.as_deref().unwrap_or("-")
                        );
                        if let Some(ref diff) = event.diff_summary {
                            println!("📝 Changes: {}", diff);
                        }
                        if let Some(ref intent) = blame_match.intent {
                            println!("💬 Intent: \"{}\"", intent);
                        }
                    }
                }
                Ok(IpcResponse::BlameNotFound { reason }) => {
                    if format == "json" {
                        let result = serde_json::json!({
                            "file": file,
                            "line": line,
                            "error": "not_found",
                            "reason": reason
                        });
                        println!("{}", serde_json::to_string_pretty(&result).unwrap());
                    } else {
                        println!("Diachron Blame");
                        println!("==============\n");
                        println!("File: {}:{}", file, line);
                        println!("\n⚠️ {}", reason);
                    }
                }
                Ok(IpcResponse::Error(e)) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
                Ok(_) => {
                    eprintln!("Unexpected response from daemon");
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!("Failed to communicate with daemon: {}", e);
                    eprintln!("Is the daemon running? Try: diachron daemon start");
                    std::process::exit(1);
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

/// Parse a time filter string into (start_time, end_time) ISO timestamps.
///
/// Supports formats:
/// - "1h", "2d", "7d" - relative from now
/// - "2024-01-01" - absolute date (assumes midnight)
/// - ISO timestamp
fn parse_time_range(since: &str) -> (String, String) {
    use chrono::{Duration, NaiveDate, Utc};

    let now = Utc::now();
    let end_time = now.format("%Y-%m-%dT%H:%M:%S").to_string();

    // Try relative time (e.g., "1h", "7d")
    if let Some(stripped) = since.strip_suffix('h') {
        if let Ok(hours) = stripped.parse::<i64>() {
            let start = now - Duration::hours(hours);
            return (start.format("%Y-%m-%dT%H:%M:%S").to_string(), end_time);
        }
    }

    if let Some(stripped) = since.strip_suffix('d') {
        if let Ok(days) = stripped.parse::<i64>() {
            let start = now - Duration::days(days);
            return (start.format("%Y-%m-%dT%H:%M:%S").to_string(), end_time);
        }
    }

    // Try date (e.g., "2024-01-01")
    if let Ok(date) = NaiveDate::parse_from_str(since, "%Y-%m-%d") {
        let start = date.and_hms_opt(0, 0, 0).unwrap();
        return (start.format("%Y-%m-%dT%H:%M:%S").to_string(), end_time);
    }

    // Try full ISO timestamp
    if since.contains('T') {
        return (since.to_string(), end_time);
    }

    // Default: last 7 days
    let start = now - Duration::days(7);
    (start.format("%Y-%m-%dT%H:%M:%S").to_string(), end_time)
}

/// Format search results for context injection at session start.
///
/// Produces token-conscious output:
/// - Max 1500 tokens (~6000 chars)
/// - Summarized snippets (first 200 chars each)
/// - Formatted as markdown for Claude to parse
///
/// T4 Quality Fixes (01/10/2026):
/// - T4-1: Strip HTML tags (<b>, </b>)
/// - T4-2: Clean line prefixes (N→)
/// - T4-3: Filter tool wrappers ([Result:, Shell cwd)
/// - T4-4: Deduplicate results
/// - T4-5: Quality threshold (score > 5.0)
fn format_context_output(results: &[diachron_core::SearchResult]) {
    use std::collections::HashSet;

    const MAX_CHARS: usize = 6000; // ~1500 tokens
    const SNIPPET_MAX: usize = 200;
    const MIN_SCORE: f32 = 5.0; // T4-5: Quality threshold

    let mut output = String::new();
    let mut char_count = 0;
    let mut included_count = 0;
    let mut seen_snippets: HashSet<String> = HashSet::new(); // T4-4: Deduplication

    // Header
    let header = "## Prior Context from This Project\n\n";
    output.push_str(header);
    char_count += header.len();

    for result in results {
        // T4-5: Skip low-quality results
        if result.score < MIN_SCORE {
            continue;
        }

        // T4-3: Skip results that are primarily tool output noise
        if is_tool_noise(&result.snippet) {
            continue;
        }

        // Clean the snippet (T4-1, T4-2, T4-3)
        let cleaned = clean_snippet(&result.snippet);

        // Skip if cleaned snippet is too short (likely all noise)
        if cleaned.len() < 20 {
            continue;
        }

        // T4-4: Skip duplicates (check first 50 chars for similarity)
        let dedup_key = safe_truncate(&cleaned, 50).to_lowercase();
        if seen_snippets.contains(&dedup_key) {
            continue;
        }
        seen_snippets.insert(dedup_key);

        // Format each result as a compact entry
        let date = if result.timestamp.len() >= 10 {
            &result.timestamp[..10] // YYYY-MM-DD
        } else {
            &result.timestamp
        };

        let source_str = match result.source {
            diachron_core::SearchSource::Event => "Code change",
            diachron_core::SearchSource::Exchange => "Discussion",
        };

        // Truncate snippet safely (UTF-8 aware)
        let snippet_final = safe_truncate(&cleaned, SNIPPET_MAX);

        let entry = format!(
            "### {} - {}\n{}\n\n",
            date, source_str, snippet_final
        );

        // Check token budget
        if char_count + entry.len() > MAX_CHARS {
            break;
        }

        output.push_str(&entry);
        char_count += entry.len();
        included_count += 1;
    }

    // Only output if we have meaningful content
    if included_count == 0 {
        return; // Silent - no quality context found
    }

    // Footer with stats (helps user understand what was injected)
    let word_count = output.split_whitespace().count();
    let approx_tokens = word_count * 4 / 3; // Rough approximation
    let footer = format!(
        "_({} items, ~{} tokens)_\n",
        included_count, approx_tokens
    );

    if char_count + footer.len() <= MAX_CHARS + 100 {
        output.push_str(&footer);
    }

    print!("{}", output);
}

/// Clean a snippet by removing artifacts and noise.
/// T4-1: Strip HTML tags
/// T4-2: Clean line prefixes
/// T4-3: Filter tool wrappers
fn clean_snippet(s: &str) -> String {
    let mut result = s.to_string();

    // T4-1: Strip HTML tags from FTS highlighting
    result = result.replace("<b>", "");
    result = result.replace("</b>", "");
    result = result.replace("<em>", "");
    result = result.replace("</em>", "");

    // T4-2: Remove line number prefixes (e.g., "1→", "42→")
    // Pattern: digits followed by → at start of line or after whitespace
    let re_line_nums = regex::Regex::new(r"(\s|^)\d+→").unwrap_or_else(|_| {
        // Fallback: simple replacement
        regex::Regex::new(r"\d+→").unwrap()
    });
    result = re_line_nums.replace_all(&result, " ").to_string();

    // T4-3: Remove tool output wrappers
    // Remove [Result: prefix
    if result.starts_with("[Result:") {
        if let Some(pos) = result.find(']') {
            result = result[pos + 1..].to_string();
        }
    }
    result = result.replace("[Result:", "");
    result = result.replace("...]", "");

    // T4-3: Remove shell noise
    let shell_patterns = [
        "Shell cwd was reset to",
        "Shell cwd: ",
        "<system-reminder>",
        "</system-reminder>",
    ];
    for pattern in &shell_patterns {
        if let Some(pos) = result.find(pattern) {
            // Remove from pattern to end of line
            if let Some(newline) = result[pos..].find('\n') {
                result = format!("{}{}", &result[..pos], &result[pos + newline..]);
            } else {
                result = result[..pos].to_string();
            }
        }
    }

    // Normalize whitespace
    result = result.replace('\n', " ");
    result = result.split_whitespace().collect::<Vec<_>>().join(" ");

    result.trim().to_string()
}

/// Check if a snippet is primarily tool output noise.
fn is_tool_noise(s: &str) -> bool {
    // Check for tool result wrappers at the start
    if s.starts_with("[Result:") || s.starts_with("[Tool:") {
        return true;
    }

    // Check for internal/system messages
    let noise_starts = [
        "Warmup",
        "Stop hook feedback",
        "Analyze this conversation",
        "You MUST call",
        "This session is being continued",
        "<function_calls>",
        "```json",
        "I'm Claude Code",
        "I'm ready to help",
    ];
    for pattern in &noise_starts {
        if s.starts_with(pattern) {
            return true;
        }
    }

    // Check for error indicators
    let noise_contains = [
        "Shell cwd was reset",
        "401 {\"type\":\"error\"",
        "authentication_error",
        "Failed to find element",
        "Permission denied",
        "No such file",
        "command not found",
        "<system-reminder>",
        "hookSpecificOutput",
    ];
    for indicator in &noise_contains {
        if s.contains(indicator) {
            return true;
        }
    }

    // Skip if mostly line numbers (file content dump)
    let arrow_count = s.matches('→').count();
    let char_count = s.len();
    if arrow_count > 3 && (arrow_count * 15) > char_count {
        return true;
    }

    false
}

/// Safely truncate a string at a UTF-8 character boundary.
fn safe_truncate(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        return s;
    }

    // Find the last valid char boundary at or before max_len
    let mut end = max_len;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }

    &s[..end]
}
