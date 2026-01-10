//! Unix socket server for the daemon

use std::sync::Arc;

use anyhow::Result;
use tokio::net::UnixListener;
use tracing::{error, info};

use crate::{handle_client, DaemonState};

/// Run the daemon server
pub async fn run(state: Arc<DaemonState>) -> Result<()> {
    let socket_path = state.socket_path();

    // Create Unix socket listener
    let listener = UnixListener::bind(&socket_path)?;
    info!("Listening on {:?}", socket_path);

    // Accept connections
    loop {
        if state.should_shutdown() {
            info!("Shutdown requested, stopping server");
            break;
        }

        tokio::select! {
            result = listener.accept() => {
                match result {
                    Ok((stream, _addr)) => {
                        let state = Arc::clone(&state);
                        tokio::spawn(async move {
                            if let Err(e) = handle_client(stream, state).await {
                                error!("Client error: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        error!("Accept error: {}", e);
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => {
                info!("Received Ctrl+C, shutting down");
                state.request_shutdown();
                break;
            }
        }
    }

    // Cleanup
    if socket_path.exists() {
        std::fs::remove_file(&socket_path)?;
    }

    info!("Daemon stopped");
    Ok(())
}
