//! LSP polling utilities for E2E tests.
//!
//! Provides retry-with-timeout patterns for waiting on async LSP responses.

use std::time::Duration;

/// Poll until a predicate returns Some(T) or max attempts reached.
///
/// # Arguments
/// * `max_attempts` - Maximum number of polling attempts
/// * `delay_ms` - Delay between attempts in milliseconds
/// * `predicate` - Function that returns Some(T) on success, None to retry
///
/// # Returns
/// * `Some(T)` if predicate succeeded within max_attempts
/// * `None` if max_attempts reached without success
pub fn poll_until<T, F>(max_attempts: usize, delay_ms: u64, mut predicate: F) -> Option<T>
where
    F: FnMut() -> Option<T>,
{
    for attempt in 1..=max_attempts {
        if let Some(result) = predicate() {
            eprintln!("poll_until succeeded on attempt {}/{}", attempt, max_attempts);
            return Some(result);
        }

        if attempt < max_attempts {
            std::thread::sleep(Duration::from_millis(delay_ms));
        }
    }

    eprintln!("poll_until exhausted {} attempts", max_attempts);
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_poll_until_succeeds_immediately() {
        let result = poll_until(5, 10, || Some(42));
        assert_eq!(result, Some(42));
    }

    #[test]
    fn test_poll_until_succeeds_after_retries() {
        let mut counter = 0;
        let result = poll_until(5, 10, || {
            counter += 1;
            if counter >= 3 {
                Some(counter)
            } else {
                None
            }
        });
        assert_eq!(result, Some(3));
    }

    #[test]
    fn test_poll_until_exhausts_attempts() {
        let result = poll_until(3, 10, || None::<i32>);
        assert_eq!(result, None);
    }
}
