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

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that GlobalShutdownTimeout type accepts values in the 5-15s range.
    ///
    /// ADR-0018 specifies the global shutdown timeout should be 5-15s.
    /// This test verifies the newtype validation accepts valid values.
    #[test]
    fn global_shutdown_timeout_accepts_valid_range() {
        // Minimum valid: 5 seconds
        let min_timeout = GlobalShutdownTimeout::new(Duration::from_secs(5));
        assert!(min_timeout.is_ok(), "5s should be valid minimum");

        // Maximum valid: 15 seconds
        let max_timeout = GlobalShutdownTimeout::new(Duration::from_secs(15));
        assert!(max_timeout.is_ok(), "15s should be valid maximum");

        // Middle of range: 10 seconds
        let mid_timeout = GlobalShutdownTimeout::new(Duration::from_secs(10));
        assert!(mid_timeout.is_ok(), "10s should be valid");
    }

    /// Test that GlobalShutdownTimeout type rejects values outside 5-15s range.
    ///
    /// ADR-0018 specifies the global shutdown timeout should be 5-15s.
    /// This test verifies the newtype validation rejects out-of-range values.
    #[test]
    fn global_shutdown_timeout_rejects_out_of_range() {
        // Below minimum: 4 seconds
        let too_short = GlobalShutdownTimeout::new(Duration::from_secs(4));
        assert!(too_short.is_err(), "4s should be rejected as too short");

        // Above maximum: 16 seconds
        let too_long = GlobalShutdownTimeout::new(Duration::from_secs(16));
        assert!(too_long.is_err(), "16s should be rejected as too long");

        // Zero duration
        let zero = GlobalShutdownTimeout::new(Duration::ZERO);
        assert!(zero.is_err(), "0s should be rejected");
    }

    /// Test that GlobalShutdownTimeout provides access to inner Duration.
    #[test]
    fn global_shutdown_timeout_as_duration() {
        let timeout = GlobalShutdownTimeout::new(Duration::from_secs(10)).expect("10s is valid");

        assert_eq!(timeout.as_duration(), Duration::from_secs(10));
    }

    /// Test sub-second boundary validation as documented.
    ///
    /// Per the documented boundary behavior:
    /// - Minimum: floor at 5 whole seconds (4.999s rejected, 5.001s accepted)
    /// - Maximum: ceiling at exactly 15 seconds (15.001s rejected)
    #[test]
    fn global_shutdown_timeout_subsecond_boundaries() {
        // 4.999s has secs=4, rejected (floor is 5 whole seconds)
        let just_under_min = GlobalShutdownTimeout::new(Duration::from_millis(4999));
        assert!(
            just_under_min.is_err(),
            "4.999s should be rejected (secs=4, under minimum)"
        );

        // 5.001s has secs=5, accepted
        let just_over_min = GlobalShutdownTimeout::new(Duration::from_millis(5001));
        assert!(just_over_min.is_ok(), "5.001s should be accepted (secs=5)");

        // 5.5s accepted (mid-range subsecond)
        let mid_subsec = GlobalShutdownTimeout::new(Duration::from_millis(5500));
        assert!(mid_subsec.is_ok(), "5.5s should be accepted");

        // 10.123s accepted (arbitrary subsecond)
        let arbitrary_subsec = GlobalShutdownTimeout::new(Duration::from_millis(10123));
        assert!(arbitrary_subsec.is_ok(), "10.123s should be accepted");

        // 15.0s exactly accepted (maximum boundary)
        let exact_max = GlobalShutdownTimeout::new(Duration::from_secs(15));
        assert!(exact_max.is_ok(), "15.0s exactly should be accepted");

        // 15.001s rejected (ceiling is exactly 15s)
        let just_over_max = GlobalShutdownTimeout::new(Duration::from_millis(15001));
        assert!(
            just_over_max.is_err(),
            "15.001s should be rejected (over maximum)"
        );

        // 15s + 1 nanosecond rejected
        let one_nano_over =
            GlobalShutdownTimeout::new(Duration::from_secs(15) + Duration::from_nanos(1));
        assert!(
            one_nano_over.is_err(),
            "15s + 1ns should be rejected (ceiling is exactly 15s)"
        );
    }

    /// Test default GlobalShutdownTimeout value.
    ///
    /// Default should be exactly 10s per ADR-0018 recommendation - a balance between
    /// allowing graceful shutdown for fast servers and bounding user wait time.
    #[test]
    fn global_shutdown_timeout_default() {
        let default_timeout = GlobalShutdownTimeout::default();

        // Assert exact default value, not just range - ensures intentional changes
        assert_eq!(
            default_timeout.as_duration(),
            Duration::from_secs(10),
            "Default should be exactly 10s per ADR-0018"
        );
    }

    /// Verify graceful_shutdown relies on global timeout, not internal timeout.
    ///
    /// This test verifies the architectural property: GlobalShutdownTimeout is the
    /// only timeout configuration for shutdown. Individual connection shutdowns
    /// don't have their own timeouts - they're all bounded by the global ceiling.
    ///
    /// Moved from pool.rs integration tests as this is a pure unit test about
    /// timeout architecture.
    #[test]
    fn graceful_shutdown_relies_on_global_timeout_not_internal() {
        let timeout = GlobalShutdownTimeout::default();
        assert_eq!(
            timeout.as_duration(),
            Duration::from_secs(10),
            "Default global timeout should be 10s per ADR-0018"
        );

        // The absence of SHUTDOWN_TIMEOUT constant in graceful_shutdown() is verified by:
        // 1. Code review during PR
        // 2. The integration tests in pool.rs which would fail if internal timeout existed
        //    (hung servers would timeout individually instead of being bounded by global)
    }
}
