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
