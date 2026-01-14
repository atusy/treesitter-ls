//! Stable region ID tracking for injection regions.
//!
//! This module provides ULID-based identifiers for injection regions
//! that remain stable across document edits (Phase 2: position-based with START-priority).

use dashmap::DashMap;
use log::warn;
use std::collections::HashMap;
use ulid::Ulid;
use url::Url;

/// Tracks stable ULID-based identifiers for injection regions.
///
/// Phase 2: Uses position-based keys (start_byte, end_byte, kind) per ADR-0019.
/// Applies START-priority invalidation rules to maintain stable ULIDs across edits.
pub(crate) struct RegionIdTracker {
    entries: DashMap<Url, HashMap<PositionKey, Ulid>>,
}

/// Key for Phase 2 position-based tracking (ADR-0019 composite key).
///
/// Note: Language is NOT part of the key per ADR-0019 specification.
/// Same position with different language gets different ULID because
/// kind will differ in the AST (e.g., fenced_code_block vs code_block).
#[derive(Hash, Eq, PartialEq, Clone, Debug)]
struct PositionKey {
    start_byte: usize,
    end_byte: usize,
    kind: String,
}

/// Information about a single edit operation.
struct EditInfo {
    start_byte: usize,
    old_end_byte: usize,
    new_end_byte: usize,
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
    /// Phase 2: Uses position-based lookup (ADR-0019 composite key).
    /// Same (uri, start_byte, end_byte, kind) always returns the same ULID.
    pub(crate) fn get_or_create(
        &self,
        uri: &Url,
        start: usize,
        end: usize,
        kind: &str,
    ) -> Ulid {
        let key = PositionKey {
            start_byte: start,
            end_byte: end,
            kind: kind.to_string(),
        };

        // NOTE: Explicit two-step pattern to avoid DashMap lifetime ambiguity.
        let mut entry = self.entries.entry(uri.clone()).or_default();
        *entry.entry(key).or_insert_with(Ulid::new)
    }

    /// Apply text change and update region positions using START-priority invalidation.
    ///
    /// This method reconstructs the edit operation from old and new text using
    /// character-level diff, then applies ADR-0019 invalidation rules.
    ///
    /// # Fast path
    /// If old_text == new_text, returns immediately without any processing.
    pub(crate) fn apply_text_change(&self, uri: &Url, old_text: &str, new_text: &str) {
        // Fast path: identical texts need no processing
        if old_text == new_text {
            return;
        }

        if let Some(edit) = Self::reconstruct_merged_edit(old_text, new_text) {
            self.apply_single_edit(uri, &edit);
        }
    }

    /// Reconstruct a single merged edit from character-level diff.
    ///
    /// Returns None if texts are identical (no edit needed).
    /// Merges all changes into one edit: [first_change_start, last_change_end_old)
    /// maps to [first_change_start, last_change_end_new).
    fn reconstruct_merged_edit(old_text: &str, new_text: &str) -> Option<EditInfo> {
        use similar::{ChangeTag, TextDiff};

        // NOTE: from_chars() for character-level diff (byte positions tracked via .len())
        // from_lines() would cause line-level granularity and over-invalidation
        let diff = TextDiff::from_chars(old_text, new_text);

        let mut first_change_start: Option<usize> = None;
        let mut last_old_end: usize = 0;
        let mut last_new_end: usize = 0;
        let mut old_byte = 0;
        let mut new_byte = 0;

        for change in diff.iter_all_changes() {
            match change.tag() {
                ChangeTag::Equal => {
                    old_byte += change.value().len();
                    new_byte += change.value().len();
                }
                ChangeTag::Delete => {
                    if first_change_start.is_none() {
                        first_change_start = Some(old_byte);
                    }
                    old_byte += change.value().len();
                    last_old_end = old_byte;
                    last_new_end = new_byte;
                }
                ChangeTag::Insert => {
                    if first_change_start.is_none() {
                        first_change_start = Some(old_byte);
                    }
                    new_byte += change.value().len();
                    last_old_end = old_byte;
                    last_new_end = new_byte;
                }
            }
        }

        first_change_start.map(|start| EditInfo {
            start_byte: start,
            old_end_byte: last_old_end,
            new_end_byte: last_new_end,
        })
    }

    /// Apply a single edit operation with START-priority invalidation (ADR-0019).
    fn apply_single_edit(&self, uri: &Url, edit: &EditInfo) {
        let delta = edit.new_end_byte as i64 - edit.old_end_byte as i64;

        let Some(mut entries) = self.entries.get_mut(uri) else {
            return;
        };

        let mut new_entries = HashMap::new();

        for (key, ulid) in entries.drain() {
            // START-priority rule with zero-length edit handling (ADR-0019)
            //
            // ADR-0019 line 74: "Zero-length edits: When edit.start == edit.old_end,
            // preserve identity only if the node's START in the new tree is unchanged."
            //
            // ## Conservative Simplification
            //
            // ADR-0019 strictly requires comparing old-tree START vs new-tree START.
            // However, RegionIdTracker doesn't have access to the parsed tree
            // (separation of concerns). We use a conservative approximation:
            //
            // - Zero-length insert AT node's START → always INVALIDATE
            //   (Rationale: insert shifts node down, so START changes in new tree)
            // - Zero-length insert BEFORE/AFTER node's START → KEEP
            //
            // This may over-invalidate in rare cases where the node's START
            // happens to remain unchanged despite being at the insert point,
            // but it's safe (never preserves stale identity).
            let should_invalidate = if edit.start_byte == edit.old_end_byte {
                // Zero-length insert: invalidate if insert is AT node's START
                // (conservative: node likely shifts down in new tree)
                key.start_byte == edit.start_byte
            } else {
                // Normal edit: invalidate if START is inside [edit.start, edit.old_end)
                // Note: Half-open interval - start is inclusive, old_end is exclusive
                key.start_byte >= edit.start_byte && key.start_byte < edit.old_end_byte
            };

            if should_invalidate {
                continue; // INVALIDATE
            }

            // Position adjustment with overflow protection (saturating arithmetic)
            let new_key = if key.start_byte >= edit.old_end_byte {
                // Node E: AFTER edit → shift
                PositionKey {
                    start_byte: (key.start_byte as i64).saturating_add(delta).max(0) as usize,
                    end_byte: (key.end_byte as i64).saturating_add(delta).max(0) as usize,
                    kind: key.kind,
                }
            } else if key.end_byte > edit.start_byte {
                // Node A/B: CONTAINS edit → adjust end only
                let new_end = (key.end_byte as i64).saturating_add(delta).max(0) as usize;
                // Guard: If range collapses (start >= end), invalidate instead
                if new_end <= key.start_byte {
                    continue; // INVALIDATE: range collapsed to zero or negative
                }
                PositionKey {
                    start_byte: key.start_byte,
                    end_byte: new_end,
                    kind: key.kind,
                }
            } else {
                // Node F: BEFORE edit → unchanged
                key
            };

            // Handle potential position collision after adjustment
            // Two nodes may collapse to same (start, end, kind) after large edits
            //
            // Policy: "first wins" - deterministic based on HashMap iteration order.
            // Collisions are rare and indicate extreme edits (e.g., massive deletion
            // causing multiple nodes to collapse to same position).
            //
            // Note: Collisions may also indicate a bug in invalidation logic -
            // if two nodes can reach the same position, one should have been
            // invalidated earlier. Log at warn level for observability.
            use std::collections::hash_map::Entry;
            match new_entries.entry(new_key.clone()) {
                Entry::Vacant(e) => {
                    e.insert(ulid);
                }
                Entry::Occupied(_) => {
                    // Collision: keep first entry, log dropped ULID for debugging
                    warn!(
                        target: "treesitter_ls::region_tracker",
                        "Position collision after edit - ULID mapping dropped: ulid={}, start={}, end={}, kind={}",
                        ulid, new_key.start_byte, new_key.end_byte, new_key.kind
                    );
                }
            }
        }

        *entries = new_entries;
    }

    /// Remove all tracked regions for a document.
    ///
    /// Called on didClose to prevent memory leaks.
    pub(crate) fn cleanup(&self, uri: &Url) {
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
        let ulid = tracker.get_or_create(&uri, 0, 10, "code_block");
        assert!(!ulid.to_string().is_empty(), "ULID should be generated");
    }

    #[test]
    fn test_same_position_returns_same_ulid() {
        let tracker = RegionIdTracker::new();
        let uri = test_uri("same");

        let ulid1 = tracker.get_or_create(&uri, 0, 10, "code_block");
        let ulid2 = tracker.get_or_create(&uri, 0, 10, "code_block");

        assert_eq!(ulid1, ulid2, "Same position key should return same ULID");
    }

    #[test]
    fn test_different_start_returns_different_ulid() {
        let tracker = RegionIdTracker::new();
        let uri = test_uri("start");

        let ulid0 = tracker.get_or_create(&uri, 0, 10, "code_block");
        let ulid1 = tracker.get_or_create(&uri, 10, 20, "code_block");

        assert_ne!(
            ulid0, ulid1,
            "Different start positions should return different ULIDs"
        );
    }

    #[test]
    fn test_different_kind_returns_different_ulid() {
        let tracker = RegionIdTracker::new();
        let uri = test_uri("kind");

        let block_ulid = tracker.get_or_create(&uri, 0, 10, "code_block");
        let fence_ulid = tracker.get_or_create(&uri, 0, 10, "fenced_code_block");

        assert_ne!(
            block_ulid, fence_ulid,
            "Different kinds should return different ULIDs"
        );
    }

    #[test]
    fn test_different_uri_returns_different_ulid() {
        let tracker = RegionIdTracker::new();
        let uri1 = test_uri("doc1");
        let uri2 = test_uri("doc2");

        let ulid1 = tracker.get_or_create(&uri1, 0, 10, "code_block");
        let ulid2 = tracker.get_or_create(&uri2, 0, 10, "code_block");

        assert_ne!(ulid1, ulid2, "Different URIs should return different ULIDs");
    }

    #[test]
    fn test_cleanup_removes_document_entries() {
        let tracker = RegionIdTracker::new();
        let uri = test_uri("cleanup");

        // Create some entries
        let ulid_before = tracker.get_or_create(&uri, 0, 10, "code_block");

        // Cleanup
        tracker.cleanup(&uri);

        // After cleanup, same key should create NEW ULID
        let ulid_after = tracker.get_or_create(&uri, 0, 10, "code_block");

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
        let ulid1_before = tracker.get_or_create(&uri1, 0, 10, "code_block");
        let _ulid2 = tracker.get_or_create(&uri2, 0, 10, "code_block");

        // Cleanup only uri2
        tracker.cleanup(&uri2);

        // uri1 should still have its ULID
        let ulid1_after = tracker.get_or_create(&uri1, 0, 10, "code_block");
        assert_eq!(
            ulid1_before, ulid1_after,
            "Cleanup should not affect other documents"
        );
    }

    #[test]
    fn test_ulid_format_is_valid() {
        let tracker = RegionIdTracker::new();
        let uri = test_uri("format");

        let ulid = tracker.get_or_create(&uri, 0, 10, "code_block");
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
                thread::spawn(move || tracker.get_or_create(&uri, 0, 10, "code_block"))
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

        // Spawn threads that get_or_create different positions
        let handles: Vec<_> = (0..5)
            .map(|offset| {
                let tracker = Arc::clone(&tracker);
                let uri = uri.clone();
                let start = offset * 10;
                thread::spawn(move || {
                    (offset, tracker.get_or_create(&uri, start, start + 10, "code_block"))
                })
            })
            .collect();

        // Collect all (offset, ULID) pairs
        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

        // All ULIDs should be different from each other
        for i in 0..results.len() {
            for j in (i + 1)..results.len() {
                assert_ne!(
                    results[i].1, results[j].1,
                    "Different positions {} and {} should have different ULIDs",
                    results[i].0, results[j].0
                );
            }
        }
    }
}
