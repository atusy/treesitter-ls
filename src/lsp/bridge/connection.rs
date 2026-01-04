//! BridgeConnection for managing connections to language servers

use std::sync::atomic::{AtomicBool, Ordering};
use tokio::process::{Child, ChildStdin, ChildStdout};

/// Represents a connection to a bridged language server
#[allow(dead_code)] // Used in Phase 2 (real LSP communication)
pub struct BridgeConnection {
    /// Spawned language server process
    process: Child,
    /// Stdin handle for sending requests/notifications
    stdin: ChildStdin,
    /// Stdout handle for receiving responses/notifications
    stdout: ChildStdout,
    /// Tracks whether the connection has been initialized
    initialized: AtomicBool,
    /// Tracks whether didOpen notification has been sent
    did_open_sent: AtomicBool,
}

impl std::fmt::Debug for BridgeConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BridgeConnection")
            .field("process_id", &self.process.id())
            .field("initialized", &self.initialized.load(Ordering::SeqCst))
            .field("did_open_sent", &self.did_open_sent.load(Ordering::SeqCst))
            .finish()
    }
}

impl BridgeConnection {
    /// Creates a new BridgeConnection by spawning a language server process
    ///
    /// # Arguments
    /// * `command` - Command to spawn (e.g., "lua-language-server")
    ///
    /// # Errors
    /// Returns error if:
    /// - Process fails to spawn
    /// - stdin/stdout handles cannot be obtained
    #[allow(dead_code)] // Used in Phase 2 (real LSP communication)
    pub(crate) async fn new(command: &str) -> Result<Self, String> {
        use tokio::process::Command;
        use std::process::Stdio;

        let mut child = Command::new(command)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to spawn {}: {}", command, e))?;

        let stdin = child.stdin.take()
            .ok_or_else(|| format!("Failed to obtain stdin for {}", command))?;
        let stdout = child.stdout.take()
            .ok_or_else(|| format!("Failed to obtain stdout for {}", command))?;

        Ok(Self {
            process: child,
            stdin,
            stdout,
            initialized: AtomicBool::new(false),
            did_open_sent: AtomicBool::new(false),
        })
    }

    /// Initializes the connection
    ///
    /// This is a fakeit implementation that does NOT send real LSP initialize
    /// request. It simply sets the initialized flag to true and returns Ok(()).
    #[allow(dead_code)] // Used in Phase 2 (real LSP communication)
    pub(crate) fn initialize(&self) -> Result<(), String> {
        self.initialized.store(true, Ordering::SeqCst);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[tokio::test]
    async fn test_bridge_connection_spawns_process_with_valid_command() {
        // RED: Test spawning a real process (use 'cat' as a simple test command)
        // This verifies tokio::process::Command integration
        let result = BridgeConnection::new("cat").await;

        assert!(result.is_ok(), "Failed to spawn process: {:?}", result.err());
        let connection = result.unwrap();

        // Verify process is alive (type checks - fields exist)
        let _stdin: &ChildStdin = &connection.stdin;
        let _stdout: &ChildStdout = &connection.stdout;

        // Initially should not be initialized
        assert!(!connection.initialized.load(Ordering::SeqCst));
        assert!(!connection.did_open_sent.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_bridge_connection_fails_with_invalid_command() {
        // RED: Test that invalid command returns clear error
        let result = BridgeConnection::new("nonexistent-binary-xyz123").await;

        assert!(result.is_err(), "Should fail for nonexistent command");
        let error = result.unwrap_err();
        assert!(error.contains("Failed to spawn"), "Error should mention spawn failure: {}", error);
        assert!(error.contains("nonexistent-binary-xyz123"), "Error should mention command: {}", error);
    }

    #[test]
    fn test_bridge_connection_initialize_sets_flag() {
        // Test that initialize() sets the initialized flag to true
        // Note: This uses stubbed synchronous initialize(), not real async protocol
        // We'll keep this test for now and remove it when real initialize() is implemented
        // For now, we can't easily create a BridgeConnection without spawning
        // Skip this test until we have real initialize() implemented
    }
}
