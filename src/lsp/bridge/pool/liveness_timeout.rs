//! Liveness timeout for downstream language servers.
//!
//! This module provides a validated timeout type for detecting hung servers
//! per ADR-0018 Tier 2 (30-120s range).

use std::time::Duration;

/// Liveness timeout for hung server detection (ADR-0018 Tier 2: 30-120s).
///
/// This timeout detects zombie servers (process alive but unresponsive).
/// The timer is active only when the connection is Ready with pending > 0.
///
/// # Valid Range
///
/// - Minimum: 30 seconds (allows for slow but responsive servers)
/// - Maximum: 120 seconds (bounds wait time for unresponsive servers)
/// - Default: 60 seconds (balance between responsiveness and false positives)
///
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

    /// Minimum valid timeout: 30 seconds (used in validation)
    #[cfg(test)]
    const MIN_SECS: u64 = 30;

    /// Maximum valid timeout: 120 seconds (used in validation)
    #[cfg(test)]
    const MAX_SECS: u64 = 120;

    /// Create a new LivenessTimeout with validation.
    ///
    /// # Arguments
    /// * `duration` - The timeout duration (must be 30-120 seconds)
    ///
    /// # Returns
    /// - `Ok(LivenessTimeout)` if duration is within valid range
    /// - `Err(io::Error)` with InvalidInput kind if duration is out of range
    ///
    /// # Boundary Behavior
    ///
    /// Sub-second precision is supported within the valid range:
    /// - `30.0s` to `120.0s` inclusive are valid
    /// - `29.999s` is rejected (floor is 30 whole seconds)
    /// - `120.001s` is rejected (ceiling is exactly 120 seconds)
    /// - `30.5s`, `60.123s`, etc. are accepted
    ///
    /// This asymmetry is intentional: the minimum ensures adequate time for
    /// slow but responsive servers, while the maximum strictly bounds
    /// user wait time (not even 1ms over 120s).
    ///
    /// # Note
    /// Currently only used in tests. Production code uses `default()`.
    /// This method will be used when configurable timeout is exposed via config.
    #[cfg(test)]
    pub(crate) fn new(duration: Duration) -> std::io::Result<Self> {
        let secs = duration.as_secs();
        let has_subsec = duration.subsec_nanos() > 0;

        // Check minimum: must be at least 30 whole seconds
        // 29.999s has secs=29, so it's rejected. 30.001s has secs=30, so it's accepted.
        if secs < Self::MIN_SECS {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "Liveness timeout must be at least {}s, got {:?}",
                    Self::MIN_SECS,
                    duration
                ),
            ));
        }

        // Check maximum: must be at most exactly 120 seconds
        // 120.0s is accepted. 120.001s is rejected (has_subsec is true when secs=120).
        if secs > Self::MAX_SECS || (secs == Self::MAX_SECS && has_subsec) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "Liveness timeout must be at most {}s, got {:?}",
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

impl Default for LivenessTimeout {
    fn default() -> Self {
        Self(Duration::from_secs(Self::DEFAULT_SECS))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that LivenessTimeout type rejects values outside 30-120s range.
    ///
    /// ADR-0018 specifies the liveness timeout should be 30-120s (Tier 2).
    /// This test verifies the newtype validation rejects out-of-range values.
    #[test]
    fn liveness_timeout_rejects_out_of_range() {
        // Below minimum: 29 seconds
        let too_short = LivenessTimeout::new(Duration::from_secs(29));
        assert!(too_short.is_err(), "29s should be rejected as too short");

        // Above maximum: 121 seconds
        let too_long = LivenessTimeout::new(Duration::from_secs(121));
        assert!(too_long.is_err(), "121s should be rejected as too long");

        // Zero duration
        let zero = LivenessTimeout::new(Duration::ZERO);
        assert!(zero.is_err(), "0s should be rejected");
    }

    /// Test that LivenessTimeout type accepts values within 30-120s range.
    #[test]
    fn liveness_timeout_accepts_valid_range() {
        // Minimum boundary: 30 seconds
        let min = LivenessTimeout::new(Duration::from_secs(30));
        assert!(min.is_ok(), "30s should be accepted (minimum)");

        // Maximum boundary: 120 seconds
        let max = LivenessTimeout::new(Duration::from_secs(120));
        assert!(max.is_ok(), "120s should be accepted (maximum)");

        // Middle of range: 60 seconds
        let mid = LivenessTimeout::new(Duration::from_secs(60));
        assert!(mid.is_ok(), "60s should be accepted (middle)");
    }

    /// Test that LivenessTimeout provides access to inner Duration.
    #[test]
    fn liveness_timeout_as_duration() {
        let timeout = LivenessTimeout::new(Duration::from_secs(60)).expect("60s is valid");

        assert_eq!(timeout.as_duration(), Duration::from_secs(60));
    }

    /// Test sub-second boundary validation.
    ///
    /// Per the documented boundary behavior (following GlobalShutdownTimeout):
    /// - Minimum: floor at 30 whole seconds (29.999s rejected, 30.001s accepted)
    /// - Maximum: ceiling at exactly 120 seconds (120.001s rejected)
    #[test]
    fn liveness_timeout_subsecond_boundaries() {
        // 29.999s has secs=29, rejected (floor is 30 whole seconds)
        let just_under_min = LivenessTimeout::new(Duration::from_millis(29999));
        assert!(
            just_under_min.is_err(),
            "29.999s should be rejected (secs=29, under minimum)"
        );

        // 30.001s has secs=30, accepted
        let just_over_min = LivenessTimeout::new(Duration::from_millis(30001));
        assert!(
            just_over_min.is_ok(),
            "30.001s should be accepted (secs=30)"
        );

        // 60.5s accepted (mid-range subsecond)
        let mid_subsec = LivenessTimeout::new(Duration::from_millis(60500));
        assert!(mid_subsec.is_ok(), "60.5s should be accepted");

        // 120.0s exactly accepted (maximum boundary)
        let exact_max = LivenessTimeout::new(Duration::from_secs(120));
        assert!(exact_max.is_ok(), "120.0s exactly should be accepted");

        // 120.001s rejected (ceiling is exactly 120s)
        let just_over_max = LivenessTimeout::new(Duration::from_millis(120001));
        assert!(
            just_over_max.is_err(),
            "120.001s should be rejected (over maximum)"
        );

        // 120s + 1 nanosecond rejected
        let one_nano_over =
            LivenessTimeout::new(Duration::from_secs(120) + Duration::from_nanos(1));
        assert!(
            one_nano_over.is_err(),
            "120s + 1ns should be rejected (ceiling is exactly 120s)"
        );
    }

    /// Test default LivenessTimeout value.
    ///
    /// Default should be exactly 60s per ADR-0018 recommendation - a balance between
    /// detecting hung servers quickly and avoiding false positives for slow servers.
    #[test]
    fn liveness_timeout_default() {
        let default_timeout = LivenessTimeout::default();

        assert_eq!(
            default_timeout.as_duration(),
            Duration::from_secs(60),
            "Default should be exactly 60s per ADR-0018"
        );
    }
}
