//! Stable region ID tracking for injection regions.
//!
//! This module provides ULID-based identifiers for injection regions
//! that remain stable across document edits (Phase 2: position-based with START-priority).

use dashmap::DashMap;
use log::warn;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
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

impl PositionKey {
    /// Create a new position key from byte range and node kind.
    fn new(start: usize, end: usize, kind: &str) -> Self {
        Self {
            start_byte: start,
            end_byte: end,
            kind: kind.to_string(),
        }
    }

    /// Create a position key with adjusted positions (for edit operations).
    fn with_positions(start: usize, end: usize, kind: String) -> Self {
        Self {
            start_byte: start,
            end_byte: end,
            kind,
        }
    }
}

/// Edit position information for region ID tracking.
///
/// Represents byte positions of a text edit. Used to decouple
/// RegionIdTracker from tree_sitter::InputEdit.
///
/// Fields are intentionally private - use `new()` or `From<&InputEdit>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EditInfo {
    start_byte: usize,
    old_end_byte: usize,
    new_end_byte: usize,
}

impl EditInfo {
    /// Create a new EditInfo with byte positions.
    pub(crate) fn new(start_byte: usize, old_end_byte: usize, new_end_byte: usize) -> Self {
        Self {
            start_byte,
            old_end_byte,
            new_end_byte,
        }
    }
}

impl From<&tree_sitter::InputEdit> for EditInfo {
    fn from(edit: &tree_sitter::InputEdit) -> Self {
        Self::new(edit.start_byte, edit.old_end_byte, edit.new_end_byte)
    }
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

/// Adjust a position key based on an edit operation (ADR-0019 position adjustment).
///
/// Returns the adjusted PositionKey, or None if the range collapsed
/// (indicating the node should be invalidated).
///
/// # Position Cases (ADR-0019)
/// - **Node E**: AFTER edit (`start >= edit.old_end`) → shift both start and end
/// - **Node A/B**: CONTAINS/OVERLAPS edit (`end > edit.start`) → adjust end
///   - End inside edit range (`end <= edit.old_end`) → clamp to `edit.new_end_byte`
///   - End after edit range (`end > edit.old_end`) → apply delta
/// - **Node F**: BEFORE edit → unchanged
fn adjust_position_for_edit(key: PositionKey, edit: &EditInfo, delta: i64) -> Option<PositionKey> {
    if key.start_byte >= edit.old_end_byte {
        // Node E: AFTER edit → shift both start and end
        Some(PositionKey::with_positions(
            apply_delta(key.start_byte, delta),
            apply_delta(key.end_byte, delta),
            key.kind,
        ))
    } else if key.end_byte > edit.start_byte {
        // Node A/B: CONTAINS or OVERLAPS edit
        //
        // Two sub-cases for end position:
        // 1. End INSIDE edit range (absorbed): clamp to edit.new_end_byte
        // 2. End AFTER edit range: apply delta normally
        let new_end = if key.end_byte <= edit.old_end_byte {
            // End absorbed: clamp to where the edit ends in the new document
            // Example: Node [20, 30), Edit delete [25, 55) → Node becomes [20, 25)
            edit.new_end_byte
        } else {
            // End after edit: apply delta
            apply_delta(key.end_byte, delta)
        };

        // Guard: If range collapses (start >= end), return None to invalidate
        //
        // PRECONDITION: This branch requires node passed START-priority check,
        // meaning key.start_byte < edit.start_byte (otherwise node would be
        // invalidated before reaching here).
        //
        // Given this precondition, collapse is unreachable because:
        //   - For collapse: new_end <= key.start_byte
        //   - If end absorbed: new_end = edit.new_end_byte >= edit.start_byte > key.start_byte
        //   - If end after edit: new_end = key.end_byte + delta > key.start_byte
        //     (since key.end_byte > edit.start_byte > key.start_byte initially)
        //
        // Kept as defense-in-depth: protects against future refactoring that
        // might invalidate the precondition or introduce new branches.
        if new_end <= key.start_byte {
            None // Range collapsed to zero or negative
        } else {
            Some(PositionKey::with_positions(
                key.start_byte,
                new_end,
                key.kind,
            ))
        }
    } else {
        // Node F: BEFORE edit → unchanged
        Some(key)
    }
}

impl RegionIdTracker {
    /// Create a new empty tracker.
    pub(crate) fn new() -> Self {
        Self {
            entries: DashMap::new(),
        }
    }

    /// Get or create a stable ULID for an injection region.
    ///
    /// Phase 2: Uses position-based lookup (ADR-0019 composite key).
    /// Same (uri, start_byte, end_byte, kind) always returns the same ULID.
    pub(crate) fn get_or_create(&self, uri: &Url, start: usize, end: usize, kind: &str) -> Ulid {
        let key = PositionKey::new(start, end, kind);

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
        let key = PositionKey::new(start, end, kind);
        self.entries.get(uri)?.get(&key).copied()
    }

    /// Apply text change and update region positions using START-priority invalidation.
    ///
    /// This method reconstructs the edit operation from old and new text using
    /// character-level diff, then applies ADR-0019 invalidation rules.
    ///
    /// Returns ULIDs that were invalidated by this edit (for Phase 3 cleanup).
    /// The caller can use these to send didClose notifications for orphaned
    /// virtual documents.
    ///
    /// # Fast path
    /// If old_text == new_text, returns empty Vec without any processing.
    pub(crate) fn apply_text_change(&self, uri: &Url, old_text: &str, new_text: &str) -> Vec<Ulid> {
        // Fast path: identical texts need no processing
        if old_text == new_text {
            return Vec::new();
        }

        if let Some(edit) = Self::reconstruct_merged_edit(old_text, new_text) {
            self.apply_single_edit(uri, &edit)
        } else {
            Vec::new()
        }
    }

    /// Apply multiple edits individually for precise invalidation.
    ///
    /// Each edit is applied sequentially in array order. The tracker updates
    /// its internal node positions after each edit, so subsequent edits'
    /// running coordinates naturally align with the tracker's state.
    ///
    /// # Why No Coordinate Conversion?
    ///
    /// LSP sends edits in "running coordinates" - each edit's positions are
    /// relative to the document state after previous edits. The tracker's
    /// `apply_single_edit` updates internal node positions after each call,
    /// keeping them synchronized with the running coordinate space.
    ///
    /// # Edit Ordering
    ///
    /// **IMPORTANT**: LSP does NOT guarantee edits are in ascending position order.
    /// VSCode sends multi-cursor edits in reverse order (bottom-to-top).
    /// This method processes edits in **array order** as the LSP spec requires:
    /// > "Apply the TextDocumentContentChangeEvents in the order you receive them."
    ///
    /// # Arguments
    /// * `uri` - Document URI
    /// * `edits` - EditInfo slice in application order (as received from LSP)
    ///
    /// # Returns
    /// All ULIDs invalidated across all edits.
    pub(crate) fn apply_edits(&self, uri: &Url, edits: &[EditInfo]) -> Vec<Ulid> {
        let mut all_invalidated = Vec::new();

        for edit in edits {
            // Defensive: skip invalid edits (old_end < start is malformed)
            if edit.old_end_byte < edit.start_byte {
                warn!(
                    target: "treesitter_ls::region_tracker",
                    "Skipping invalid edit: old_end_byte ({}) < start_byte ({})",
                    edit.old_end_byte, edit.start_byte
                );
                continue;
            }

            // apply_single_edit updates tracker's internal positions
            // So next edit's running coords will match
            all_invalidated.extend(self.apply_single_edit(uri, edit));
        }

        all_invalidated
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
    ///
    /// Returns ULIDs that were invalidated by this edit (for Phase 3 cleanup).
    fn apply_single_edit(&self, uri: &Url, edit: &EditInfo) -> Vec<Ulid> {
        let delta = edit.delta();
        let mut invalidated = Vec::new();

        let Some(mut entries) = self.entries.get_mut(uri) else {
            return invalidated;
        };

        let mut new_entries = HashMap::new();

        for (key, ulid) in entries.drain() {
            if Self::should_invalidate_node(&key, edit) {
                invalidated.push(ulid);
                continue; // INVALIDATE
            }

            // Position adjustment (returns None if range collapsed)
            let Some(new_key) = adjust_position_for_edit(key, edit, delta) else {
                invalidated.push(ulid);
                continue; // INVALIDATE: range collapsed
            };

            // Handle potential position collision after adjustment
            // Two nodes may collapse to same (start, end, kind) after large edits
            //
            // Policy: "first wins" - whichever ULID is encountered first during
            // HashMap iteration is kept. Since HashMap order is non-deterministic,
            // the surviving ULID is arbitrary in collision cases.
            //
            // This is acceptable because:
            // 1. Collisions are rare (require extreme edits like massive deletions)
            // 2. Either ULID is equally valid for the resulting position
            // 3. Collisions may indicate a bug in invalidation logic anyway
            //
            // Log at warn level for observability and debugging.
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
                    // Note: Collided ULID is also invalidated (both nodes can't coexist)
                    invalidated.push(ulid);
                }
            }
        }

        *entries = new_entries;
        invalidated
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
        // Proper Node B case: Edit partially overlaps node's end region.
        // With end clamping, the end is clamped to edit.new_end_byte.
        //
        // Example: Node [20, 50), edit deletes [40, 60)
        // - START 20 is NOT in [40, 60) → KEEP
        // - END 50 is inside [40, 60) → clamp to edit.new_end_byte = 40
        // - New node: [20, 40) - valid range, ULID preserved
        let tracker = RegionIdTracker::new();
        let uri = test_uri("node_b");

        // Create node at [20, 50)
        let ulid_original = tracker.get_or_create(&uri, 20, 50, "block");

        // Edit overlaps end: [40, 60) → delete 20 bytes
        let old_text = text_with_markers(100);
        let mut new_text = old_text.clone();
        new_text.replace_range(40..60, "");

        tracker.apply_text_change(&uri, &old_text, &new_text);

        // After edit, node should:
        // - START unchanged (20 not in [40, 60))
        // - END clamped to edit.new_end_byte = 40 (since 50 is inside [40, 60))
        // - Range [20, 40) is valid (40 > 20), so ULID preserved
        let ulid_after = tracker.get(&uri, 20, 40, "block");
        assert_eq!(
            Some(ulid_original),
            ulid_after,
            "Node B should preserve ULID at clamped position [20, 40)"
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
    fn test_end_clamping_prevents_range_collapse() {
        // With end clamping, range collapse is prevented for Node A/B cases.
        // Previously large deletes could cause end <= start, but now end is
        // clamped to edit.new_end_byte, keeping a valid range.
        let tracker = RegionIdTracker::new();
        let uri = test_uri("collapse");

        // Create node at [30, 50)
        let _ulid_original = tracker.get_or_create(&uri, 30, 50, "block");

        // Large delete that includes node START: [20, 45) → delete 25 bytes
        let old_text = text_with_markers(100);
        let mut new_text = old_text.clone();
        new_text.replace_range(20..45, "");

        tracker.apply_text_change(&uri, &old_text, &new_text);

        // Node at [30, 50): START 30 is in [20, 45) → INVALIDATED by START-priority

        // Test end clamping: Node where START survives but END would collapse without clamping
        let ulid2 = tracker.get_or_create(&uri, 10, 40, "block2");

        // Edit [20, 60): massive delete
        // Without clamping: END 40 adjusted to 40 + (20 - 60) = 0 → collapse
        // With clamping: END 40 is inside [20, 60) → clamp to new_end = 20 → [10, 20)
        let old_text2 = text_with_markers(100);
        let mut new_text2 = old_text2.clone();
        new_text2.replace_range(20..60, "");

        tracker.apply_text_change(&uri, &old_text2, &new_text2);

        // Node at [10, 40): START 10 < 20 (not invalidated by START rule)
        // END 40 is inside [20, 60) → clamped to 20
        // New range: [10, 20) - valid, ULID preserved

        // ULID should be preserved at clamped position [10, 20)
        let ulid2_after = tracker.get(&uri, 10, 20, "block2");
        assert_eq!(
            Some(ulid2),
            ulid2_after,
            "End clamping should prevent range collapse, preserving ULID at [10, 20)"
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

    // ========================================
    // Phase 3 Tests: Invalidated ULID Return Value
    // ========================================

    #[test]
    fn test_apply_text_change_returns_invalidated_ulids() {
        // Phase 3: Verify apply_text_change returns the invalidated ULIDs
        let tracker = RegionIdTracker::new();
        let uri = test_uri("phase3_return");

        // Create node at [40, 60) - will be invalidated
        let ulid_invalidated = tracker.get_or_create(&uri, 40, 60, "block");

        // Edit overlaps start: [35, 45) → invalidates [40, 60)
        let old_text = text_with_markers(100);
        let mut new_text = old_text.clone();
        new_text.replace_range(35..45, "");

        let invalidated = tracker.apply_text_change(&uri, &old_text, &new_text);

        assert_eq!(
            invalidated.len(),
            1,
            "Should return exactly one invalidated ULID"
        );
        assert_eq!(
            invalidated[0], ulid_invalidated,
            "Returned ULID should match the invalidated node"
        );
    }

    #[test]
    fn test_apply_text_change_returns_multiple_invalidated_ulids() {
        // Phase 3: Multiple nodes invalidated by a single edit
        let tracker = RegionIdTracker::new();
        let uri = test_uri("phase3_multiple");

        // Create multiple nodes that will be invalidated by overlapping start
        let ulid_1 = tracker.get_or_create(&uri, 40, 50, "block1");
        let ulid_2 = tracker.get_or_create(&uri, 42, 55, "block2");
        let ulid_3 = tracker.get_or_create(&uri, 70, 80, "block3"); // Not invalidated

        // Edit [35, 50) invalidates nodes with START in [35, 50)
        // ulid_1 at [40, 50): START 40 in [35, 50) → invalidated
        // ulid_2 at [42, 55): START 42 in [35, 50) → invalidated
        // ulid_3 at [70, 80): START 70 not in [35, 50) → kept (shifted)
        let old_text = text_with_markers(100);
        let mut new_text = old_text.clone();
        new_text.replace_range(35..50, "xxxxx"); // Replace 15 chars with 5

        let invalidated = tracker.apply_text_change(&uri, &old_text, &new_text);

        assert_eq!(
            invalidated.len(),
            2,
            "Should return exactly two invalidated ULIDs"
        );
        assert!(
            invalidated.contains(&ulid_1),
            "Should contain first invalidated ULID"
        );
        assert!(
            invalidated.contains(&ulid_2),
            "Should contain second invalidated ULID"
        );
        assert!(
            !invalidated.contains(&ulid_3),
            "Should NOT contain kept ULID"
        );
    }

    #[test]
    fn test_apply_text_change_returns_empty_when_no_invalidation() {
        // Phase 3: Edit that doesn't invalidate any node
        let tracker = RegionIdTracker::new();
        let uri = test_uri("phase3_no_invalidation");

        // Create node at [50, 60)
        let _ulid = tracker.get_or_create(&uri, 50, 60, "block");

        // Edit at [10, 15) - before the node, doesn't overlap START
        let old_text = text_with_markers(100);
        let mut new_text = old_text.clone();
        new_text.replace_range(10..15, "xxx");

        let invalidated = tracker.apply_text_change(&uri, &old_text, &new_text);

        assert!(
            invalidated.is_empty(),
            "Should return empty when no nodes are invalidated"
        );
    }

    #[test]
    fn test_apply_text_change_returns_empty_for_identical_texts() {
        // Phase 3: Fast path when texts are identical
        let tracker = RegionIdTracker::new();
        let uri = test_uri("phase3_identical");

        let _ulid = tracker.get_or_create(&uri, 10, 20, "block");

        let text = text_with_markers(50);
        let invalidated = tracker.apply_text_change(&uri, &text, &text);

        assert!(
            invalidated.is_empty(),
            "Should return empty for identical texts (fast path)"
        );
    }

    #[test]
    fn test_apply_text_change_returns_empty_for_unknown_uri() {
        // Phase 3: Unknown URI returns empty (no entries to invalidate)
        let tracker = RegionIdTracker::new();
        let uri = test_uri("phase3_unknown");

        let old_text = text_with_markers(50);
        let mut new_text = old_text.clone();
        new_text.replace_range(10..20, "");

        let invalidated = tracker.apply_text_change(&uri, &old_text, &new_text);

        assert!(
            invalidated.is_empty(),
            "Should return empty for URI with no tracked entries"
        );
    }

    // ============================================================
    // Phase 4 Tests: apply_edits for Precise LSP Edit Processing
    // ============================================================
    // These tests verify the apply_edits method that processes
    // LSP InputEdits directly for precise invalidation.

    #[test]
    fn test_apply_edits_inside_node_keeps_with_adjusted_end() {
        // ADR-0019 Node A case: Edit INSIDE node → KEEP (adjust end)
        // Node [10, 20), Edit [15, 18) → [15, 25)
        // START 10 NOT in [15, 18) → KEEP
        let tracker = RegionIdTracker::new();
        let uri = test_uri("edit_inside");

        let ulid = tracker.get_or_create(&uri, 10, 20, "block");

        let edits = vec![EditInfo::new(15, 18, 25)]; // delta = +7

        let invalidated = tracker.apply_edits(&uri, &edits);

        // Should NOT be invalidated (Node A case)
        assert!(
            invalidated.is_empty(),
            "Edit inside node should KEEP it, not invalidate"
        );

        // Verify ULID at adjusted position [10, 27)
        assert_eq!(
            tracker.get(&uri, 10, 27, "block"),
            Some(ulid),
            "Node should be at adjusted position [10, 27)"
        );
    }

    #[test]
    fn test_apply_edits_at_node_start_invalidates() {
        // Edit starts at node's START → INVALIDATE
        // Node [20, 40), Edit [20, 25) → [20, 30)
        // START 20 in [20, 25) → INVALIDATE
        let tracker = RegionIdTracker::new();
        let uri = test_uri("edit_at_start");

        let ulid = tracker.get_or_create(&uri, 20, 40, "block");

        let edits = vec![EditInfo::new(20, 25, 30)];

        let invalidated = tracker.apply_edits(&uri, &edits);

        assert!(
            invalidated.contains(&ulid),
            "Edit at node START should invalidate"
        );
    }

    #[test]
    fn test_apply_edits_exact_match_invalidates() {
        // Delete exactly matching node range → INVALIDATE
        // Node [30, 50), Edit [30, 50) delete all
        // START 30 in [30, 50) → INVALIDATE
        let tracker = RegionIdTracker::new();
        let uri = test_uri("exact_match");

        let ulid = tracker.get_or_create(&uri, 30, 50, "block");

        let edits = vec![EditInfo::new(30, 50, 30)]; // delete 20 bytes

        let invalidated = tracker.apply_edits(&uri, &edits);

        assert!(
            invalidated.contains(&ulid),
            "Edit exactly matching node should invalidate"
        );
    }

    #[test]
    fn test_apply_edits_before_node_shifts() {
        // Edit BEFORE node → KEEP and shift
        // Node [50, 70), Edit [20, 30) delete 10 bytes
        // START 50 NOT in [20, 30) → KEEP, shift to [40, 60)
        let tracker = RegionIdTracker::new();
        let uri = test_uri("edit_before");

        let ulid = tracker.get_or_create(&uri, 50, 70, "block");

        let edits = vec![EditInfo::new(20, 30, 20)]; // delete 10 bytes, delta = -10

        let invalidated = tracker.apply_edits(&uri, &edits);

        assert!(
            invalidated.is_empty(),
            "Edit before node should not invalidate"
        );

        // Node should shift from [50, 70) to [40, 60)
        assert_eq!(
            tracker.get(&uri, 40, 60, "block"),
            Some(ulid),
            "Node should shift to [40, 60)"
        );
    }

    #[test]
    fn test_apply_edits_node_at_edit_old_end_shifts() {
        // Boundary: Node START exactly at edit.old_end → KEEP (shift)
        // Node [50, 70), Edit [30, 50) delete
        // START 50 NOT in [30, 50) because interval is [30, 50) exclusive
        let tracker = RegionIdTracker::new();
        let uri = test_uri("at_old_end");

        let ulid = tracker.get_or_create(&uri, 50, 70, "block");

        let edits = vec![EditInfo::new(30, 50, 30)]; // delete [30, 50), delta = -20

        let invalidated = tracker.apply_edits(&uri, &edits);

        assert!(
            invalidated.is_empty(),
            "Node at edit.old_end (exclusive) should not invalidate"
        );

        // Node shifts from [50, 70) to [30, 50)
        assert_eq!(
            tracker.get(&uri, 30, 50, "block"),
            Some(ulid),
            "Node should shift to [30, 50)"
        );
    }

    #[test]
    fn test_apply_edits_multiple_sequential() {
        // Two edits in sequence, both shifting nodes
        //
        // Initial state:
        //   Node 1 [10, 20), Node 2 [50, 60)
        //
        // Edit 1: insert 5 bytes at [5, 5)
        //   - Zero-length insert at 5
        //   - Node 1 START 10 ≠ 5 → KEEP, shift to [15, 25)
        //   - Node 2 START 50 ≠ 5 → KEEP, shift to [55, 65)
        //
        // Edit 2: delete [60, 63) in running coords (3 bytes, delta = -3)
        //   - Node 1 at [15, 25): START 15 NOT in [60, 63) → KEEP unchanged
        //   - Node 2 at [55, 65): START 55 NOT in [60, 63) → KEEP
        //     - END 65 > 60, adjust end: 65 + (-3) = 62
        //     - Final: [55, 62)
        let tracker = RegionIdTracker::new();
        let uri = test_uri("multi_edit");

        let ulid1 = tracker.get_or_create(&uri, 10, 20, "block1");
        let ulid2 = tracker.get_or_create(&uri, 50, 60, "block2");

        let edits = vec![
            EditInfo::new(5, 5, 10),   // insert 5 bytes at position 5
            EditInfo::new(60, 63, 60), // delete 3 bytes (running coords after first edit)
        ];

        let invalidated = tracker.apply_edits(&uri, &edits);

        // Neither should be invalidated (edits don't touch STARTs)
        assert!(
            !invalidated.contains(&ulid1),
            "Node 1 should not be invalidated"
        );
        assert!(
            !invalidated.contains(&ulid2),
            "Node 2 should not be invalidated"
        );

        // Verify final positions
        assert_eq!(
            tracker.get(&uri, 15, 25, "block1"),
            Some(ulid1),
            "Node 1 should be at [15, 25) after Edit 1"
        );
        assert_eq!(
            tracker.get(&uri, 55, 62, "block2"),
            Some(ulid2),
            "Node 2 should be at [55, 62) after both edits"
        );
    }

    #[test]
    fn test_apply_edits_zero_length_insert_at_start_invalidates() {
        // ADR-0019: Zero-length insert AT node START → INVALIDATE
        let tracker = RegionIdTracker::new();
        let uri = test_uri("zero_at_start");

        let ulid = tracker.get_or_create(&uri, 20, 40, "block");

        let edits = vec![EditInfo::new(20, 20, 25)]; // Zero-length insert 5 bytes

        let invalidated = tracker.apply_edits(&uri, &edits);

        assert!(
            invalidated.contains(&ulid),
            "Zero-length insert at node START should invalidate"
        );
    }

    #[test]
    fn test_apply_edits_zero_length_insert_before_node_shifts() {
        // Zero-length insert BEFORE node START → KEEP and shift
        let tracker = RegionIdTracker::new();
        let uri = test_uri("zero_before");

        let ulid = tracker.get_or_create(&uri, 30, 50, "block");

        let edits = vec![EditInfo::new(10, 10, 15)]; // Zero-length insert 5 bytes

        let invalidated = tracker.apply_edits(&uri, &edits);

        assert!(
            invalidated.is_empty(),
            "Zero-length insert before node should not invalidate"
        );

        // Verify node shifted from [30, 50) to [35, 55)
        assert_eq!(
            tracker.get(&uri, 35, 55, "block"),
            Some(ulid),
            "Node should shift to [35, 55)"
        );
    }

    #[test]
    fn test_apply_edits_empty_slice() {
        let tracker = RegionIdTracker::new();
        let uri = test_uri("empty");

        tracker.get_or_create(&uri, 10, 20, "block");

        let invalidated = tracker.apply_edits(&uri, &[]);

        assert!(
            invalidated.is_empty(),
            "Empty edits should return empty Vec"
        );
    }

    #[test]
    fn test_apply_edits_unknown_uri_returns_empty() {
        // Unknown URI should return empty Vec (no entries to invalidate)
        let tracker = RegionIdTracker::new();
        let uri = test_uri("unknown");

        let edits = vec![EditInfo::new(10, 20, 15)];
        let invalidated = tracker.apply_edits(&uri, &edits);

        assert!(
            invalidated.is_empty(),
            "Unknown URI should return empty Vec"
        );
    }

    #[test]
    fn test_apply_edits_single_edit_spans_multiple_nodes() {
        // Single edit that affects multiple nodes differently
        // Nodes: A [20, 30), B [40, 50), C [60, 70)
        // Edit: delete [35, 55) (delta = -20)
        //
        // START-priority analysis:
        //   - Node A [20, 30): END 30 < edit.start 35 → completely before edit, KEEP unchanged
        //   - Node B [40, 50): START 40 in [35, 55) → INVALIDATE
        //   - Node C [60, 70): START 60 >= old_end 55 → KEEP, shift by delta -20
        //
        // Final positions:
        //   - Node A: [20, 30) unchanged
        //   - Node B: invalidated
        //   - Node C: [60-20, 70-20) = [40, 50)
        let tracker = RegionIdTracker::new();
        let uri = test_uri("multi_node");

        let ulid_a = tracker.get_or_create(&uri, 20, 30, "blockA");
        let ulid_b = tracker.get_or_create(&uri, 40, 50, "blockB");
        let ulid_c = tracker.get_or_create(&uri, 60, 70, "blockC");

        let edits = vec![EditInfo::new(35, 55, 35)]; // delete [35, 55)

        let invalidated = tracker.apply_edits(&uri, &edits);

        // Only Node B should be invalidated (START in edit range)
        assert!(
            !invalidated.contains(&ulid_a),
            "Node A should NOT be invalidated"
        );
        assert!(
            invalidated.contains(&ulid_b),
            "Node B should be invalidated"
        );
        assert!(
            !invalidated.contains(&ulid_c),
            "Node C should NOT be invalidated"
        );

        // Verify final positions
        assert_eq!(
            tracker.get(&uri, 20, 30, "blockA"),
            Some(ulid_a),
            "Node A should remain at [20, 30) (before edit)"
        );
        assert_eq!(
            tracker.get(&uri, 40, 50, "blockC"),
            Some(ulid_c),
            "Node C should shift to [40, 50)"
        );
    }

    #[test]
    fn test_apply_edits_end_absorbed_keeps_node() {
        // Node's END is inside edit range → clamp end to edit.new_end_byte, KEEP node
        // This is the "end absorbed" case from ADR-0019
        //
        // Node [20, 30), Edit: delete [25, 55) → [25, 25)
        // - START 20 NOT in [25, 55) → KEEP (START-priority rule)
        // - END 30 is inside [25, 55) → end absorbed, clamp to edit.new_end_byte (25)
        // - Final: [20, 25)
        //
        // IMPORTANT: This requires adjust_position_for_edit to clamp end when:
        //   edit.start_byte < node.end_byte <= edit.old_end_byte
        // Instead of applying delta (which would cause range collapse).
        let tracker = RegionIdTracker::new();
        let uri = test_uri("end_absorbed");

        let ulid = tracker.get_or_create(&uri, 20, 30, "block");

        let edits = vec![EditInfo::new(25, 55, 25)]; // delete [25, 55)

        let invalidated = tracker.apply_edits(&uri, &edits);

        // Node should NOT be invalidated (START not in edit range)
        assert!(
            invalidated.is_empty(),
            "Node with absorbed end should be KEPT, not invalidated"
        );

        // End is clamped to edit.new_end_byte (25), so node becomes [20, 25)
        assert_eq!(
            tracker.get(&uri, 20, 25, "block"),
            Some(ulid),
            "Node should be at [20, 25) (end clamped to edit.new_end_byte)"
        );
    }

    #[test]
    fn test_apply_edits_larger_than_node_still_invalidates() {
        // Edit range larger than node → should still invalidate
        // Node [40, 50), Edit [30, 60) delete
        // START 40 in [30, 60) → INVALIDATE
        let tracker = RegionIdTracker::new();
        let uri = test_uri("larger_edit");

        let ulid = tracker.get_or_create(&uri, 40, 50, "block");

        let edits = vec![EditInfo::new(30, 60, 30)]; // delete larger range

        let invalidated = tracker.apply_edits(&uri, &edits);

        assert!(
            invalidated.contains(&ulid),
            "Edit larger than node should still invalidate"
        );
    }

    #[test]
    fn test_apply_edits_reverse_order_vscode_multicursor() {
        // VSCode sends multi-cursor edits in REVERSE order (bottom-to-top)
        // This test verifies we handle non-ascending edit order correctly
        //
        // Document: "line1\nline2\nline3\n" (bytes: 6, 12, 18)
        // User types "X" at start of line3 (byte 12) and line1 (byte 0)
        // VSCode sends: first line3 edit, then line1 edit (reverse position order)
        //
        // Nodes: A [0, 5), B [12, 17)
        //
        // Edit 1: insert at [12, 12) → [12, 13) (running coords)
        //   - Node A [0, 5): START 0 < 12, KEEP unchanged
        //   - Node B [12, 17): START 12 == 12 (zero-length at START) → INVALIDATE
        //
        // Edit 2: insert at [0, 0) → [0, 1) (running coords after Edit 1)
        //   - Node A: already invalidated? No, A wasn't invalidated
        //   - Node A [0, 5): START 0 == 0 (zero-length at START) → INVALIDATE
        let tracker = RegionIdTracker::new();
        let uri = test_uri("vscode_multicursor");

        let ulid_a = tracker.get_or_create(&uri, 0, 5, "line1");
        let ulid_b = tracker.get_or_create(&uri, 12, 17, "line3");

        // VSCode sends reverse order: line3 first, then line1
        let edits = vec![
            EditInfo::new(12, 12, 13), // Insert at line3 (later position first)
            EditInfo::new(0, 0, 1),    // Insert at line1 (earlier position second)
        ];

        let invalidated = tracker.apply_edits(&uri, &edits);

        // Both should be invalidated (zero-length insert at START)
        assert!(
            invalidated.contains(&ulid_a),
            "Node A should be invalidated (insert at START)"
        );
        assert!(
            invalidated.contains(&ulid_b),
            "Node B should be invalidated (insert at START)"
        );
    }

    #[test]
    fn test_apply_edits_reverse_order_preserves_positions() {
        // Strengthened VSCode reverse-order test: verify final positions are correct
        //
        // Scenario: Multiple nodes, reverse-order edits happen BEFORE them (not at START)
        // All nodes should be preserved with correctly shifted positions
        //
        // Document layout (20 bytes each line for simplicity):
        // Nodes: A [40, 50), B [80, 90), C [120, 130)
        //
        // VSCode sends edits in reverse order (bottom-to-top):
        // Edit 1: insert 5 bytes at position 100 (between B and C)
        // Edit 2: insert 3 bytes at position 60 (between A and B)
        // Edit 3: insert 2 bytes at position 20 (before A)
        //
        // Running coordinate analysis:
        // After Edit 1 (insert 5 at 100):
        //   A [40, 50) → [40, 50) unchanged (before edit)
        //   B [80, 90) → [80, 90) unchanged (before edit)
        //   C [120, 130) → [125, 135) shifted +5
        //
        // After Edit 2 (insert 3 at 60, in post-edit-1 coords):
        //   A [40, 50) → [40, 50) unchanged (before edit)
        //   B [80, 90) → [83, 93) shifted +3
        //   C [125, 135) → [128, 138) shifted +3
        //
        // After Edit 3 (insert 2 at 20, in post-edit-2 coords):
        //   A [40, 50) → [42, 52) shifted +2
        //   B [83, 93) → [85, 95) shifted +2
        //   C [128, 138) → [130, 140) shifted +2
        //
        // Final positions: A [42, 52), B [85, 95), C [130, 140)
        // Total shift: A +2, B +5, C +10 (cumulative from 3 edits)
        let tracker = RegionIdTracker::new();
        let uri = test_uri("vscode_reverse_positions");

        let ulid_a = tracker.get_or_create(&uri, 40, 50, "block");
        let ulid_b = tracker.get_or_create(&uri, 80, 90, "block");
        let ulid_c = tracker.get_or_create(&uri, 120, 130, "block");

        // VSCode reverse order: later positions first
        let edits = vec![
            EditInfo::new(100, 100, 105), // Insert 5 at 100 (between B and C)
            EditInfo::new(60, 60, 63),    // Insert 3 at 60 (between A and B)
            EditInfo::new(20, 20, 22),    // Insert 2 at 20 (before A)
        ];

        let invalidated = tracker.apply_edits(&uri, &edits);

        // No nodes should be invalidated (all edits before their START)
        assert!(
            invalidated.is_empty(),
            "No nodes should be invalidated: {:?}",
            invalidated
        );

        // Verify exact final positions
        assert_eq!(
            tracker.get(&uri, 42, 52, "block"),
            Some(ulid_a),
            "Node A should be at [42, 52) after +2 shift"
        );
        assert_eq!(
            tracker.get(&uri, 85, 95, "block"),
            Some(ulid_b),
            "Node B should be at [85, 95) after +5 cumulative shift"
        );
        assert_eq!(
            tracker.get(&uri, 130, 140, "block"),
            Some(ulid_c),
            "Node C should be at [130, 140) after +10 cumulative shift"
        );

        // Verify old positions are gone
        assert_eq!(
            tracker.get(&uri, 40, 50, "block"),
            None,
            "Old position A [40, 50) should not exist"
        );
        assert_eq!(
            tracker.get(&uri, 80, 90, "block"),
            None,
            "Old position B [80, 90) should not exist"
        );
        assert_eq!(
            tracker.get(&uri, 120, 130, "block"),
            None,
            "Old position C [120, 130) should not exist"
        );
    }

    #[test]
    fn test_apply_edits_invalid_edit_skipped() {
        // Invalid edit (old_end < start) should be skipped with warning
        let tracker = RegionIdTracker::new();
        let uri = test_uri("invalid_edit");

        let ulid = tracker.get_or_create(&uri, 30, 50, "block");

        let edits = vec![
            EditInfo::new(50, 40, 50), // INVALID: old_end_byte 40 < start_byte 50
            EditInfo::new(20, 25, 20), // Valid edit before node (shift)
        ];

        let invalidated = tracker.apply_edits(&uri, &edits);

        // Invalid edit skipped, valid edit processed
        assert!(
            invalidated.is_empty(),
            "No invalidation (invalid edit skipped, valid edit shifts)"
        );

        // Node should shift from [30, 50) to [25, 45) due to valid delete
        assert_eq!(
            tracker.get(&uri, 25, 45, "block"),
            Some(ulid),
            "Node should shift after valid edit (invalid edit skipped)"
        );
    }

    #[test]
    fn test_apply_edits_start_inside_edit_range_invalidates() {
        // Node whose START falls inside edit range → INVALIDATE by START-priority
        //
        // Node [10, 12), Edit: delete [5, 20) → [5, 5)
        // - START 10 IS in [5, 20) (5 <= 10 < 20) → INVALIDATE
        //
        // NOTE: This tests START-priority invalidation, not range collapse.
        // Range collapse via end clamping is theoretically unreachable because:
        //   - For collapse: edit.new_end_byte <= node.start
        //   - But if node.start < edit.start (required to pass START-priority),
        //     then edit.new_end_byte >= edit.start > node.start
        //   - Therefore collapse condition can never be satisfied
        // The range collapse check in adjust_position_for_edit is kept
        // as defense-in-depth against unexpected edge cases.
        let tracker = RegionIdTracker::new();
        let uri = test_uri("start_in_edit");

        let ulid = tracker.get_or_create(&uri, 10, 12, "block");

        let edits = vec![EditInfo::new(5, 20, 5)]; // delete [5, 20)

        let invalidated = tracker.apply_edits(&uri, &edits);

        assert!(
            invalidated.contains(&ulid),
            "Node with START inside edit range should be invalidated"
        );
    }

    #[test]
    fn test_apply_edits_zero_length_insert_after_node_keeps_unchanged() {
        // Zero-length insert AFTER node → KEEP unchanged
        let tracker = RegionIdTracker::new();
        let uri = test_uri("zero_after");

        let ulid = tracker.get_or_create(&uri, 20, 40, "block");

        let edits = vec![EditInfo::new(50, 50, 55)]; // Zero-length insert after node

        let invalidated = tracker.apply_edits(&uri, &edits);

        assert!(
            invalidated.is_empty(),
            "Zero-length insert after node should not invalidate"
        );

        // Verify node unchanged at [20, 40)
        assert_eq!(
            tracker.get(&uri, 20, 40, "block"),
            Some(ulid),
            "Node should remain at [20, 40)"
        );
    }

    // === Phase 4 Boundary Tests ===

    #[test]
    fn test_apply_edits_end_exactly_at_old_end_clamps() {
        // Boundary: Node END exactly equals edit.old_end_byte
        //
        // Node [20, 55), Edit: delete [25, 55) → [25, 25)
        // - START 20 NOT in [25, 55) → KEEP
        // - END 55 == old_end 55 → condition (end <= old_end) is TRUE → clamp to 25
        // - Final: [20, 25)
        let tracker = RegionIdTracker::new();
        let uri = test_uri("end_at_old_end");

        let ulid = tracker.get_or_create(&uri, 20, 55, "block");

        let edits = vec![EditInfo::new(25, 55, 25)]; // delete [25, 55)

        let invalidated = tracker.apply_edits(&uri, &edits);

        assert!(
            invalidated.is_empty(),
            "Node with end at old_end should be KEPT (clamped)"
        );

        assert_eq!(
            tracker.get(&uri, 20, 25, "block"),
            Some(ulid),
            "Node should be at [20, 25) (end clamped to new_end)"
        );
    }

    #[test]
    fn test_apply_edits_end_exactly_at_edit_start_unchanged() {
        // Boundary: Node END exactly equals edit.start_byte
        //
        // Node [20, 25), Edit: delete [25, 55) → [25, 25)
        // - Branch check: end > edit.start? → 25 > 25? NO
        // - Falls to else branch (Node F) → unchanged
        let tracker = RegionIdTracker::new();
        let uri = test_uri("end_at_start");

        let ulid = tracker.get_or_create(&uri, 20, 25, "block");

        let edits = vec![EditInfo::new(25, 55, 25)]; // delete [25, 55)

        let invalidated = tracker.apply_edits(&uri, &edits);

        assert!(
            invalidated.is_empty(),
            "Node ending at edit.start should be unchanged"
        );

        assert_eq!(
            tracker.get(&uri, 20, 25, "block"),
            Some(ulid),
            "Node should remain at [20, 25) (completely before edit)"
        );
    }

    #[test]
    fn test_apply_edits_zero_length_insert_inside_node_expands() {
        // Zero-length insert INSIDE node (not at boundaries)
        //
        // Node [20, 40), Edit: insert at [30, 30) → [30, 35) (5 bytes)
        // - START 20 != 30 → KEEP (not zero-length at START)
        // - END 40 > edit.start 30 → enters Node A/B branch
        // - END 40 <= old_end 30? NO (40 > 30) → apply delta +5
        // - Final: [20, 45)
        let tracker = RegionIdTracker::new();
        let uri = test_uri("insert_inside");

        let ulid = tracker.get_or_create(&uri, 20, 40, "block");

        let edits = vec![EditInfo::new(30, 30, 35)]; // insert 5 bytes at 30

        let invalidated = tracker.apply_edits(&uri, &edits);

        assert!(
            invalidated.is_empty(),
            "Insert inside node should KEEP it (expand)"
        );

        assert_eq!(
            tracker.get(&uri, 20, 45, "block"),
            Some(ulid),
            "Node should expand to [20, 45) (end shifted by +5)"
        );
    }

    // ============================================================
    // Phase 4 Concurrency Tests: apply_edits Thread-Safety
    // ============================================================
    // These tests verify thread-safety when multiple threads call
    // apply_edits() concurrently, mirroring the apply_text_change tests.

    // ============================================================
    // Phase 4 UTF-8 Multi-byte Tests: apply_edits with Unicode
    // ============================================================
    // These tests verify apply_edits handles byte positions correctly
    // for multi-byte UTF-8 characters. LSP provides byte offsets directly,
    // so we must ensure the START-priority logic works with any byte values.

    #[test]
    fn test_apply_edits_utf8_delete_emoji_before_node_shifts() {
        // Delete 4-byte emoji before node → shift by -4
        // Mirrors test_utf8_multibyte_edit_before_node_shifts_correctly
        let tracker = RegionIdTracker::new();
        let uri = test_uri("apply_edits_utf8_before");

        // Text: "abc🦀def" where 🦀 is at bytes [3, 7)
        // Node at bytes [7, 10) covering "def"
        let ulid = tracker.get_or_create(&uri, 7, 10, "block");

        // Edit: delete [3, 7) (the emoji), new_end = 3 (delta = -4)
        let edits = vec![EditInfo::new(3, 7, 3)];
        let invalidated = tracker.apply_edits(&uri, &edits);

        assert!(invalidated.is_empty(), "Node after edit should be kept");

        // Node shifts from [7, 10) to [3, 6)
        assert_eq!(
            tracker.get(&uri, 3, 6, "block"),
            Some(ulid),
            "Node should shift by 4 bytes (emoji size)"
        );
    }

    #[test]
    fn test_apply_edits_utf8_delete_inside_node_adjusts_end() {
        // Delete 3-byte character inside node → end shrinks
        // Node [0, 17), delete bytes [8, 11) → [0, 14)
        let tracker = RegionIdTracker::new();
        let uri = test_uri("apply_edits_utf8_inside");

        // Text: "start日本語end" (17 bytes)
        // Node covers entire text [0, 17)
        let ulid = tracker.get_or_create(&uri, 0, 17, "block");

        // Edit: delete "本" at [8, 11), new_end = 8 (delta = -3)
        let edits = vec![EditInfo::new(8, 11, 8)];
        let invalidated = tracker.apply_edits(&uri, &edits);

        assert!(invalidated.is_empty(), "Node START 0 not in [8, 11)");

        // End adjusts from 17 to 14
        assert_eq!(
            tracker.get(&uri, 0, 14, "block"),
            Some(ulid),
            "Node end should adjust by -3 (3-byte kanji)"
        );
    }

    #[test]
    fn test_apply_edits_utf8_start_inside_edit_invalidates() {
        // Node START inside edit range → INVALIDATE
        // Node [7, 14), edit [3, 10) → START 7 ∈ [3, 10) → INVALIDATE
        let tracker = RegionIdTracker::new();
        let uri = test_uri("apply_edits_utf8_invalidate");

        // Text: "前🎉後text" - node [7, 14) covers "後text"
        let ulid = tracker.get_or_create(&uri, 7, 14, "block");

        // Edit: delete [3, 10) (🎉後 = 4+3 = 7 bytes), new_end = 3
        let edits = vec![EditInfo::new(3, 10, 3)];
        let invalidated = tracker.apply_edits(&uri, &edits);

        assert_eq!(
            invalidated,
            vec![ulid],
            "Node with START 7 inside [3, 10) should be invalidated"
        );

        // Original position gone
        assert_eq!(tracker.get(&uri, 7, 14, "block"), None);
    }

    #[test]
    fn test_apply_edits_utf8_insert_emoji_shifts_node() {
        // Insert 4-byte emoji before node → shift by +4
        let tracker = RegionIdTracker::new();
        let uri = test_uri("apply_edits_utf8_insert");

        // Text: "abcdef", node [3, 6) covers "def"
        let ulid = tracker.get_or_create(&uri, 3, 6, "block");

        // Edit: insert 4 bytes at position 2 (ab|🚀|cdef)
        // [2, 2) → [2, 6) means insert 4 bytes at position 2
        let edits = vec![EditInfo::new(2, 2, 6)];
        let invalidated = tracker.apply_edits(&uri, &edits);

        assert!(invalidated.is_empty(), "Node after insert should be kept");

        // Node shifts from [3, 6) to [7, 10)
        assert_eq!(
            tracker.get(&uri, 7, 10, "block"),
            Some(ulid),
            "Node should shift by +4 bytes (emoji insertion)"
        );
    }

    #[test]
    fn test_apply_edits_utf8_mixed_operations() {
        // Multiple UTF-8 aware edits in sequence
        // Tests running coordinate updates with multi-byte deltas
        let tracker = RegionIdTracker::new();
        let uri = test_uri("apply_edits_utf8_mixed");

        // Three nodes: [0, 5), [10, 15), [20, 25)
        let ulid1 = tracker.get_or_create(&uri, 0, 5, "block");
        let ulid2 = tracker.get_or_create(&uri, 10, 15, "block");
        let ulid3 = tracker.get_or_create(&uri, 20, 25, "block");

        // Apply two edits in sequence (LSP order: as they were applied)
        // Edit 1: Insert 3-byte char at position 7 (between nodes 1 and 2)
        //         [7, 7) → [7, 10) delta +3
        // Edit 2: Delete 4-byte char at position 18 (between nodes 2 and 3, after first edit)
        //         [18, 22) → [18, 18) delta -4
        let edits = vec![EditInfo::new(7, 7, 10), EditInfo::new(18, 22, 18)];

        let invalidated = tracker.apply_edits(&uri, &edits);

        assert!(invalidated.is_empty(), "No nodes should be invalidated");

        // After edit 1 (delta +3):
        //   Node 1 [0, 5) - before edit, unchanged
        //   Node 2 [10, 15) → [13, 18) - after edit
        //   Node 3 [20, 25) → [23, 28) - after edit
        // After edit 2 (delta -4), applied to post-edit-1 positions:
        //   Node 1 [0, 5) - still unchanged (before both edits)
        //   Node 2 [13, 18) - before edit 2, unchanged
        //   Node 3 [23, 28) → [19, 24) - after edit 2 (edit at 18 < 23)
        assert_eq!(
            tracker.get(&uri, 0, 5, "block"),
            Some(ulid1),
            "Node 1 unchanged"
        );
        assert_eq!(
            tracker.get(&uri, 13, 18, "block"),
            Some(ulid2),
            "Node 2 shifted by +3 from first edit"
        );
        assert_eq!(
            tracker.get(&uri, 19, 24, "block"),
            Some(ulid3),
            "Node 3 shifted by +3 then -4 = -1 net"
        );
    }

    #[test]
    fn test_concurrent_apply_edits_same_uri_no_panic() {
        use std::sync::Arc;
        use std::thread;

        let tracker = Arc::new(RegionIdTracker::new());
        let uri = test_uri("concurrent_apply_edits");

        // Pre-populate with several nodes spread across the document
        for i in 0..10 {
            let start = i * 20;
            tracker.get_or_create(&uri, start, start + 15, "block");
        }

        // Spawn multiple threads that apply different edits concurrently
        let handles: Vec<_> = (0..5)
            .map(|thread_id| {
                let tracker = Arc::clone(&tracker);
                let uri = uri.clone();
                thread::spawn(move || {
                    // Each thread does multiple edit cycles
                    for cycle in 0..3 {
                        // Create edit at different positions per thread
                        let edit_start = (thread_id * 30 + cycle * 5) % 150;
                        let edit_end = edit_start + 5;

                        // Small deletion: [edit_start, edit_end) → [edit_start, edit_start)
                        let edits = vec![EditInfo::new(edit_start, edit_end, edit_start)];
                        tracker.apply_edits(&uri, &edits);
                    }
                })
            })
            .collect();

        // Wait for all threads to complete - no panics should occur
        for handle in handles {
            handle
                .join()
                .expect("Thread should not panic during concurrent apply_edits");
        }

        // Verify tracker is still functional after concurrent edits
        let new_ulid = tracker.get_or_create(&uri, 1000, 1010, "test");
        assert!(
            new_ulid.to_string().len() == 26,
            "ULID should be valid after concurrent apply_edits"
        );
    }

    #[test]
    fn test_concurrent_apply_edits_different_uris_independent() {
        use std::sync::Arc;
        use std::thread;

        let tracker = Arc::new(RegionIdTracker::new());

        // Each thread works on its own URI
        let handles: Vec<_> = (0..5)
            .map(|thread_id| {
                let tracker = Arc::clone(&tracker);
                let uri = test_uri(&format!("apply_edits_uri_{}", thread_id));

                thread::spawn(move || {
                    // Create initial node at [50, 100)
                    let ulid = tracker.get_or_create(&uri, 50, 100, "block");

                    // Apply an edit that shifts the node: delete [20, 30) before node
                    // This is a deletion of 10 bytes before the node (delta = -10)
                    let edits = vec![EditInfo::new(20, 30, 20)];
                    tracker.apply_edits(&uri, &edits);

                    // After edit, node should shift from [50, 100) to [40, 90)
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
                "Each URI should independently maintain correct position adjustment with apply_edits"
            );
        }
    }

    #[test]
    fn test_concurrent_get_and_apply_edits_interleaved() {
        use std::sync::Arc;
        use std::thread;

        let tracker = Arc::new(RegionIdTracker::new());
        let uri = test_uri("apply_edits_interleaved");

        // Pre-populate with one stable node that won't be affected by edits
        // Node at [0, 10) - edits will be at [50+)
        let stable_ulid = tracker.get_or_create(&uri, 0, 10, "stable");

        // Spawn threads that interleave get_or_create and apply_edits
        let handles: Vec<_> = (0..4)
            .map(|thread_id| {
                let tracker = Arc::clone(&tracker);
                let uri = uri.clone();
                let stable_ulid = stable_ulid;

                thread::spawn(move || {
                    let mut results = Vec::new();

                    for cycle in 0..5 {
                        // Create new node at a unique position per thread and cycle
                        let base = 50 + thread_id * 100 + cycle * 20;
                        let _ = tracker.get_or_create(&uri, base, base + 10, "dynamic");

                        // Apply edit that doesn't affect stable node at [0, 10)
                        if cycle % 2 == 0 {
                            let edit_start = 50 + thread_id * 100;
                            let edits = vec![EditInfo::new(edit_start, edit_start + 5, edit_start)];
                            tracker.apply_edits(&uri, &edits);
                        }

                        // Always verify stable node is accessible
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
                    "Stable node should have consistent ULID across concurrent apply_edits access"
                );
            }
        }
    }
}
