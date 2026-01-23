//! Cache coordination for semantic token operations.
//!
//! This module provides `CacheCoordinator` which unifies four cache structures
//! under a single coherent API with clear document lifecycle management:
//!
//! - `SemanticTokenCache` - Document-level token caching by URI with result_id validation
//! - `InjectionMap` - Tracks injection regions per document using interval trees
//! - `InjectionTokenCache` - Per-injection semantic tokens by (URI, region_id)
//! - `SemanticRequestTracker` - Cancellation support for in-flight requests
//!
//! ## Architecture
//!
//! ```text
//!                     CacheCoordinator
//!                           │
//!     ┌─────────────────────┼─────────────────────┐
//!     │         │           │          │          │
//!     ▼         ▼           ▼          ▼          ▼
//! Semantic   Injection   Injection   Request    Document
//! Cache      Map         TokenCache  Tracker    Lifecycle
//! ```
//!
//! The coordinator provides:
//! - Document lifecycle management (remove_document for did_close)
//! - Edit handling (invalidate_for_edits, invalidate_semantic for did_change)
//! - Injection map operations (populate, get, find_overlapping)
//! - Semantic token operations (get, store)
//! - Request tracking (start, is_active, finish, cancel)

use std::collections::{HashMap, HashSet};

use tower_lsp_server::ls_types::SemanticTokens;
use tree_sitter::{InputEdit, Tree};
use url::Url;

use crate::analysis::{InjectionMap, InjectionTokenCache, SemanticTokenCache, next_result_id};
use crate::language::LanguageCoordinator;
use crate::language::injection::{CacheableInjectionRegion, collect_all_injections};

use super::semantic_request_tracker::SemanticRequestTracker;

/// Request ID type for tracking semantic token requests.
pub(crate) type RequestId = u64;

/// Coordinates all cache structures for semantic token operations.
///
/// This struct wraps four underlying caches and provides a unified API
/// for document lifecycle management, edit handling, and token operations.
pub(crate) struct CacheCoordinator {
    semantic_cache: SemanticTokenCache,
    injection_map: InjectionMap,
    injection_token_cache: InjectionTokenCache,
    request_tracker: SemanticRequestTracker,
}

impl CacheCoordinator {
    /// Create a new cache coordinator with empty caches.
    pub(crate) fn new() -> Self {
        Self {
            semantic_cache: SemanticTokenCache::new(),
            injection_map: InjectionMap::new(),
            injection_token_cache: InjectionTokenCache::new(),
            request_tracker: SemanticRequestTracker::new(),
        }
    }

    // ========================================================================
    // Document lifecycle (did_close)
    // ========================================================================

    /// Remove all cached data for a document.
    ///
    /// Called when a document is closed to clean up:
    /// - Semantic token cache
    /// - Injection map
    /// - Injection token cache
    /// - Request tracking state
    pub(crate) fn remove_document(&self, uri: &Url) {
        self.semantic_cache.remove(uri);
        self.injection_map.clear(uri);
        self.injection_token_cache.clear_document(uri);
        self.request_tracker.cancel_all_for_uri(uri);
    }

    // ========================================================================
    // Edit handling (did_change)
    // ========================================================================

    /// Invalidate injection caches for regions that overlap with edits.
    ///
    /// Called BEFORE parse_document to use pre-edit byte offsets against pre-edit
    /// injection regions. This implements AC4/AC5 (PBI-083): edits outside injections
    /// preserve caches, edits inside invalidate only affected regions.
    ///
    /// PBI-167: Uses O(log n) interval tree query instead of O(n) iteration.
    pub(crate) fn invalidate_for_edits(&self, uri: &Url, edits: &[InputEdit]) {
        if edits.is_empty() {
            return;
        }

        // Find all regions that overlap with any edit using O(log n) queries
        for edit in edits {
            let edit_start = edit.start_byte;
            let edit_end = edit.old_end_byte;

            // Query interval tree for overlapping regions (O(log n) instead of O(n))
            if let Some(overlapping_regions) = self
                .injection_map
                .find_overlapping(uri, edit_start, edit_end)
            {
                for region in overlapping_regions {
                    // This region is affected - invalidate its cache
                    self.injection_token_cache.remove(uri, &region.region_id);
                    log::debug!(
                        target: "kakehashi::injection_cache",
                        "Invalidated injection cache for {} region (edit bytes {}..{})",
                        region.language,
                        edit_start,
                        edit_end
                    );
                }
            }
        }
    }

    /// Invalidate semantic token cache for a document.
    ///
    /// Called during did_change to ensure fresh tokens for delta calculations.
    pub(crate) fn invalidate_semantic(&self, uri: &Url) {
        self.semantic_cache.remove(uri);
    }

    /// Remove injection token cache entries for specific ULIDs.
    ///
    /// Called when region IDs are invalidated (e.g., due to edits touching their START).
    /// The corresponding virtual documents become orphaned in downstream LSs.
    pub(crate) fn remove_injection_tokens_for_ulids(&self, uri: &Url, ulids: &[ulid::Ulid]) {
        for ulid in ulids {
            self.injection_token_cache.remove(uri, &ulid.to_string());
        }
    }

    // ========================================================================
    // Injection map (post-parse)
    // ========================================================================

    /// Populate InjectionMap with injection regions from the parsed tree.
    ///
    /// This enables targeted cache invalidation (PBI-083): when an edit occurs,
    /// we can check which injection regions overlap and only invalidate those.
    ///
    /// AC6: Also clears stale InjectionTokenCache entries for removed regions.
    /// Since result_ids are regenerated on each parse, we clear the entire
    /// document's injection token cache and let it be repopulated on demand.
    pub(crate) fn populate_injections(
        &self,
        uri: &Url,
        text: &str,
        tree: &Tree,
        language_name: &str,
        language: &LanguageCoordinator,
    ) {
        // Get the injection query for this language
        let injection_query = match language.get_injection_query(language_name) {
            Some(q) => q,
            None => {
                // No injection query = no injections to track
                // Clear any stale injection caches
                self.injection_map.clear(uri);
                self.injection_token_cache.clear_document(uri);
                return;
            }
        };

        // Collect all injection regions from the parsed tree
        if let Some(regions) =
            collect_all_injections(&tree.root_node(), text, Some(injection_query.as_ref()))
        {
            if regions.is_empty() {
                // Clear any existing regions and caches for this document
                self.injection_map.clear(uri);
                self.injection_token_cache.clear_document(uri);
                return;
            }

            // Build map of existing regions by (language, content_hash) for stable ID matching
            // This enables cache reuse when document structure changes but injection content stays same
            let existing_regions = self.injection_map.get(uri);
            let existing_by_hash: HashMap<(&str, u64), &CacheableInjectionRegion> =
                existing_regions
                    .as_ref()
                    .map(|regions| {
                        regions
                            .iter()
                            .map(|r| ((r.language.as_str(), r.content_hash), r))
                            .collect()
                    })
                    .unwrap_or_default();

            // Convert to CacheableInjectionRegion, reusing region_ids for unchanged content
            let cacheable_regions: Vec<CacheableInjectionRegion> = regions
                .iter()
                .map(|info| {
                    // Compute hash for the new region's content
                    let temp_region = CacheableInjectionRegion::from_region_info(info, "", text);
                    let key = (info.language.as_str(), temp_region.content_hash);

                    // Check if we have an existing region with same (language, content_hash)
                    if let Some(existing) = existing_by_hash.get(&key) {
                        // Reuse the existing region_id - this enables cache hit!
                        CacheableInjectionRegion {
                            language: temp_region.language,
                            byte_range: temp_region.byte_range,
                            line_range: temp_region.line_range,
                            region_id: existing.region_id.clone(),
                            content_hash: temp_region.content_hash,
                        }
                    } else {
                        // New content - generate new region_id
                        CacheableInjectionRegion {
                            region_id: next_result_id(),
                            ..temp_region
                        }
                    }
                })
                .collect();

            // Find stale region IDs that are no longer present
            if let Some(old_regions) = existing_regions {
                let new_hashes: HashSet<_> = cacheable_regions
                    .iter()
                    .map(|r| (r.language.as_str(), r.content_hash))
                    .collect();
                for old in old_regions.iter() {
                    if !new_hashes.contains(&(old.language.as_str(), old.content_hash)) {
                        // This region no longer exists - clear its cache
                        self.injection_token_cache.remove(uri, &old.region_id);
                    }
                }
            }

            // Store in injection map
            self.injection_map.insert(uri.clone(), cacheable_regions);
        }
    }

    /// Get all injection regions for a document (test helper).
    #[cfg(test)]
    pub(crate) fn get_injections(&self, uri: &Url) -> Option<Vec<CacheableInjectionRegion>> {
        self.injection_map.get(uri)
    }

    // ========================================================================
    // Semantic tokens (semantic_tokens.rs)
    // ========================================================================

    /// Get cached semantic tokens if the result_id matches.
    ///
    /// Returns None if:
    /// - No tokens are cached for this URI
    /// - The cached result_id doesn't match the expected one
    pub(crate) fn get_tokens_if_valid(
        &self,
        uri: &Url,
        expected_result_id: &str,
    ) -> Option<SemanticTokens> {
        self.semantic_cache.get_if_valid(uri, expected_result_id)
    }

    /// Store semantic tokens for a document.
    pub(crate) fn store_tokens(&self, uri: Url, tokens: SemanticTokens) {
        self.semantic_cache.store(uri, tokens);
    }

    // ========================================================================
    // Request tracking (semantic_tokens.rs)
    // ========================================================================

    /// Start tracking a new request for the given URI.
    ///
    /// Returns a request ID that should be passed to subsequent operations.
    /// Automatically supersedes any previous request for the same URI.
    pub(crate) fn start_request(&self, uri: &Url) -> RequestId {
        self.request_tracker.start_request(uri)
    }

    /// Check if a request is still active (not superseded by a newer one).
    ///
    /// Returns true if the request should continue, false if it should abort.
    pub(crate) fn is_request_active(&self, uri: &Url, request_id: RequestId) -> bool {
        self.request_tracker.is_active(uri, request_id)
    }

    /// Finish a request, removing it from tracking if it's still the active one.
    pub(crate) fn finish_request(&self, uri: &Url, request_id: RequestId) {
        self.request_tracker.finish_request(uri, request_id);
    }

    /// Cancel all requests for a given URI (test helper).
    #[cfg(test)]
    pub(crate) fn cancel_requests(&self, uri: &Url) {
        self.request_tracker.cancel_all_for_uri(uri);
    }
}

impl Default for CacheCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp_server::ls_types::SemanticToken;

    fn create_test_uri(path: &str) -> Url {
        Url::parse(&format!("file:///{}", path)).unwrap()
    }

    #[test]
    fn test_remove_document_clears_all_caches() {
        let cache = CacheCoordinator::new();
        let uri = create_test_uri("test.rs");

        // Store some tokens
        let tokens = SemanticTokens {
            result_id: Some("test-id".to_string()),
            data: vec![SemanticToken {
                delta_line: 0,
                delta_start: 0,
                length: 5,
                token_type: 0,
                token_modifiers_bitset: 0,
            }],
        };
        cache.store_tokens(uri.clone(), tokens);

        // Start a request
        let _request_id = cache.start_request(&uri);

        // Remove the document
        cache.remove_document(&uri);

        // Verify all caches are cleared
        assert!(cache.get_tokens_if_valid(&uri, "test-id").is_none());
        assert!(cache.get_injections(&uri).is_none());
    }

    #[test]
    fn test_invalidate_semantic() {
        let cache = CacheCoordinator::new();
        let uri = create_test_uri("test.rs");

        // Store tokens
        let tokens = SemanticTokens {
            result_id: Some("test-id".to_string()),
            data: vec![],
        };
        cache.store_tokens(uri.clone(), tokens);

        // Verify stored
        assert!(cache.get_tokens_if_valid(&uri, "test-id").is_some());

        // Invalidate
        cache.invalidate_semantic(&uri);

        // Verify removed
        assert!(cache.get_tokens_if_valid(&uri, "test-id").is_none());
    }

    #[test]
    fn test_request_tracking() {
        let cache = CacheCoordinator::new();
        let uri = create_test_uri("test.rs");

        // Start a request
        let req1 = cache.start_request(&uri);
        assert!(cache.is_request_active(&uri, req1));

        // Start another request - should supersede the first
        let req2 = cache.start_request(&uri);
        assert!(!cache.is_request_active(&uri, req1));
        assert!(cache.is_request_active(&uri, req2));

        // Finish the second request
        cache.finish_request(&uri, req2);
        assert!(!cache.is_request_active(&uri, req2));
    }

    #[test]
    fn test_cancel_requests() {
        let cache = CacheCoordinator::new();
        let uri = create_test_uri("test.rs");

        // Start a request
        let req = cache.start_request(&uri);
        assert!(cache.is_request_active(&uri, req));

        // Cancel all requests
        cache.cancel_requests(&uri);
        assert!(!cache.is_request_active(&uri, req));
    }
}
