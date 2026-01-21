//! Global shutdown timeout for language server pool.
//!
//! This module provides a validated timeout type for graceful shutdown
//! per ADR-0018 Tier 3 (5-15s range).

use std::time::Duration;

/// Global shutdown timeout for all connections (ADR-0018 Tier 3: 5-15s).
///
/// This is the single ceiling for the entire shutdown sequence across all
/// connections. Per ADR-0017, all connections shut down in parallel under
/// this global timeout. When the timeout expires, remaining connections
/// receive force_kill_all() with SIGTERM->SIGKILL escalation.
///
/// # Valid Range
///
/// - Minimum: 5 seconds (allows graceful LSP handshake for fast servers)
/// - Maximum: 15 seconds (bounds user wait time during shutdown)
/// - Default: 10 seconds (balance between graceful exit and user experience)
///
/// # Usage
///
/// ```ignore
/// let timeout = GlobalShutdownTimeout::new(Duration::from_secs(10))?;
/// pool.shutdown_all_with_timeout(timeout).await;
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GlobalShutdownTimeout(Duration);

impl GlobalShutdownTimeout {
    /// Default timeout: 10 seconds
    const DEFAULT_SECS: u64 = 10;

    /// Minimum valid timeout: 5 seconds (used in validation)
    #[cfg(test)]
    const MIN_SECS: u64 = 5;

    /// Maximum valid timeout: 15 seconds (used in validation)
    #[cfg(test)]
    const MAX_SECS: u64 = 15;

    /// Create a new GlobalShutdownTimeout with validation.
    ///
    /// # Arguments
    /// * `duration` - The timeout duration (must be 5-15 seconds)
    ///
    /// # Returns
    /// - `Ok(GlobalShutdownTimeout)` if duration is within valid range
    /// - `Err(io::Error)` with InvalidInput kind if duration is out of range
    ///
    /// # Boundary Behavior
    ///
    /// Sub-second precision is supported within the valid range:
    /// - `5.0s` to `15.0s` inclusive are valid
    /// - `4.999s` is rejected (floor is 5 whole seconds)
    /// - `15.001s` is rejected (ceiling is exactly 15 seconds)
    /// - `5.5s`, `10.123s`, etc. are accepted
    ///
    /// This asymmetry is intentional: the minimum ensures adequate time for
    /// LSP handshake (5 whole seconds), while the maximum strictly bounds
    /// user wait time (not even 1ms over 15s).
    ///
    /// # Note
    /// Currently only used in tests. Production code uses `default()`.
    /// This method will be used when configurable timeout is exposed via config.
    #[cfg(test)]
    pub(crate) fn new(duration: Duration) -> std::io::Result<Self> {
        let secs = duration.as_secs();
        let has_subsec = duration.subsec_nanos() > 0;

        // Check minimum: must be at least 5 whole seconds
        // 4.999s has secs=4, so it's rejected. 5.001s has secs=5, so it's accepted.
        if secs < Self::MIN_SECS {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "Global shutdown timeout must be at least {}s, got {:?}",
                    Self::MIN_SECS,
                    duration
                ),
            ));
        }

        // Check maximum: must be at most exactly 15 seconds
        // 15.0s is accepted. 15.001s is rejected (has_subsec is true when secs=15).
        if secs > Self::MAX_SECS || (secs == Self::MAX_SECS && has_subsec) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "Global shutdown timeout must be at most {}s, got {:?}",
                    Self::MAX_SECS,
                    duration
                ),
            ));
        }

        Ok(Self(duration))
    }

    /// Get the inner Duration value.
    pub(crate) fn as_duration(&self) -> Duration {
        self.0
    }
}

impl Default for GlobalShutdownTimeout {
    fn default() -> Self {
        Self(Duration::from_secs(Self::DEFAULT_SECS))
    }
}
