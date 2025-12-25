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

    #[test]
    fn test_next_result_id_monotonic() {
        let id1: u64 = next_result_id().parse().expect("should be numeric");
        let id2: u64 = next_result_id().parse().expect("should be numeric");
        let id3: u64 = next_result_id().parse().expect("should be numeric");

        assert!(
            id2 > id1,
            "id2 ({}) should be greater than id1 ({})",
            id2,
            id1
        );
        assert!(
            id3 > id2,
            "id3 ({}) should be greater than id2 ({})",
            id3,
            id2
        );
    }

    #[test]
    fn test_next_result_id_concurrent() {
        use std::collections::HashSet;
        use std::sync::Arc;
        use std::thread;

        const NUM_THREADS: usize = 10;
        const IDS_PER_THREAD: usize = 100;

        let results: Arc<std::sync::Mutex<Vec<String>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));

        let handles: Vec<_> = (0..NUM_THREADS)
            .map(|_| {
                let results = Arc::clone(&results);
                thread::spawn(move || {
                    for _ in 0..IDS_PER_THREAD {
                        let id = next_result_id();
                        results.lock().unwrap().push(id);
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().expect("thread should not panic");
        }

        let all_ids = results.lock().unwrap();
        let unique_ids: HashSet<_> = all_ids.iter().collect();

        assert_eq!(
            all_ids.len(),
            unique_ids.len(),
            "All {} IDs should be unique, but found {} duplicates",
            all_ids.len(),
            all_ids.len() - unique_ids.len()
        );
    }
}
