//! Atomic result_id generation for semantic tokens.
//!
//! Provides sequential, thread-safe result IDs for LSP semantic token responses.

use std::sync::atomic::{AtomicU64, Ordering};

static TOKEN_RESULT_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Generate a unique, monotonically increasing result_id for semantic tokens.
///
/// This function is thread-safe and returns sequential string IDs like "1", "2", "3", etc.
pub fn next_result_id() -> String {
    TOKEN_RESULT_COUNTER
        .fetch_add(1, Ordering::SeqCst)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_next_result_id_returns_string() {
        let id: String = next_result_id();
        assert!(!id.is_empty(), "result_id should not be empty");
    }
}
