//! BridgeConnection for managing connections to language servers

use std::sync::atomic::AtomicBool;

/// Represents a connection to a bridged language server
///
/// This is a fakeit implementation that stubs out process spawning
/// and LSP communication for API structure validation.
pub struct BridgeConnection {
    /// Tracks whether the connection has been initialized
    initialized: AtomicBool,
    /// Tracks whether didOpen notification has been sent
    did_open_sent: AtomicBool,
}

impl BridgeConnection {
    /// Creates a new BridgeConnection instance
    ///
    /// This is a fakeit implementation that does NOT spawn a real process.
    /// It returns immediately with an uninitialized connection stub.
    pub(crate) fn new() -> Self {
        Self {
            initialized: AtomicBool::new(false),
            did_open_sent: AtomicBool::new(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bridge_connection_new_creates_instance() {
        // Test that BridgeConnection::new() creates an instance
        // This should complete immediately without spawning a real process
        let connection = BridgeConnection::new();

        // Verify connection is created (type check)
        let _type_check: BridgeConnection = connection;
    }

    #[test]
    fn test_bridge_connection_new_does_not_hang() {
        // Test that creating a connection completes quickly
        // (no real process spawning should occur)
        use std::time::{Duration, Instant};

        let start = Instant::now();
        let _connection = BridgeConnection::new();
        let elapsed = start.elapsed();

        // Should complete in under 100ms (way faster than spawning a process)
        assert!(
            elapsed < Duration::from_millis(100),
            "BridgeConnection::new() took {:?}, expected < 100ms",
            elapsed
        );
    }
}
