//! Liveness timeout for downstream language servers.

use std::time::Duration;

/// Liveness timeout for hung server detection (ADR-0018 Tier 2: 30-120s).
///
/// This timeout detects zombie servers (process alive but unresponsive).
/// The timer is active only when the connection is Ready with pending > 0.
///
/// # Timer Behavior (ADR-0014)
///
/// - Starts when pending count transitions 0 -> 1 in Ready state
/// - Resets on any stdout activity (response or notification)
/// - Stops when pending count returns to 0
/// - Fires Ready -> Failed transition if no activity while pending > 0
///
/// # Valid Range
///
/// The ADR recommends 30-120 seconds for Tier 2 timeouts, but this type
/// currently does not enforce this range at construction time.
///
/// # Future Work
///
/// TODO: When adding user-configurable liveness timeout, introduce both
/// validation and configuration together:
/// - Add a `new(duration: Duration) -> Result<Self, Error>` constructor
///   that validates the duration is within the 30-120s range
/// - Add configuration support in workspace settings (e.g., `bridge.livenessTimeoutSecs`)
/// - Consider per-language-server timeout configuration
/// - Keep validation and configuration changes in the same PR to ensure
///   users can only configure valid values
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LivenessTimeout(Duration);

impl LivenessTimeout {
    /// Default timeout: 60 seconds (middle of ADR-0018 recommended 30-120s range)
    const DEFAULT_SECS: u64 = 60;

    /// Get the inner Duration value.
    pub(crate) fn as_duration(&self) -> Duration {
        self.0
    }
}

impl Default for LivenessTimeout {
    fn default() -> Self {
        Self(Duration::from_secs(Self::DEFAULT_SECS))
    }
}
