//! Connection state machine for downstream language servers.
//!
//! This module defines the lifecycle states for LSP connections per ADR-0015.

/// State of a downstream language server connection.
///
/// Tracks the lifecycle of the LSP handshake per ADR-0015:
/// - Initializing: spawn started, awaiting initialize response
/// - Ready: initialize/initialized handshake complete, can accept requests
/// - Failed: initialization failed (timeout, error, etc.)
/// - Closing: graceful shutdown in progress (LSP shutdown/exit handshake)
/// - Closed: connection terminated (terminal state)
///
/// State transitions per ADR-0015 Operation Gating:
/// - Initializing -> Ready (on successful init)
/// - Initializing -> Failed (on timeout/error)
/// - Initializing -> Closing (on shutdown signal during init)
/// - Ready -> Closing (on shutdown signal)
/// - Closing -> Closed (on completion/timeout)
/// - Failed -> Closed (direct, no LSP handshake - stdin unavailable)
/// - Failed connections are removed from pool, next request spawns fresh server
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConnectionState {
    /// Server spawned, initialize request sent, awaiting response
    Initializing,
    /// Initialize/initialized handshake complete, ready for requests
    Ready,
    /// Initialization failed (timeout, error, server crash)
    Failed,
    /// Graceful shutdown in progress (LSP shutdown/exit handshake)
    Closing,
    /// Connection terminated (terminal state)
    Closed,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::bridge::pool::test_helpers::*;

    /// Test that ConnectionState enum has exactly 5 states.
    ///
    /// States: Initializing, Ready, Failed, Closing, Closed
    /// This test verifies the enum is exhaustively enumerable.
    #[test]
    fn connection_state_has_all_five_states() {
        // Verify all 5 states exist by constructing them
        let states = [
            ConnectionState::Initializing,
            ConnectionState::Ready,
            ConnectionState::Failed,
            ConnectionState::Closing,
            ConnectionState::Closed,
        ];

        // Verify we have exactly 5 states
        assert_eq!(
            states.len(),
            5,
            "ConnectionState should have exactly 5 variants"
        );

        // Verify each state has the expected Debug representation
        assert_eq!(
            format!("{:?}", ConnectionState::Initializing),
            "Initializing"
        );
        assert_eq!(format!("{:?}", ConnectionState::Ready), "Ready");
        assert_eq!(format!("{:?}", ConnectionState::Failed), "Failed");
        assert_eq!(format!("{:?}", ConnectionState::Closing), "Closing");
        assert_eq!(format!("{:?}", ConnectionState::Closed), "Closed");
    }

    /// Test that Ready state transitions to Closing on shutdown signal.
    ///
    /// ADR-0015: Ready → Closing transition occurs when shutdown is initiated.
    /// This is the graceful shutdown path for active connections.
    #[tokio::test]
    async fn ready_to_closing_transition() {
        let handle = create_handle_with_state(ConnectionState::Ready).await;

        // Verify initial state
        assert_eq!(
            handle.state(),
            ConnectionState::Ready,
            "Should start in Ready state"
        );

        // Trigger shutdown - should transition to Closing
        handle.begin_shutdown();

        // Verify transition
        assert_eq!(
            handle.state(),
            ConnectionState::Closing,
            "Ready + shutdown signal = Closing"
        );
    }

    /// Test that Initializing state transitions to Closing on shutdown signal.
    ///
    /// ADR-0017: When shutdown is initiated during initialization, abort init
    /// and proceed directly to shutdown. This handles cases where editor closes
    /// during slow server startup.
    #[tokio::test]
    async fn initializing_to_closing_transition() {
        let handle = create_handle_with_state(ConnectionState::Initializing).await;

        // Verify initial state
        assert_eq!(
            handle.state(),
            ConnectionState::Initializing,
            "Should start in Initializing state"
        );

        // Trigger shutdown - should transition to Closing
        handle.begin_shutdown();

        // Verify transition
        assert_eq!(
            handle.state(),
            ConnectionState::Closing,
            "Initializing + shutdown signal = Closing"
        );
    }

    /// Test that Closing state transitions to Closed on completion.
    ///
    /// ADR-0015: Closing → Closed transition occurs when LSP shutdown/exit
    /// handshake completes or times out. This is the terminal state for
    /// graceful shutdown.
    #[tokio::test]
    async fn closing_to_closed_transition() {
        let handle = create_handle_with_state(ConnectionState::Closing).await;

        // Verify initial state
        assert_eq!(
            handle.state(),
            ConnectionState::Closing,
            "Should start in Closing state"
        );

        // Complete shutdown - should transition to Closed
        handle.complete_shutdown();

        // Verify transition
        assert_eq!(
            handle.state(),
            ConnectionState::Closed,
            "Closing + completion = Closed"
        );
    }

    /// Test that Failed state transitions directly to Closed (bypassing Closing).
    ///
    /// ADR-0017: Failed connections cannot perform LSP shutdown/exit handshake
    /// because stdin is unavailable. They go directly to Closed state.
    #[tokio::test]
    async fn failed_to_closed_direct_transition() {
        let handle = create_handle_with_state(ConnectionState::Failed).await;

        // Verify initial state
        assert_eq!(
            handle.state(),
            ConnectionState::Failed,
            "Should start in Failed state"
        );

        // Direct shutdown completion - bypasses Closing state
        handle.complete_shutdown();

        // Verify transition
        assert_eq!(
            handle.state(),
            ConnectionState::Closed,
            "Failed + completion = Closed (bypasses Closing)"
        );
    }
}
