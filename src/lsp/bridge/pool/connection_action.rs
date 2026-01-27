//! Connection action decision logic for the language server pool.
//!
//! This module extracts pure decision logic from `get_or_create_connection_with_timeout`,
//! enabling unit testing without spawning real processes.

use super::ConnectionState;

/// Maximum consecutive panics before giving up on a server.
///
/// After this many consecutive handshake task panics for a server,
/// we stop retrying and return an error to prevent infinite retry loops.
pub(super) const MAX_CONSECUTIVE_PANICS: u32 = 3;

/// Action to take when requesting a connection for a language.
///
/// This enum represents the decision made based on existing connection state,
/// allowing the logic to be tested in isolation from I/O operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ConnectionAction {
    /// Return the existing connection (state is Ready)
    ReturnExisting,
    /// Spawn a new connection (no connection exists, or previous was Failed/Closed)
    SpawnNew,
    /// Fail fast with error message (state is Initializing or Closing)
    FailFast(&'static str),
}

/// Decide what action to take based on existing connection state and panic history.
///
/// This is a pure function that can be unit tested without spawning processes.
///
/// # State-based decisions per ADR-0015 Operation Gating:
/// - `None`: No connection exists → SpawnNew (unless too many panics)
/// - `Initializing`: Init in progress → FailFast (reject concurrent requests)
/// - `Ready`: Connection available → ReturnExisting
/// - `Failed`: Previous attempt failed → SpawnNew (unless too many panics)
/// - `Closing`: Shutdown in progress → FailFast (reject new requests)
/// - `Closed`: Connection terminated → SpawnNew (unless too many panics)
///
/// # Panic Protection
///
/// If `consecutive_panic_count >= MAX_CONSECUTIVE_PANICS`, returns FailFast
/// to prevent infinite retry loops when a handshake consistently panics.
pub(super) fn decide_connection_action(
    state: Option<ConnectionState>,
    consecutive_panic_count: u32,
) -> ConnectionAction {
    // Check for too many consecutive panics first
    if consecutive_panic_count >= MAX_CONSECUTIVE_PANICS {
        log::error!(
            target: "kakehashi::bridge::connection",
            "Server disabled after {} consecutive handshake panics (max: {})",
            consecutive_panic_count,
            MAX_CONSECUTIVE_PANICS
        );
        return ConnectionAction::FailFast(
            "bridge: server disabled after repeated handshake failures",
        );
    }

    match state {
        None => ConnectionAction::SpawnNew,
        Some(ConnectionState::Ready) => ConnectionAction::ReturnExisting,
        Some(ConnectionState::Initializing) => {
            ConnectionAction::FailFast("bridge: downstream server initializing")
        }
        Some(ConnectionState::Failed) => ConnectionAction::SpawnNew,
        Some(ConnectionState::Closing) => ConnectionAction::FailFast("bridge: connection closing"),
        Some(ConnectionState::Closed) => ConnectionAction::SpawnNew,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that no existing connection results in SpawnNew action.
    #[test]
    fn no_connection_spawns_new() {
        assert_eq!(
            decide_connection_action(None, 0),
            ConnectionAction::SpawnNew
        );
    }

    /// Test that Ready state returns existing connection.
    #[test]
    fn ready_state_returns_existing() {
        assert_eq!(
            decide_connection_action(Some(ConnectionState::Ready), 0),
            ConnectionAction::ReturnExisting
        );
    }

    /// Test that Initializing state fails fast.
    ///
    /// ADR-0015: Concurrent requests during initialization should fail immediately
    /// rather than block or queue.
    #[test]
    fn initializing_state_fails_fast() {
        let action = decide_connection_action(Some(ConnectionState::Initializing), 0);
        assert_eq!(
            action,
            ConnectionAction::FailFast("bridge: downstream server initializing")
        );
    }

    /// Test that Failed state triggers respawn.
    ///
    /// ADR-0015: Failed connections are removed from pool, next request spawns fresh.
    #[test]
    fn failed_state_spawns_new() {
        assert_eq!(
            decide_connection_action(Some(ConnectionState::Failed), 0),
            ConnectionAction::SpawnNew
        );
    }

    /// Test that Closing state fails fast.
    ///
    /// ADR-0017: Connections in graceful shutdown reject new requests.
    #[test]
    fn closing_state_fails_fast() {
        let action = decide_connection_action(Some(ConnectionState::Closing), 0);
        assert_eq!(
            action,
            ConnectionAction::FailFast("bridge: connection closing")
        );
    }

    /// Test that Closed state triggers respawn.
    ///
    /// Terminated connections should be replaced on next request.
    #[test]
    fn closed_state_spawns_new() {
        assert_eq!(
            decide_connection_action(Some(ConnectionState::Closed), 0),
            ConnectionAction::SpawnNew
        );
    }

    /// Test that too many consecutive panics stops retry attempts.
    ///
    /// Prevents infinite retry loops when handshake consistently panics.
    #[test]
    fn too_many_panics_fails_fast() {
        // At the threshold, should fail fast
        let action = decide_connection_action(None, MAX_CONSECUTIVE_PANICS);
        assert_eq!(
            action,
            ConnectionAction::FailFast("bridge: server disabled after repeated handshake failures")
        );

        // Above the threshold, should also fail fast
        let action =
            decide_connection_action(Some(ConnectionState::Failed), MAX_CONSECUTIVE_PANICS + 1);
        assert_eq!(
            action,
            ConnectionAction::FailFast("bridge: server disabled after repeated handshake failures")
        );
    }

    /// Test that below panic threshold allows normal retry.
    #[test]
    fn below_panic_threshold_allows_retry() {
        // Just below the threshold, should still spawn new
        let action =
            decide_connection_action(Some(ConnectionState::Failed), MAX_CONSECUTIVE_PANICS - 1);
        assert_eq!(action, ConnectionAction::SpawnNew);
    }
}
