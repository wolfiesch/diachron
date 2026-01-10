//! IPC client for communicating with the Diachron daemon
//!
//! This module provides a synchronous client for sending messages to the daemon
//! over a Unix socket. It's designed to be used by the hook (which needs sync I/O)
//! and can also be used by CLI commands.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

use crate::{CaptureEvent, IpcMessage, IpcResponse};

/// Default socket path
pub fn socket_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".diachron")
        .join("diachron.sock")
}

/// Error type for IPC operations
#[derive(Debug)]
pub enum IpcError {
    /// Daemon is not running (socket doesn't exist or connection refused)
    DaemonNotRunning,
    /// Socket connection failed
    ConnectionFailed(std::io::Error),
    /// Failed to send message
    SendFailed(std::io::Error),
    /// Failed to receive response
    ReceiveFailed(std::io::Error),
    /// Invalid response format
    InvalidResponse(String),
    /// Daemon returned an error
    DaemonError(String),
}

impl std::fmt::Display for IpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IpcError::DaemonNotRunning => write!(f, "Daemon not running"),
            IpcError::ConnectionFailed(e) => write!(f, "Connection failed: {}", e),
            IpcError::SendFailed(e) => write!(f, "Send failed: {}", e),
            IpcError::ReceiveFailed(e) => write!(f, "Receive failed: {}", e),
            IpcError::InvalidResponse(msg) => write!(f, "Invalid response: {}", msg),
            IpcError::DaemonError(msg) => write!(f, "Daemon error: {}", msg),
        }
    }
}

impl std::error::Error for IpcError {}

/// IPC client for communicating with the daemon
pub struct IpcClient {
    socket_path: PathBuf,
    timeout: Duration,
}

impl Default for IpcClient {
    fn default() -> Self {
        Self::new()
    }
}

impl IpcClient {
    /// Create a new IPC client with default settings
    pub fn new() -> Self {
        Self {
            socket_path: socket_path(),
            timeout: Duration::from_secs(5),
        }
    }

    /// Create a client with a custom socket path
    pub fn with_socket_path(socket_path: PathBuf) -> Self {
        Self {
            socket_path,
            timeout: Duration::from_secs(5),
        }
    }

    /// Set the connection timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Check if the daemon appears to be running (socket exists)
    pub fn daemon_available(&self) -> bool {
        self.socket_path.exists()
    }

    /// Send a message to the daemon and wait for a response
    pub fn send(&self, message: &IpcMessage) -> Result<IpcResponse, IpcError> {
        // Check if socket exists first (fast path)
        if !self.socket_path.exists() {
            return Err(IpcError::DaemonNotRunning);
        }

        // Connect to daemon
        let mut stream = UnixStream::connect(&self.socket_path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::ConnectionRefused
                || e.kind() == std::io::ErrorKind::NotFound
            {
                IpcError::DaemonNotRunning
            } else {
                IpcError::ConnectionFailed(e)
            }
        })?;

        // Set timeouts
        stream
            .set_read_timeout(Some(self.timeout))
            .ok();
        stream
            .set_write_timeout(Some(self.timeout))
            .ok();

        // Send message as JSON line
        let json = serde_json::to_string(message)
            .map_err(|e| IpcError::SendFailed(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))?;

        writeln!(stream, "{}", json).map_err(IpcError::SendFailed)?;
        stream.flush().map_err(IpcError::SendFailed)?;

        // Read response
        let mut reader = BufReader::new(stream);
        let mut response_line = String::new();
        reader
            .read_line(&mut response_line)
            .map_err(IpcError::ReceiveFailed)?;

        // Parse response
        let response: IpcResponse = serde_json::from_str(&response_line)
            .map_err(|e| IpcError::InvalidResponse(e.to_string()))?;

        // Check for daemon errors
        if let IpcResponse::Error(msg) = &response {
            return Err(IpcError::DaemonError(msg.clone()));
        }

        Ok(response)
    }

    /// Convenience method: Send a capture event to the daemon
    pub fn capture(&self, event: CaptureEvent) -> Result<(), IpcError> {
        let response = self.send(&IpcMessage::Capture(event))?;
        match response {
            IpcResponse::Ok => Ok(()),
            IpcResponse::Error(msg) => Err(IpcError::DaemonError(msg)),
            _ => Err(IpcError::InvalidResponse("Unexpected response type".into())),
        }
    }

    /// Convenience method: Ping the daemon
    pub fn ping(&self) -> Result<(u64, u64), IpcError> {
        let response = self.send(&IpcMessage::Ping)?;
        match response {
            IpcResponse::Pong { uptime_secs, events_count } => Ok((uptime_secs, events_count)),
            IpcResponse::Error(msg) => Err(IpcError::DaemonError(msg)),
            _ => Err(IpcError::InvalidResponse("Unexpected response type".into())),
        }
    }
}

/// Simple function to send a capture event to the daemon
///
/// Returns Ok(()) if the daemon handled the event, or an error if:
/// - The daemon is not running
/// - The connection failed
/// - The daemon returned an error
///
/// This is designed for use in the hook where we want to fall back
/// to local database writes if the daemon is unavailable.
pub fn send_to_daemon(event: CaptureEvent) -> Result<(), IpcError> {
    IpcClient::new().capture(event)
}

/// Check if the daemon is running
pub fn is_daemon_running() -> bool {
    IpcClient::new().daemon_available()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_socket_path() {
        let path = socket_path();
        assert!(path.ends_with("diachron.sock"));
        assert!(path.to_str().unwrap().contains(".diachron"));
    }

    #[test]
    fn test_daemon_not_running() {
        let client = IpcClient::with_socket_path(PathBuf::from("/nonexistent/path.sock"));
        assert!(!client.daemon_available());

        let result = client.ping();
        assert!(matches!(result, Err(IpcError::DaemonNotRunning)));
    }
}
