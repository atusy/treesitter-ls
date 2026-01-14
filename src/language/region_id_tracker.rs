//! Stable region ID tracking for injection regions.
//!
//! This module provides ULID-based identifiers for injection regions
//! that remain stable across document edits (Phase 1: ordinal-based).

use dashmap::DashMap;
use std::collections::HashMap;
use ulid::Ulid;
use url::Url;

/// Tracks stable ULID-based identifiers for injection regions.
///
/// Phase 1: Uses ordinal-based keys (language, ordinal).
/// Phase 2: Will migrate to position-based keys.
pub(crate) struct RegionIdTracker {
    entries: DashMap<Url, HashMap<OrdinalKey, Ulid>>,
}

/// Key for Phase 1 ordinal-based tracking.
#[derive(Hash, Eq, PartialEq, Clone, Debug)]
struct OrdinalKey {
    language: String,
    ordinal: usize,
}

impl RegionIdTracker {
    /// Create a new empty tracker.
    pub fn new() -> Self {
        Self {
            entries: DashMap::new(),
        }
    }

    /// Get or create a stable ULID for an injection region.
    ///
    /// Phase 1: Uses ordinal-based lookup. Same (uri, language, ordinal)
    /// always returns the same ULID within a session.
    pub fn get_or_create(&self, uri: &Url, language: &str, ordinal: usize) -> Ulid {
        let key = OrdinalKey {
            language: language.to_string(),
            ordinal,
        };

        // NOTE: Explicit two-step pattern to avoid DashMap lifetime ambiguity.
        let mut entry = self.entries.entry(uri.clone()).or_default();
        *entry.entry(key).or_insert_with(Ulid::new)
    }

    /// Remove all tracked regions for a document.
    ///
    /// Called on didClose to prevent memory leaks.
    pub fn cleanup(&self, uri: &Url) {
        self.entries.remove(uri);
    }
}

impl Default for RegionIdTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_uri(name: &str) -> Url {
        Url::parse(&format!("file:///test/{}.md", name)).unwrap()
    }

    #[test]
    fn test_new_tracker_is_empty() {
        let tracker = RegionIdTracker::new();
        // No direct way to check emptiness, but get_or_create should work
        let uri = test_uri("empty");
        let ulid = tracker.get_or_create(&uri, "lua", 0);
        assert!(!ulid.to_string().is_empty(), "ULID should be generated");
    }

    #[test]
    fn test_same_key_returns_same_ulid() {
        let tracker = RegionIdTracker::new();
        let uri = test_uri("same");

        let ulid1 = tracker.get_or_create(&uri, "lua", 0);
        let ulid2 = tracker.get_or_create(&uri, "lua", 0);

        assert_eq!(ulid1, ulid2, "Same key should return same ULID");
    }

    #[test]
    fn test_different_ordinal_returns_different_ulid() {
        let tracker = RegionIdTracker::new();
        let uri = test_uri("ordinal");

        let ulid0 = tracker.get_or_create(&uri, "lua", 0);
        let ulid1 = tracker.get_or_create(&uri, "lua", 1);

        assert_ne!(
            ulid0, ulid1,
            "Different ordinals should return different ULIDs"
        );
    }

    #[test]
    fn test_different_language_returns_different_ulid() {
        let tracker = RegionIdTracker::new();
        let uri = test_uri("language");

        let lua_ulid = tracker.get_or_create(&uri, "lua", 0);
        let python_ulid = tracker.get_or_create(&uri, "python", 0);

        assert_ne!(
            lua_ulid, python_ulid,
            "Different languages should return different ULIDs"
        );
    }

    #[test]
    fn test_different_uri_returns_different_ulid() {
        let tracker = RegionIdTracker::new();
        let uri1 = test_uri("doc1");
        let uri2 = test_uri("doc2");

        let ulid1 = tracker.get_or_create(&uri1, "lua", 0);
        let ulid2 = tracker.get_or_create(&uri2, "lua", 0);

        assert_ne!(ulid1, ulid2, "Different URIs should return different ULIDs");
    }

    #[test]
    fn test_cleanup_removes_document_entries() {
        let tracker = RegionIdTracker::new();
        let uri = test_uri("cleanup");

        // Create some entries
        let ulid_before = tracker.get_or_create(&uri, "lua", 0);

        // Cleanup
        tracker.cleanup(&uri);

        // After cleanup, same key should create NEW ULID
        let ulid_after = tracker.get_or_create(&uri, "lua", 0);

        assert_ne!(
            ulid_before, ulid_after,
            "After cleanup, new ULID should be generated"
        );
    }

    #[test]
    fn test_cleanup_does_not_affect_other_documents() {
        let tracker = RegionIdTracker::new();
        let uri1 = test_uri("keep");
        let uri2 = test_uri("remove");

        // Create entries for both documents
        let ulid1_before = tracker.get_or_create(&uri1, "lua", 0);
        let _ulid2 = tracker.get_or_create(&uri2, "lua", 0);

        // Cleanup only uri2
        tracker.cleanup(&uri2);

        // uri1 should still have its ULID
        let ulid1_after = tracker.get_or_create(&uri1, "lua", 0);
        assert_eq!(
            ulid1_before, ulid1_after,
            "Cleanup should not affect other documents"
        );
    }

    #[test]
    fn test_ulid_format_is_valid() {
        let tracker = RegionIdTracker::new();
        let uri = test_uri("format");

        let ulid = tracker.get_or_create(&uri, "lua", 0);
        let ulid_str = ulid.to_string();

        // ULID is 26 characters, uppercase alphanumeric
        assert_eq!(ulid_str.len(), 26, "ULID should be 26 characters");
        assert!(
            ulid_str.chars().all(|c| c.is_ascii_alphanumeric()),
            "ULID should be alphanumeric"
        );
    }

    #[test]
    fn test_concurrent_get_or_create_returns_same_ulid() {
        use std::sync::Arc;
        use std::thread;

        let tracker = Arc::new(RegionIdTracker::new());
        let uri = test_uri("concurrent");

        // Spawn 10 threads that all try to get_or_create the same key
        let handles: Vec<_> = (0..10)
            .map(|_| {
                let tracker = Arc::clone(&tracker);
                let uri = uri.clone();
                thread::spawn(move || tracker.get_or_create(&uri, "lua", 0))
            })
            .collect();

        // Collect all ULIDs
        let ulids: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

        // All ULIDs should be identical (thread-safe get-or-create)
        let first = &ulids[0];
        assert!(
            ulids.iter().all(|ulid| ulid == first),
            "All concurrent get_or_create calls should return the same ULID"
        );
    }

    #[test]
    fn test_concurrent_different_keys_returns_different_ulids() {
        use std::sync::Arc;
        use std::thread;

        let tracker = Arc::new(RegionIdTracker::new());
        let uri = test_uri("concurrent_diff");

        // Spawn threads that get_or_create different ordinals
        let handles: Vec<_> = (0..5)
            .map(|ordinal| {
                let tracker = Arc::clone(&tracker);
                let uri = uri.clone();
                thread::spawn(move || (ordinal, tracker.get_or_create(&uri, "lua", ordinal)))
            })
            .collect();

        // Collect all (ordinal, ULID) pairs
        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

        // All ULIDs should be different from each other
        for i in 0..results.len() {
            for j in (i + 1)..results.len() {
                assert_ne!(
                    results[i].1, results[j].1,
                    "Different ordinals {} and {} should have different ULIDs",
                    results[i].0, results[j].0
                );
            }
        }
    }
}
