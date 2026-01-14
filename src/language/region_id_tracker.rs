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

impl EditInfo {
    /// Calculate the byte delta (positive for insertion, negative for deletion).
    fn delta(&self) -> i64 {
        self.new_end_byte as i64 - self.old_end_byte as i64
    }

    /// Check if this is a zero-length (insertion-only) edit.
    ///
    /// Zero-length edits have special handling in ADR-0019:
    /// they insert content without deleting anything.
    fn is_insertion_only(&self) -> bool {
        self.start_byte == self.old_end_byte
    }
}

/// Apply a signed delta to a byte position with overflow protection.
///
/// Uses saturating arithmetic to prevent overflow/underflow:
/// - Clamps result to 0 if delta would make it negative
/// - Uses i64 internally to handle large negative deltas safely
fn apply_delta(position: usize, delta: i64) -> usize {
    (position as i64).saturating_add(delta).max(0) as usize
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
    pub(crate) fn get_or_create(&self, uri: &Url, start: usize, end: usize, kind: &str) -> Ulid {
        let key = PositionKey {
            start_byte: start,
            end_byte: end,
            kind: kind.to_string(),
        };

        // NOTE: Explicit two-step pattern to avoid DashMap lifetime ambiguity.
        let mut entry = self.entries.entry(uri.clone()).or_default();
        *entry.entry(key).or_insert_with(Ulid::new)
    }

    /// Get the ULID for a position if it exists, without creating it.
    ///
    /// Returns None if no entry exists for this position.
    /// Used in tests to verify position adjustment without side effects.
    #[cfg(test)]
    fn get(&self, uri: &Url, start: usize, end: usize, kind: &str) -> Option<Ulid> {
        let key = PositionKey {
            start_byte: start,
            end_byte: end,
            kind: kind.to_string(),
        };
        self.entries.get(uri)?.get(&key).copied()
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

    /// Determine if a node should be invalidated based on START-priority rule (ADR-0019).
    ///
    /// # ADR-0019 START-Priority Rule
    ///
    /// A node is invalidated if its START position falls inside the edit range
    /// `[edit.start, edit.old_end)` (half-open interval: start inclusive, end exclusive).
    ///
    /// # Zero-Length Edit Handling
    ///
    /// ADR-0019 line 74: "Zero-length edits: When edit.start == edit.old_end,
    /// preserve identity only if the node's START in the new tree is unchanged."
    ///
    /// ## Conservative Simplification
    ///
    /// ADR-0019 strictly requires comparing old-tree START vs new-tree START.
    /// However, RegionIdTracker doesn't have access to the parsed tree
    /// (separation of concerns). We use a conservative approximation:
    ///
    /// - Zero-length insert AT node's START → always INVALIDATE
    ///   (Rationale: insert shifts node down, so START changes in new tree)
    /// - Zero-length insert BEFORE/AFTER node's START → KEEP
    ///
    /// This may over-invalidate in rare cases where the node's START
    /// happens to remain unchanged despite being at the insert point,
    /// but it's safe (never preserves stale identity).
    fn should_invalidate_node(key: &PositionKey, edit: &EditInfo) -> bool {
        if edit.is_insertion_only() {
            // Zero-length insert: invalidate if insert is AT node's START
            key.start_byte == edit.start_byte
        } else {
            // Normal edit: invalidate if START is inside [edit.start, edit.old_end)
            key.start_byte >= edit.start_byte && key.start_byte < edit.old_end_byte
        }
    }

    /// Apply a single edit operation with START-priority invalidation (ADR-0019).
    fn apply_single_edit(&self, uri: &Url, edit: &EditInfo) {
        let delta = edit.delta();

        let Some(mut entries) = self.entries.get_mut(uri) else {
            return;
        };

        let mut new_entries = HashMap::new();

        for (key, ulid) in entries.drain() {
            if Self::should_invalidate_node(&key, edit) {
                continue; // INVALIDATE
            }

            // Position adjustment
            let new_key = if key.start_byte >= edit.old_end_byte {
                // Node E: AFTER edit → shift both start and end
                PositionKey {
                    start_byte: apply_delta(key.start_byte, delta),
                    end_byte: apply_delta(key.end_byte, delta),
                    kind: key.kind,
                }
            } else if key.end_byte > edit.start_byte {
                // Node A/B: CONTAINS edit → adjust end only
                let new_end = apply_delta(key.end_byte, delta);
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
                    (
                        offset,
                        tracker.get_or_create(&uri, start, start + 10, "code_block"),
                    )
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

    // ============================================================
    // Concurrent apply_text_change() Tests
    // ============================================================
    // These tests verify thread-safety when multiple threads call
    // apply_text_change() concurrently on the same URI.

    #[test]
    fn test_concurrent_apply_text_change_same_uri_no_panic() {
        use std::sync::Arc;
        use std::thread;

        let tracker = Arc::new(RegionIdTracker::new());
        let uri = test_uri("concurrent_edit");

        // Pre-populate with several nodes
        for i in 0..10 {
            let start = i * 20;
            tracker.get_or_create(&uri, start, start + 15, "block");
        }

        // Spawn multiple threads that apply different edits concurrently
        // Each thread applies a small edit at different positions
        let handles: Vec<_> = (0..5)
            .map(|thread_id| {
                let tracker = Arc::clone(&tracker);
                let uri = uri.clone();
                thread::spawn(move || {
                    // Each thread does multiple edit cycles
                    for cycle in 0..3 {
                        // Create text with edit at different positions per thread
                        let edit_pos = (thread_id * 30 + cycle * 5) % 150;
                        let old_text = text_with_markers(200);
                        let mut new_text = old_text.clone();

                        // Apply a small deletion
                        if edit_pos + 5 <= new_text.len() {
                            new_text.replace_range(edit_pos..edit_pos + 5, "");
                            tracker.apply_text_change(&uri, &old_text, &new_text);
                        }
                    }
                })
            })
            .collect();

        // Wait for all threads to complete - no panics should occur
        for handle in handles {
            handle
                .join()
                .expect("Thread should not panic during concurrent edits");
        }

        // Verify tracker is still functional after concurrent edits
        // (create a new entry to ensure no corruption)
        let new_ulid = tracker.get_or_create(&uri, 1000, 1010, "test");
        assert!(
            new_ulid.to_string().len() == 26,
            "ULID should be valid after concurrent edits"
        );
    }

    #[test]
    fn test_concurrent_apply_text_change_different_uris_independent() {
        use std::sync::Arc;
        use std::thread;

        let tracker = Arc::new(RegionIdTracker::new());

        // Each thread works on its own URI
        let handles: Vec<_> = (0..5)
            .map(|thread_id| {
                let tracker = Arc::clone(&tracker);
                let uri = test_uri(&format!("uri_{}", thread_id));

                thread::spawn(move || {
                    // Create initial node
                    let ulid = tracker.get_or_create(&uri, 50, 100, "block");

                    // Apply an edit that shifts the node
                    let old_text = text_with_markers(200);
                    let mut new_text = old_text.clone();
                    new_text.replace_range(20..30, ""); // Delete before node, delta = -10

                    tracker.apply_text_change(&uri, &old_text, &new_text);

                    // Verify the node was shifted correctly
                    let shifted_ulid = tracker.get(&uri, 40, 90, "block");
                    (ulid, shifted_ulid)
                })
            })
            .collect();

        // Collect results and verify each URI maintained correct state
        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

        for (original, shifted) in results {
            assert_eq!(
                Some(original),
                shifted,
                "Each URI should independently maintain correct position adjustment"
            );
        }
    }

    #[test]
    fn test_concurrent_get_and_apply_interleaved() {
        use std::sync::Arc;
        use std::thread;

        let tracker = Arc::new(RegionIdTracker::new());
        let uri = test_uri("interleaved");

        // Pre-populate with one stable node that won't be affected by edits
        // Node at [0, 10) - edits will be at [50+)
        let stable_ulid = tracker.get_or_create(&uri, 0, 10, "stable");

        // Spawn threads that interleave get_or_create and apply_text_change
        let handles: Vec<_> = (0..4)
            .map(|thread_id| {
                let tracker = Arc::clone(&tracker);
                let uri = uri.clone();
                let stable_ulid = stable_ulid;

                thread::spawn(move || {
                    let mut results = Vec::new();

                    for i in 0..5 {
                        // Alternate between creating new entries and applying edits
                        if i % 2 == 0 {
                            // Create new entry at high position (won't conflict with edits)
                            let pos = 500 + thread_id * 100 + i * 10;
                            tracker.get_or_create(&uri, pos, pos + 5, "dynamic");
                        } else {
                            // Apply edit at mid-range (affects nodes at [50, 100))
                            let old_text = text_with_markers(200);
                            let mut new_text = old_text.clone();
                            let edit_start = 60 + thread_id * 5;
                            if edit_start + 5 <= new_text.len() {
                                new_text.replace_range(edit_start..edit_start + 5, "");
                                tracker.apply_text_change(&uri, &old_text, &new_text);
                            }
                        }

                        // Always verify stable node is accessible
                        // (might be accessed during concurrent edits elsewhere)
                        if let Some(ulid) = tracker.get(&uri, 0, 10, "stable") {
                            results.push(ulid);
                        }
                    }

                    // Verify stable node's ULID was consistent across all reads
                    (stable_ulid, results)
                })
            })
            .collect();

        // Verify all threads saw consistent stable ULID
        for handle in handles {
            let (expected, observed) = handle.join().expect("Thread should not panic");
            for ulid in observed {
                assert_eq!(
                    expected, ulid,
                    "Stable node should have consistent ULID across concurrent access"
                );
            }
        }
    }

    // ============================================================
    // Phase 2 Tests: ADR-0019 START-Priority Invalidation
    // ============================================================

    /// Helper to create test text with unique characters at each position
    /// This ensures diff algorithms can correctly identify edits
    fn text_with_markers(size: usize) -> String {
        (0..size)
            .map(|i| {
                // Cycle through printable ASCII characters (33-126)
                char::from_u32(33 + (i % 94) as u32).unwrap()
            })
            .collect()
    }

    #[test]
    fn test_node_a_start_before_edit_end_after_keeps_ulid_adjusts_end() {
        // ADR-0019 Node A: Node START before edit, END after edit → KEEP (adjust end)
        let tracker = RegionIdTracker::new();
        let uri = test_uri("node_a");

        // Create node at [30, 50)
        let ulid_original = tracker.get_or_create(&uri, 30, 50, "block");

        // Edit inside the node: [35, 40) → delete 5 bytes (delta: -5)
        let old_text = text_with_markers(100);
        let mut new_text = old_text.clone();
        new_text.replace_range(35..40, ""); // Delete 5 bytes

        tracker.apply_text_change(&uri, &old_text, &new_text);

        // After edit, node should:
        // - START unchanged (30 not in [35, 40))
        // - END adjusted: 50 + (-5) = 45
        // - ULID preserved at adjusted position [30, 45)
        let ulid_after = tracker.get(&uri, 30, 45, "block");
        assert_eq!(
            Some(ulid_original),
            ulid_after,
            "Node A should preserve ULID at adjusted position [30, 45)"
        );

        // Verify old position no longer exists
        assert_eq!(
            tracker.get(&uri, 30, 50, "block"),
            None,
            "Old position [30, 50) should be removed"
        );
    }

    #[test]
    fn test_node_b_start_before_edit_end_absorbed_keeps_ulid() {
        // ADR-0019 Node B: Node START before edit, END absorbed/overlaps with edit → KEEP
        //
        // Proper Node B case: Edit partially overlaps node's end region,
        // but the adjustment doesn't cause range collapse.
        //
        // Example: Node [20, 50), edit deletes [40, 60)
        // - START 20 is NOT in [40, 60) → KEEP
        // - END 50 is in (40, 60), adjusted: 50 + (40-60) = 30
        // - New node: [20, 30) - valid range, ULID preserved
        let tracker = RegionIdTracker::new();
        let uri = test_uri("node_b");

        // Create node at [20, 50)
        let ulid_original = tracker.get_or_create(&uri, 20, 50, "block");

        // Edit overlaps end: [40, 60) → delete 20 bytes (delta = -20)
        let old_text = text_with_markers(100);
        let mut new_text = old_text.clone();
        new_text.replace_range(40..60, "");

        tracker.apply_text_change(&uri, &old_text, &new_text);

        // After edit, node should:
        // - START unchanged (20 not in [40, 60))
        // - END adjusted: 50 + (-20) = 30
        // - Range [20, 30) is valid (30 > 20), so ULID preserved
        let ulid_after = tracker.get(&uri, 20, 30, "block");
        assert_eq!(
            Some(ulid_original),
            ulid_after,
            "Node B should preserve ULID at adjusted position [20, 30)"
        );

        // Verify old position no longer exists
        assert_eq!(
            tracker.get(&uri, 20, 50, "block"),
            None,
            "Old position [20, 50) should be removed"
        );
    }

    #[test]
    fn test_node_b_end_exactly_at_edit_end_keeps_ulid() {
        // ADR-0019 Node B variant: Node END exactly at edit end → KEEP
        let tracker = RegionIdTracker::new();
        let uri = test_uri("node_b_exact");

        // Create node at [30, 50)
        let ulid_original = tracker.get_or_create(&uri, 30, 50, "block");

        // Edit ends exactly where node ends: [40, 50) → delete 10 bytes
        let old_text = text_with_markers(100);
        let mut new_text = old_text.clone();
        new_text.replace_range(40..50, "");

        tracker.apply_text_change(&uri, &old_text, &new_text);

        // After edit, node should:
        // - START unchanged (30 not in [40, 50))
        // - END adjusted: 50 + (-10) = 40
        // - ULID preserved at adjusted position [30, 40)
        let ulid_after = tracker.get(&uri, 30, 40, "block");
        assert_eq!(
            Some(ulid_original),
            ulid_after,
            "Node B should preserve ULID at adjusted position [30, 40)"
        );

        // Verify old position no longer exists
        assert_eq!(
            tracker.get(&uri, 30, 50, "block"),
            None,
            "Old position [30, 50) should be removed"
        );
    }

    #[test]
    fn test_node_c_start_inside_edit_invalidates() {
        // ADR-0019 Node C: Node START inside edit range → INVALIDATE
        let tracker = RegionIdTracker::new();
        let uri = test_uri("node_c");

        // Create node at [40, 60)
        let ulid_original = tracker.get_or_create(&uri, 40, 60, "block");

        // Edit overlaps start: [35, 45) → delete 10 bytes
        let old_text = text_with_markers(100);
        let mut new_text = old_text.clone();
        new_text.replace_range(35..45, "");

        tracker.apply_text_change(&uri, &old_text, &new_text);

        // After edit, node should be INVALIDATED (START 40 is in [35, 45))
        // Try to get with adjusted position [35, 50) - should return NEW ULID
        let ulid_after = tracker.get_or_create(&uri, 35, 50, "block");
        assert_ne!(
            ulid_original, ulid_after,
            "Node C should invalidate when START is inside edit range"
        );
    }

    #[test]
    fn test_node_d_fully_inside_edit_invalidates() {
        // ADR-0019 Node D: Node fully inside edit → INVALIDATE
        let tracker = RegionIdTracker::new();
        let uri = test_uri("node_d");

        // Create node at [40, 45)
        let ulid_original = tracker.get_or_create(&uri, 40, 45, "block");

        // Edit contains entire node: [35, 50) → delete 15 bytes
        let old_text = text_with_markers(100);
        let mut new_text = old_text.clone();
        new_text.replace_range(35..50, "");

        tracker.apply_text_change(&uri, &old_text, &new_text);

        // After edit, node should be INVALIDATED (START 40 is in [35, 50))
        let ulid_after = tracker.get_or_create(&uri, 35, 35, "block");
        assert_ne!(
            ulid_original, ulid_after,
            "Node D should invalidate when fully inside edit range"
        );
    }

    #[test]
    fn test_node_e_after_edit_keeps_ulid_shifts_position() {
        // ADR-0019 Node E: Node after edit → KEEP (shift position)
        let tracker = RegionIdTracker::new();
        let uri = test_uri("node_e");

        // Create node at [60, 80)
        let ulid_original = tracker.get_or_create(&uri, 60, 80, "block");

        // Edit before node: [30, 35) → delete 5 bytes (delta: -5)
        let old_text = text_with_markers(100);
        let mut new_text = old_text.clone();
        new_text.replace_range(30..35, "");

        tracker.apply_text_change(&uri, &old_text, &new_text);

        // After edit, node should:
        // - START shifted: 60 + (-5) = 55
        // - END shifted: 80 + (-5) = 75
        // - ULID preserved at shifted position [55, 75)
        let ulid_after = tracker.get(&uri, 55, 75, "block");
        assert_eq!(
            Some(ulid_original),
            ulid_after,
            "Node E should preserve ULID at shifted position [55, 75)"
        );

        // Verify old position no longer exists
        assert_eq!(
            tracker.get(&uri, 60, 80, "block"),
            None,
            "Old position [60, 80) should be removed"
        );
    }

    #[test]
    fn test_node_f_before_edit_unchanged_keeps_ulid() {
        // ADR-0019 Node F: Node before edit, no overlap → KEEP (unchanged)
        let tracker = RegionIdTracker::new();
        let uri = test_uri("node_f");

        // Create node at [10, 20)
        let ulid_original = tracker.get_or_create(&uri, 10, 20, "block");

        // Edit after node: [30, 35) → delete 5 bytes
        let old_text = text_with_markers(100);
        let mut new_text = old_text.clone();
        new_text.replace_range(30..35, "");

        tracker.apply_text_change(&uri, &old_text, &new_text);

        // After edit, node position unchanged
        let ulid_after = tracker.get_or_create(&uri, 10, 20, "block");
        assert_eq!(
            ulid_original, ulid_after,
            "Node F should preserve ULID and position when before edit with no overlap"
        );
    }

    #[test]
    fn test_boundary_start_at_edit_start_invalidates() {
        // Boundary condition: Node START exactly at edit.start (inclusive) → INVALIDATE
        let tracker = RegionIdTracker::new();
        let uri = test_uri("boundary_start");

        // Create node at [35, 50)
        let ulid_original = tracker.get_or_create(&uri, 35, 50, "block");

        // Edit starts exactly at node start: [35, 40)
        let old_text = text_with_markers(100);
        let mut new_text = old_text.clone();
        new_text.replace_range(35..40, "");

        tracker.apply_text_change(&uri, &old_text, &new_text);

        // START 35 is in [35, 40) (inclusive start) → INVALIDATE
        let ulid_after = tracker.get_or_create(&uri, 35, 45, "block");
        assert_ne!(
            ulid_original, ulid_after,
            "Node with START at edit.start should invalidate (inclusive boundary)"
        );
    }

    #[test]
    fn test_boundary_start_at_edit_old_end_keeps_ulid() {
        // Boundary condition: Node START exactly at edit.old_end (exclusive) → KEEP
        let tracker = RegionIdTracker::new();
        let uri = test_uri("boundary_end");

        // Create node at [40, 60)
        let ulid_original = tracker.get_or_create(&uri, 40, 60, "block");

        // Edit ends just before node: [30, 40)
        let old_text = text_with_markers(100);
        let mut new_text = old_text.clone();
        new_text.replace_range(30..40, "");

        tracker.apply_text_change(&uri, &old_text, &new_text);

        // START 40 is NOT in [30, 40) (exclusive end) → KEEP and shift
        // After edit deleting [30, 40), node shifts from [40, 60) to [30, 50)
        let ulid_after = tracker.get(&uri, 30, 50, "block");
        assert_eq!(
            Some(ulid_original),
            ulid_after,
            "Node with START at edit.old_end should keep ULID at shifted position [30, 50)"
        );

        // Verify old position no longer exists
        assert_eq!(
            tracker.get(&uri, 40, 60, "block"),
            None,
            "Old position [40, 60) should be removed"
        );
    }

    #[test]
    fn test_zero_length_insert_at_node_start_invalidates() {
        // Zero-length insert AT node START → INVALIDATE (conservative)
        let tracker = RegionIdTracker::new();
        let uri = test_uri("zero_at_start");

        // Create node at [40, 60)
        let ulid_original = tracker.get_or_create(&uri, 40, 60, "block");

        // Zero-length insert at node START: insert "abc" at position 40
        let old_text = text_with_markers(100);
        let mut new_text = old_text.clone();
        new_text.insert_str(40, "abc"); // Insert without deleting

        tracker.apply_text_change(&uri, &old_text, &new_text);

        // Conservative: invalidate because insert is AT START
        let ulid_after = tracker.get_or_create(&uri, 40, 63, "block");
        assert_ne!(
            ulid_original, ulid_after,
            "Zero-length insert at node START should invalidate (conservative)"
        );
    }

    #[test]
    fn test_zero_length_insert_before_node_keeps_ulid() {
        // Zero-length insert BEFORE node START → KEEP (shift)
        let tracker = RegionIdTracker::new();
        let uri = test_uri("zero_before");

        // Create node at [40, 60)
        let ulid_original = tracker.get_or_create(&uri, 40, 60, "block");

        // Zero-length insert before node: insert "abc" at position 30
        let old_text = text_with_markers(100);
        let mut new_text = old_text.clone();
        new_text.insert_str(30, "abc");

        tracker.apply_text_change(&uri, &old_text, &new_text);

        // Node shifts: [43, 63)
        let ulid_after = tracker.get(&uri, 43, 63, "block");
        assert_eq!(
            Some(ulid_original),
            ulid_after,
            "Zero-length insert before node START should keep ULID at shifted position [43, 63)"
        );

        // Verify old position no longer exists
        assert_eq!(
            tracker.get(&uri, 40, 60, "block"),
            None,
            "Old position [40, 60) should be removed"
        );
    }

    #[test]
    fn test_range_collapse_invalidates() {
        // Range collapse: Large delete causes end <= start → INVALIDATE
        let tracker = RegionIdTracker::new();
        let uri = test_uri("collapse");

        // Create node at [30, 50)
        let _ulid_original = tracker.get_or_create(&uri, 30, 50, "block");

        // Large delete that collapses the range: [20, 45) → delete 25 bytes
        let old_text = text_with_markers(100);
        let mut new_text = old_text.clone();
        new_text.replace_range(20..45, "");

        tracker.apply_text_change(&uri, &old_text, &new_text);

        // Node at [30, 50): START 30 is in [20, 45) → INVALIDATED
        // (Range collapse check wouldn't trigger because it's invalidated first)

        // Let's test actual range collapse: START not invalidated but range collapses
        // Create a node where START survives but END collapses
        let ulid2 = tracker.get_or_create(&uri, 10, 40, "block2");

        // Edit [20, 60): massive delete that would make end < start
        let old_text2 = text_with_markers(100);
        let mut new_text2 = old_text2.clone();
        new_text2.replace_range(20..60, "");

        tracker.apply_text_change(&uri, &old_text2, &new_text2);

        // Node at [10, 40): START 10 < 20 (not invalidated by START rule)
        // But: end 40 in (20, 60), adjusted to 40 + (20 - 60) = 0
        // Range becomes [10, 0) → collapsed → should be invalidated

        // Try to get it with any position - should be NEW ULID
        let ulid2_after = tracker.get_or_create(&uri, 10, 20, "block2");
        assert_ne!(
            ulid2, ulid2_after,
            "Node with collapsed range should be invalidated"
        );
    }

    // ============================================================
    // UTF-8 Multi-byte Tests
    // ============================================================
    // These tests verify that position calculations correctly handle
    // multi-byte UTF-8 characters (emoji, CJK, etc.)

    #[test]
    fn test_utf8_multibyte_edit_before_node_shifts_correctly() {
        // Test: Delete multi-byte characters before a node
        // Emoji 🦀 is 4 bytes, so deleting it should shift by 4, not 1
        let tracker = RegionIdTracker::new();
        let uri = test_uri("utf8_before");

        // Text: "abc🦀def" where 🦀 is at bytes [3, 7)
        // Node at bytes [7, 10) covering "def"
        let old_text = "abc🦀def";
        assert_eq!(old_text.len(), 10); // 3 + 4 + 3 = 10 bytes

        let ulid_original = tracker.get_or_create(&uri, 7, 10, "block");

        // Delete the emoji: "abc🦀def" → "abcdef"
        let new_text = "abcdef";
        assert_eq!(new_text.len(), 6);

        tracker.apply_text_change(&uri, old_text, new_text);

        // Node should shift from [7, 10) to [3, 6) (delta = -4 bytes)
        let ulid_after = tracker.get(&uri, 3, 6, "block");
        assert_eq!(
            Some(ulid_original),
            ulid_after,
            "Node should preserve ULID and shift by 4 bytes (emoji size)"
        );

        // Old position should be gone
        assert_eq!(
            tracker.get(&uri, 7, 10, "block"),
            None,
            "Old position [7, 10) should be removed"
        );
    }

    #[test]
    fn test_utf8_multibyte_edit_inside_node_adjusts_end() {
        // Test: Delete multi-byte characters inside a node
        let tracker = RegionIdTracker::new();
        let uri = test_uri("utf8_inside");

        // Text: "start日本語end" where:
        // - "start" at [0, 5)
        // - "日" at [5, 8) - 3 bytes
        // - "本" at [8, 11) - 3 bytes
        // - "語" at [11, 14) - 3 bytes
        // - "end" at [14, 17)
        let old_text = "start日本語end";
        assert_eq!(old_text.len(), 17); // 5 + 9 + 3 = 17 bytes

        // Node spans entire content [0, 17)
        let ulid_original = tracker.get_or_create(&uri, 0, 17, "block");

        // Delete "本" (bytes [8, 11)): "start日本語end" → "start日語end"
        let new_text = "start日語end";
        assert_eq!(new_text.len(), 14); // 17 - 3 = 14 bytes

        tracker.apply_text_change(&uri, old_text, new_text);

        // Node should adjust end from 17 to 14 (delta = -3)
        // START 0 is not in [8, 11), so preserved
        let ulid_after = tracker.get(&uri, 0, 14, "block");
        assert_eq!(
            Some(ulid_original),
            ulid_after,
            "Node should preserve ULID with adjusted end (delta = -3 for 3-byte char)"
        );
    }

    #[test]
    fn test_utf8_multibyte_node_start_inside_edit_invalidates() {
        // Test: Node START falls inside edit range containing multi-byte chars
        let tracker = RegionIdTracker::new();
        let uri = test_uri("utf8_start_inside");

        // Text: "前🎉後text" where:
        // - "前" at [0, 3) - 3 bytes
        // - "🎉" at [3, 7) - 4 bytes
        // - "後" at [7, 10) - 3 bytes
        // - "text" at [10, 14)
        let old_text = "前🎉後text";
        assert_eq!(old_text.len(), 14);

        // Node at [7, 14) covering "後text"
        let ulid_original = tracker.get_or_create(&uri, 7, 14, "block");

        // Delete "🎉後" (bytes [3, 10)): "前🎉後text" → "前text"
        let new_text = "前text";
        assert_eq!(new_text.len(), 7); // 3 + 4 = 7 bytes

        tracker.apply_text_change(&uri, old_text, new_text);

        // Node START 7 is in [3, 10) → INVALIDATED
        // Try to get at adjusted position - should be NEW ULID
        let ulid_after = tracker.get_or_create(&uri, 3, 7, "block");
        assert_ne!(
            ulid_original, ulid_after,
            "Node with START inside edit should be invalidated"
        );
    }

    #[test]
    fn test_utf8_insert_multibyte_shifts_correctly() {
        // Test: Insert multi-byte characters, verify shift
        let tracker = RegionIdTracker::new();
        let uri = test_uri("utf8_insert");

        // Text: "abcdef"
        let old_text = "abcdef";

        // Node at [3, 6) covering "def"
        let ulid_original = tracker.get_or_create(&uri, 3, 6, "block");

        // Insert emoji at position 2: "abcdef" → "ab🚀cdef"
        let new_text = "ab🚀cdef";
        assert_eq!(new_text.len(), 10); // 6 + 4 = 10 bytes

        tracker.apply_text_change(&uri, old_text, new_text);

        // Node should shift from [3, 6) to [7, 10) (delta = +4)
        let ulid_after = tracker.get(&uri, 7, 10, "block");
        assert_eq!(
            Some(ulid_original),
            ulid_after,
            "Node should shift by 4 bytes when 4-byte emoji inserted before it"
        );
    }

    #[test]
    fn test_utf8_mixed_ascii_and_multibyte() {
        // Test: Complex edit with mixed ASCII and multi-byte
        let tracker = RegionIdTracker::new();
        let uri = test_uri("utf8_mixed");

        // Text: "Hello世界World" where:
        // - "Hello" at [0, 5)
        // - "世" at [5, 8) - 3 bytes
        // - "界" at [8, 11) - 3 bytes
        // - "World" at [11, 16)
        let old_text = "Hello世界World";
        assert_eq!(old_text.len(), 16);

        // Multiple nodes
        let ulid_hello = tracker.get_or_create(&uri, 0, 5, "greeting");
        let _ulid_cjk = tracker.get_or_create(&uri, 5, 11, "cjk");
        let ulid_world = tracker.get_or_create(&uri, 11, 16, "world");

        // Replace "世界" with "🌍" (6 bytes → 4 bytes, delta = -2)
        let new_text = "Hello🌍World";
        assert_eq!(new_text.len(), 14); // 5 + 4 + 5 = 14

        tracker.apply_text_change(&uri, old_text, new_text);

        // "Hello" [0, 5): START 0 not in [5, 11) → KEEP unchanged
        assert_eq!(
            tracker.get(&uri, 0, 5, "greeting"),
            Some(ulid_hello),
            "Node before edit should be unchanged"
        );

        // "世界" [5, 11): START 5 is in [5, 11) → INVALIDATED
        assert_eq!(
            tracker.get(&uri, 5, 11, "cjk"),
            None,
            "Node with START inside edit should be invalidated"
        );

        // "World" [11, 16): START 11 >= 11 (edit.old_end) → KEEP and shift
        // New position: [11 + (-2), 16 + (-2)] = [9, 14)
        assert_eq!(
            tracker.get(&uri, 9, 14, "world"),
            Some(ulid_world),
            "Node after edit should shift by delta"
        );
    }
}
