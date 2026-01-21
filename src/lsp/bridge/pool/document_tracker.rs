//! Document tracking for downstream language servers.
//!
//! This module provides the DocumentTracker which manages virtual document state
//! for downstream language servers. It tracks:
//! - Document versions (for didChange notifications)
//! - Host-to-virtual mappings (for didClose propagation)
//! - Opened state (for LSP spec compliance - ADR-0015)

use std::collections::{HashMap, HashSet};

use log::warn;
use tokio::sync::Mutex;
use url::Url;

use super::OpenedVirtualDoc;
use crate::lsp::bridge::protocol::VirtualDocumentUri;

/// Tracks virtual document state for downstream language servers.
///
/// Manages three related concerns:
/// - Document versions (for didChange notifications)
/// - Host-to-virtual mappings (for didClose propagation)
/// - Opened state (for LSP spec compliance - ADR-0015)
///
/// # Lock Ordering Contract
///
/// When acquiring multiple locks, the order must be:
/// 1. `document_versions` first
/// 2. `host_to_virtual` second (while holding #1)
///
/// The `opened_documents` lock (std::sync::RwLock) can be acquired
/// independently of async locks for fast, synchronous read checks.
pub(crate) struct DocumentTracker {
    /// Map of language -> (virtual document URI -> version)
    document_versions: Mutex<HashMap<String, HashMap<String, i32>>>,
    /// Tracks which virtual documents were opened for each host document
    host_to_virtual: Mutex<HashMap<Url, Vec<OpenedVirtualDoc>>>,
    /// Tracks documents that have had didOpen ACTUALLY sent to downstream.
    /// Uses std::sync::RwLock for fast, synchronous read checks (ADR-0015).
    opened_documents: std::sync::RwLock<HashSet<String>>,
}

impl DocumentTracker {
    /// Create a new DocumentTracker with empty state.
    ///
    /// All tracking maps start empty. Documents are registered via
    /// `should_send_didopen()` and marked as opened via `mark_document_opened()`.
    pub(crate) fn new() -> Self {
        Self {
            document_versions: Mutex::new(HashMap::new()),
            host_to_virtual: Mutex::new(HashMap::new()),
            opened_documents: std::sync::RwLock::new(HashSet::new()),
        }
    }

    /// Check if document is opened and mark it as opened atomically.
    ///
    /// Returns true if the document was NOT previously opened (i.e., didOpen should be sent).
    /// Returns false if the document was already opened (i.e., skip didOpen).
    ///
    /// When returning true, also records the mapping from host_uri to the virtual document
    /// in host_to_virtual. This mapping is used for didClose propagation when the host
    /// document is closed.
    ///
    /// # Lock Ordering
    ///
    /// Acquires `document_versions` first, then `host_to_virtual` (only when inserting).
    /// This order must be consistent to prevent deadlocks.
    pub(super) async fn should_send_didopen(
        &self,
        host_uri: &Url,
        virtual_uri: &VirtualDocumentUri,
    ) -> bool {
        use std::collections::hash_map::Entry;

        let uri_string = virtual_uri.to_uri_string();
        let language = virtual_uri.language();

        let mut versions = self.document_versions.lock().await;
        let docs = versions.entry(language.to_string()).or_default();

        if let Entry::Vacant(e) = docs.entry(uri_string) {
            e.insert(1);

            // Record the host -> virtual mapping for didClose propagation
            let mut host_map = self.host_to_virtual.lock().await;
            host_map
                .entry(host_uri.clone())
                .or_default()
                .push(OpenedVirtualDoc {
                    virtual_uri: virtual_uri.clone(),
                });

            true
        } else {
            false
        }
    }

    /// Mark a document as having had didOpen sent to downstream (ADR-0015).
    ///
    /// This should be called AFTER the didOpen notification has been successfully
    /// written to the downstream server. Request handlers check `is_document_opened()`
    /// before sending requests to ensure LSP spec compliance.
    pub(super) fn mark_document_opened(&self, virtual_uri: &VirtualDocumentUri) {
        let uri_string = virtual_uri.to_uri_string();

        match self.opened_documents.write() {
            Ok(mut opened) => {
                opened.insert(uri_string);
            }
            Err(poisoned) => {
                warn!(
                    target: "kakehashi::lock_recovery",
                    "Recovered from poisoned opened_documents lock in mark_document_opened()"
                );
                poisoned.into_inner().insert(uri_string);
            }
        }
    }

    /// Check if a document has had didOpen ACTUALLY sent to downstream (ADR-0015).
    ///
    /// This is a fast, synchronous check used by request handlers to ensure
    /// they don't send requests before didOpen has been sent.
    ///
    /// Returns true if `mark_document_opened()` has been called for this document.
    /// Returns false if the document hasn't been opened yet.
    pub(crate) fn is_document_opened(&self, virtual_uri: &VirtualDocumentUri) -> bool {
        let uri_string = virtual_uri.to_uri_string();

        match self.opened_documents.read() {
            Ok(opened) => opened.contains(&uri_string),
            Err(poisoned) => {
                warn!(
                    target: "kakehashi::lock_recovery",
                    "Recovered from poisoned opened_documents lock in is_document_opened()"
                );
                poisoned.into_inner().contains(&uri_string)
            }
        }
    }

    /// Increment the version of a virtual document and return the new version.
    ///
    /// Returns None if the document has not been opened.
    pub(super) async fn increment_document_version(
        &self,
        virtual_uri: &VirtualDocumentUri,
    ) -> Option<i32> {
        let uri_string = virtual_uri.to_uri_string();
        let language = virtual_uri.language();

        let mut versions = self.document_versions.lock().await;
        if let Some(docs) = versions.get_mut(language)
            && let Some(version) = docs.get_mut(&uri_string)
        {
            *version += 1;
            return Some(*version);
        }
        None
    }

    /// Remove a document from all tracking state.
    ///
    /// Removes the document from:
    /// - `document_versions` (version tracking for didChange)
    /// - `opened_documents` (opened state for LSP compliance)
    ///
    /// Note: Does NOT remove from `host_to_virtual`. That cleanup is handled
    /// separately by `remove_host_virtual_docs()` or `remove_matching_virtual_docs()`,
    /// which are called before this method in the close flow.
    ///
    /// Used by did_close module for cleanup, and by Phase 3
    /// close_invalidated_virtual_docs for invalidated region cleanup.
    pub(crate) async fn untrack_document(&self, virtual_uri: &VirtualDocumentUri) {
        let uri_string = virtual_uri.to_uri_string();
        let language = virtual_uri.language();

        let mut versions = self.document_versions.lock().await;
        if let Some(docs) = versions.get_mut(language) {
            docs.remove(&uri_string);
        }

        match self.opened_documents.write() {
            Ok(mut opened) => {
                opened.remove(&uri_string);
            }
            Err(poisoned) => {
                warn!(
                    target: "kakehashi::lock_recovery",
                    "Recovered from poisoned opened_documents lock in untrack_document()"
                );
                poisoned.into_inner().remove(&uri_string);
            }
        }
    }

    /// Remove and return all virtual documents for a host URI.
    ///
    /// Used by did_close module for cleanup.
    pub(super) async fn remove_host_virtual_docs(&self, host_uri: &Url) -> Vec<OpenedVirtualDoc> {
        let mut host_map = self.host_to_virtual.lock().await;
        host_map.remove(host_uri).unwrap_or_default()
    }

    /// Take virtual documents matching the given ULIDs, removing them from tracking.
    ///
    /// This is atomic: lookup and removal happen in a single lock acquisition,
    /// preventing race conditions with concurrent didOpen requests.
    ///
    /// Returns the removed documents (for sending didClose). Documents that
    /// were never opened (not in host_to_virtual) are not returned.
    ///
    /// # Arguments
    /// * `host_uri` - The host document URI
    /// * `invalidated_ulids` - ULIDs to match against virtual document URIs
    pub(crate) async fn remove_matching_virtual_docs(
        &self,
        host_uri: &Url,
        invalidated_ulids: &[ulid::Ulid],
    ) -> Vec<OpenedVirtualDoc> {
        if invalidated_ulids.is_empty() {
            return Vec::new();
        }

        // Convert ULIDs to strings for matching
        let ulid_strs: std::collections::HashSet<String> =
            invalidated_ulids.iter().map(|u| u.to_string()).collect();

        let mut host_map = self.host_to_virtual.lock().await;
        let Some(docs) = host_map.get_mut(host_uri) else {
            return Vec::new();
        };

        // Partition: matching docs to return, non-matching to keep
        let mut to_close = Vec::new();
        docs.retain(|doc| {
            // Match region_id directly from VirtualDocumentUri
            let should_close = ulid_strs.contains(doc.virtual_uri.region_id());
            if should_close {
                to_close.push(doc.clone());
                false // Remove from host_to_virtual
            } else {
                true // Keep in host_to_virtual
            }
        });

        to_close
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::bridge::pool::test_helpers::*;

    // ========================================
    // should_send_didopen tests
    // ========================================

    /// Test that should_send_didopen records host to virtual mapping.
    ///
    /// When should_send_didopen returns true (meaning didOpen should be sent),
    /// it should also record the mapping from host URI to the opened virtual document.
    #[tokio::test]
    async fn should_send_didopen_records_host_to_virtual_mapping() {
        let tracker = DocumentTracker::new();
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let virtual_uri = VirtualDocumentUri::new(&url_to_uri(&host_uri), "lua", "lua-0");

        // First call should return true (document not opened yet)
        let result = tracker.should_send_didopen(&host_uri, &virtual_uri).await;
        assert!(result, "First call should return true");

        // Verify the host_to_virtual mapping was recorded
        let host_map = tracker.host_to_virtual.lock().await;
        let virtual_docs = host_map
            .get(&host_uri)
            .expect("host_uri should have entry in host_to_virtual");
        assert_eq!(virtual_docs.len(), 1);
        assert_eq!(virtual_docs[0].virtual_uri.language(), "lua");
        assert_eq!(virtual_docs[0].virtual_uri.region_id(), "lua-0");
    }

    /// Test that should_send_didopen records multiple virtual docs for same host.
    ///
    /// A markdown file may have multiple Lua code blocks, each creating a separate
    /// virtual document. All should be tracked under the same host URI.
    #[tokio::test]
    async fn should_send_didopen_records_multiple_virtual_docs_for_same_host() {
        let tracker = DocumentTracker::new();
        let host_uri = Url::parse("file:///project/doc.md").unwrap();

        // Open first Lua block
        let virtual_uri_0 = VirtualDocumentUri::new(&url_to_uri(&host_uri), "lua", "lua-0");
        let result = tracker.should_send_didopen(&host_uri, &virtual_uri_0).await;
        assert!(result, "First Lua block should return true");

        // Open second Lua block
        let virtual_uri_1 = VirtualDocumentUri::new(&url_to_uri(&host_uri), "lua", "lua-1");
        let result = tracker.should_send_didopen(&host_uri, &virtual_uri_1).await;
        assert!(result, "Second Lua block should return true");

        // Verify both are tracked under the same host
        let host_map = tracker.host_to_virtual.lock().await;
        let virtual_docs = host_map
            .get(&host_uri)
            .expect("host_uri should have entry in host_to_virtual");
        assert_eq!(virtual_docs.len(), 2);
        assert_eq!(virtual_docs[0].virtual_uri.region_id(), "lua-0");
        assert_eq!(virtual_docs[1].virtual_uri.region_id(), "lua-1");
    }

    /// Test that should_send_didopen does not duplicate mapping on second call.
    ///
    /// When should_send_didopen returns false (document already opened),
    /// it should NOT add a duplicate entry to host_to_virtual.
    #[tokio::test]
    async fn should_send_didopen_does_not_duplicate_mapping() {
        let tracker = DocumentTracker::new();
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let virtual_uri = VirtualDocumentUri::new(&url_to_uri(&host_uri), "lua", "lua-0");

        // First call - should return true and record mapping
        let result = tracker.should_send_didopen(&host_uri, &virtual_uri).await;
        assert!(result, "First call should return true");

        // Second call for same virtual doc - should return false
        let result = tracker.should_send_didopen(&host_uri, &virtual_uri).await;
        assert!(!result, "Second call should return false");

        // Verify only one entry exists (no duplicate)
        let host_map = tracker.host_to_virtual.lock().await;
        let virtual_docs = host_map
            .get(&host_uri)
            .expect("host_uri should have entry in host_to_virtual");
        assert_eq!(
            virtual_docs.len(),
            1,
            "Should only have one entry, not duplicates"
        );
    }

    /// Test that should_send_didopen does NOT mark document as opened.
    ///
    /// should_send_didopen only reserves the document version for tracking.
    /// The actual "opened" state should only be set by mark_document_opened
    /// which is called AFTER didOpen is sent to downstream.
    #[tokio::test]
    async fn should_send_didopen_does_not_mark_as_opened() {
        let tracker = DocumentTracker::new();
        let host_uri = Url::parse("file:///test/doc.md").unwrap();
        let virtual_uri = VirtualDocumentUri::new(&url_to_uri(&host_uri), "lua", TEST_ULID_LUA_0);

        // Call should_send_didopen - this reserves the version but doesn't mark as opened
        let should_open = tracker.should_send_didopen(&host_uri, &virtual_uri).await;
        assert!(should_open, "First call should return true");

        // is_document_opened should still return false
        assert!(
            !tracker.is_document_opened(&virtual_uri),
            "is_document_opened should return false even after should_send_didopen"
        );
    }

    // ========================================
    // is_document_opened tests
    // ========================================

    /// Test that is_document_opened returns false before mark_document_opened is called.
    ///
    /// This is part of the fix for LSP spec violation where requests were sent
    /// before didOpen. The is_document_opened() method checks whether didOpen
    /// has ACTUALLY been sent to the downstream server (not just marked for sending).
    #[tokio::test]
    async fn is_document_opened_returns_false_before_marked() {
        let tracker = DocumentTracker::new();
        let host_uri = Url::parse("file:///test/doc.md").unwrap();
        let virtual_uri = VirtualDocumentUri::new(&url_to_uri(&host_uri), "lua", TEST_ULID_LUA_0);

        // Before marking, should return false
        assert!(
            !tracker.is_document_opened(&virtual_uri),
            "is_document_opened should return false before mark_document_opened"
        );
    }

    /// Test that is_document_opened returns true after mark_document_opened is called.
    #[tokio::test]
    async fn is_document_opened_returns_true_after_marked() {
        let tracker = DocumentTracker::new();
        let host_uri = Url::parse("file:///test/doc.md").unwrap();
        let virtual_uri = VirtualDocumentUri::new(&url_to_uri(&host_uri), "lua", TEST_ULID_LUA_0);

        // Mark the document as opened
        tracker.mark_document_opened(&virtual_uri);

        // After marking, should return true
        assert!(
            tracker.is_document_opened(&virtual_uri),
            "is_document_opened should return true after mark_document_opened"
        );
    }

    // ========================================
    // increment_document_version tests
    // ========================================

    /// Test that increment_document_version returns None for unopened document.
    #[tokio::test]
    async fn increment_document_version_returns_none_for_unopened() {
        let tracker = DocumentTracker::new();
        let host_uri = Url::parse("file:///test/doc.md").unwrap();
        let virtual_uri = VirtualDocumentUri::new(&url_to_uri(&host_uri), "lua", TEST_ULID_LUA_0);

        // Document was never opened via should_send_didopen
        let version = tracker.increment_document_version(&virtual_uri).await;
        assert!(
            version.is_none(),
            "increment_document_version should return None for unopened document"
        );
    }

    /// Test that increment_document_version increments and returns new version.
    #[tokio::test]
    async fn increment_document_version_increments_after_open() {
        let tracker = DocumentTracker::new();
        let host_uri = Url::parse("file:///test/doc.md").unwrap();
        let virtual_uri = VirtualDocumentUri::new(&url_to_uri(&host_uri), "lua", TEST_ULID_LUA_0);

        // Open the document (sets version to 1)
        tracker.should_send_didopen(&host_uri, &virtual_uri).await;

        // First increment: 1 -> 2
        let version = tracker.increment_document_version(&virtual_uri).await;
        assert_eq!(version, Some(2), "First increment should return 2");

        // Second increment: 2 -> 3
        let version = tracker.increment_document_version(&virtual_uri).await;
        assert_eq!(version, Some(3), "Second increment should return 3");
    }

    // ========================================
    // untrack_document tests
    // ========================================

    /// Test that untrack_document removes from document_versions.
    #[tokio::test]
    async fn untrack_document_removes_from_versions() {
        let tracker = DocumentTracker::new();
        let host_uri = Url::parse("file:///test/doc.md").unwrap();
        let virtual_uri = VirtualDocumentUri::new(&url_to_uri(&host_uri), "lua", TEST_ULID_LUA_0);

        // Open the document
        tracker.should_send_didopen(&host_uri, &virtual_uri).await;

        // Verify version exists
        let version = tracker.increment_document_version(&virtual_uri).await;
        assert!(
            version.is_some(),
            "Document should have version before untrack"
        );

        // Untrack the document
        tracker.untrack_document(&virtual_uri).await;

        // Version should no longer exist
        let version = tracker.increment_document_version(&virtual_uri).await;
        assert!(
            version.is_none(),
            "Document should not have version after untrack"
        );
    }

    /// Test that untrack_document removes from opened_documents.
    #[tokio::test]
    async fn untrack_document_removes_from_opened() {
        let tracker = DocumentTracker::new();
        let host_uri = Url::parse("file:///test/doc.md").unwrap();
        let virtual_uri = VirtualDocumentUri::new(&url_to_uri(&host_uri), "lua", TEST_ULID_LUA_0);

        // Open and mark as opened
        tracker.should_send_didopen(&host_uri, &virtual_uri).await;
        tracker.mark_document_opened(&virtual_uri);
        assert!(
            tracker.is_document_opened(&virtual_uri),
            "Document should be opened before untrack"
        );

        // Untrack the document
        tracker.untrack_document(&virtual_uri).await;

        // Should no longer be marked as opened
        assert!(
            !tracker.is_document_opened(&virtual_uri),
            "Document should not be opened after untrack"
        );
    }

    /// Test that untrack_document does NOT remove from host_to_virtual.
    ///
    /// The host_to_virtual cleanup is handled separately by remove_host_virtual_docs
    /// or remove_matching_virtual_docs, which are called before untrack_document.
    #[tokio::test]
    async fn untrack_document_does_not_remove_from_host_to_virtual() {
        let tracker = DocumentTracker::new();
        let host_uri = Url::parse("file:///test/doc.md").unwrap();
        let virtual_uri = VirtualDocumentUri::new(&url_to_uri(&host_uri), "lua", TEST_ULID_LUA_0);

        // Open the document (adds to host_to_virtual)
        tracker.should_send_didopen(&host_uri, &virtual_uri).await;

        // Untrack the document
        tracker.untrack_document(&virtual_uri).await;

        // host_to_virtual should still have the entry
        let host_map = tracker.host_to_virtual.lock().await;
        let docs = host_map.get(&host_uri);
        assert!(
            docs.is_some() && !docs.unwrap().is_empty(),
            "untrack_document should NOT remove from host_to_virtual"
        );
    }

    // ========================================
    // remove_matching_virtual_docs tests
    // ========================================

    #[tokio::test]
    async fn remove_matching_virtual_docs_removes_matching_docs() {
        let tracker = DocumentTracker::new();
        let host_uri = test_host_uri("phase3_take");

        // Register some virtual docs using should_send_didopen
        let virtual_uri_1 = VirtualDocumentUri::new(&url_to_uri(&host_uri), "lua", TEST_ULID_LUA_0);
        let virtual_uri_2 =
            VirtualDocumentUri::new(&url_to_uri(&host_uri), "python", TEST_ULID_PYTHON_0);

        tracker.should_send_didopen(&host_uri, &virtual_uri_1).await;
        tracker.should_send_didopen(&host_uri, &virtual_uri_2).await;

        // Parse the ULIDs for matching
        let ulid_lua: ulid::Ulid = TEST_ULID_LUA_0.parse().unwrap();

        // Take only the Lua ULID
        let taken = tracker
            .remove_matching_virtual_docs(&host_uri, &[ulid_lua])
            .await;

        // Should return the Lua doc
        assert_eq!(taken.len(), 1, "Should take exactly one doc");
        assert_eq!(
            taken[0].virtual_uri.language(),
            "lua",
            "Should be the Lua doc"
        );
        assert_eq!(
            taken[0].virtual_uri.region_id(),
            TEST_ULID_LUA_0,
            "Should have the Lua ULID"
        );

        // Verify remaining docs in host_to_virtual
        let host_map = tracker.host_to_virtual.lock().await;
        let remaining = host_map.get(&host_uri).unwrap();
        assert_eq!(remaining.len(), 1, "Should have one remaining doc");
        assert_eq!(
            remaining[0].virtual_uri.language(),
            "python",
            "Python doc should remain"
        );
    }

    #[tokio::test]
    async fn remove_matching_virtual_docs_returns_empty_for_no_match() {
        let tracker = DocumentTracker::new();
        let host_uri = test_host_uri("phase3_no_match");

        // Register a virtual doc
        let virtual_uri = VirtualDocumentUri::new(&url_to_uri(&host_uri), "lua", TEST_ULID_LUA_0);
        tracker.should_send_didopen(&host_uri, &virtual_uri).await;

        // Try to take a different ULID
        let other_ulid: ulid::Ulid = TEST_ULID_LUA_1.parse().unwrap();
        let taken = tracker
            .remove_matching_virtual_docs(&host_uri, &[other_ulid])
            .await;

        assert!(taken.is_empty(), "Should return empty when no ULIDs match");

        // Original doc should still be there
        let host_map = tracker.host_to_virtual.lock().await;
        let remaining = host_map.get(&host_uri).unwrap();
        assert_eq!(remaining.len(), 1, "Original doc should remain");
    }

    #[tokio::test]
    async fn remove_matching_virtual_docs_returns_empty_for_unknown_host() {
        let tracker = DocumentTracker::new();
        let host_uri = test_host_uri("phase3_unknown_host");

        let ulid: ulid::Ulid = TEST_ULID_LUA_0.parse().unwrap();
        let taken = tracker
            .remove_matching_virtual_docs(&host_uri, &[ulid])
            .await;

        assert!(taken.is_empty(), "Should return empty for unknown host URI");
    }

    #[tokio::test]
    async fn remove_matching_virtual_docs_returns_empty_for_empty_ulids() {
        let tracker = DocumentTracker::new();
        let host_uri = test_host_uri("phase3_empty_ulids");

        // Register a virtual doc
        let virtual_uri = VirtualDocumentUri::new(&url_to_uri(&host_uri), "lua", TEST_ULID_LUA_0);
        tracker.should_send_didopen(&host_uri, &virtual_uri).await;

        // Take with empty ULID list (fast path)
        let taken = tracker.remove_matching_virtual_docs(&host_uri, &[]).await;

        assert!(taken.is_empty(), "Should return empty for empty ULID list");

        // Original doc should still be there
        let host_map = tracker.host_to_virtual.lock().await;
        let remaining = host_map.get(&host_uri).unwrap();
        assert_eq!(remaining.len(), 1, "Original doc should remain");
    }

    #[tokio::test]
    async fn remove_matching_virtual_docs_takes_multiple_docs() {
        let tracker = DocumentTracker::new();
        let host_uri = test_host_uri("phase3_multiple");

        // Register multiple virtual docs using VirtualDocumentUri for proper type safety
        let virtual_uri_1 = VirtualDocumentUri::new(&url_to_uri(&host_uri), "lua", TEST_ULID_LUA_0);
        let virtual_uri_2 = VirtualDocumentUri::new(&url_to_uri(&host_uri), "lua", TEST_ULID_LUA_1);
        let virtual_uri_3 =
            VirtualDocumentUri::new(&url_to_uri(&host_uri), "python", TEST_ULID_PYTHON_0);

        tracker.should_send_didopen(&host_uri, &virtual_uri_1).await;
        tracker.should_send_didopen(&host_uri, &virtual_uri_2).await;
        tracker.should_send_didopen(&host_uri, &virtual_uri_3).await;

        // Take both Lua ULIDs
        let ulid_1: ulid::Ulid = TEST_ULID_LUA_0.parse().unwrap();
        let ulid_2: ulid::Ulid = TEST_ULID_LUA_1.parse().unwrap();

        let taken = tracker
            .remove_matching_virtual_docs(&host_uri, &[ulid_1, ulid_2])
            .await;

        assert_eq!(taken.len(), 2, "Should take both Lua docs");

        // Verify Python doc remains
        let host_map = tracker.host_to_virtual.lock().await;
        let remaining = host_map.get(&host_uri).unwrap();
        assert_eq!(remaining.len(), 1, "Python doc should remain");
        assert_eq!(
            remaining[0].virtual_uri.language(),
            "python",
            "Remaining doc should be Python"
        );
    }
}
