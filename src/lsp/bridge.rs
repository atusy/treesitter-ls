//! Async Bridge Connection for LSP language server integration
//!
//! This module implements the async bridge architecture (ADR-0014) for communicating
//! with downstream language servers via stdio.

use std::io;
use std::process::Stdio;
use tokio::process::{Child, Command};

/// Async connection to a downstream language server process.
///
/// Manages the lifecycle of a child process running a language server,
/// providing async I/O for LSP JSON-RPC communication over stdio.
///
/// # Architecture (ADR-0014)
///
/// - Uses `tokio::process::Command` for process spawning
/// - Dedicated async reader task with `select!` for cancellation
/// - Pending request tracking via `DashMap<RequestId, oneshot::Sender>`
pub(crate) struct AsyncBridgeConnection {
    child: Child,
}

impl AsyncBridgeConnection {
    /// Spawn a new language server process.
    ///
    /// # Arguments
    /// * `cmd` - Command and arguments to spawn (e.g., `["lua-language-server"]`)
    ///
    /// # Returns
    /// A new `AsyncBridgeConnection` with stdio pipes connected to the child process.
    pub(crate) async fn spawn(cmd: Vec<String>) -> io::Result<Self> {
        let (program, args) = cmd.split_first().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "command must not be empty")
        })?;

        let child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;

        Ok(Self { child })
    }

    /// Get the process ID of the child process.
    pub(crate) fn child_id(&self) -> Option<u32> {
        self.child.id()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_exports_async_bridge_connection_type() {
        // Verify the type exists and is accessible
        fn assert_type_exists<T>() {}
        assert_type_exists::<AsyncBridgeConnection>();
    }

    /// RED: Test that spawn() creates a child process with stdio pipes
    #[tokio::test]
    async fn spawn_creates_child_process_with_stdio() {
        // Use `cat` as a simple test process that echoes stdin to stdout
        let cmd = vec!["cat".to_string()];
        let conn = AsyncBridgeConnection::spawn(cmd).await.expect("spawn should succeed");

        // The connection should have a child process ID
        assert!(conn.child_id().is_some(), "child process should have an ID");
    }
}
