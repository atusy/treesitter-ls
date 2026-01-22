//! Liveness timeout for downstream language servers.

use std::time::Duration;

/// Liveness timeout for hung server detection (ADR-0018 Tier 2: 30-120s).
///
/// This timeout detects zombie servers (process alive but unresponsive).
/// The timer is active only when the connection is Ready with pending > 0.
/// # Timer Behavior (ADR-0014)
///
/// - Starts when pending count transitions 0 -> 1 in Ready state
/// - Resets on any stdout activity (response or notification)
/// - Stops when pending count returns to 0
/// - Fires Ready -> Failed transition if no activity while pending > 0
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LivenessTimeout(Duration);

impl LivenessTimeout {
    /// Default timeout: 60 seconds
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
