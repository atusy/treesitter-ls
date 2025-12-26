//! Semantic token caching with result_id validation and injection region tracking.
//!
//! This module provides three caching layers for semantic token performance:
//!
//! 1. **SemanticTokenCache** - Document-level token caching by URI with result_id validation.
//!    Used for cache hits when the document version matches.
//!
//! 2. **InjectionMap** - Tracks all injection regions per document URI.
//!    Each `CacheableInjectionRegion` stores language, byte/line ranges, and a result_id.
//!    Enables targeted invalidation: when an edit occurs, only regions overlapping
//!    the edit need re-tokenization (see PBI-083).
//!
//! 3. **InjectionTokenCache** - Per-injection token caching by (URI, region_id).
//!    Stores tokens for individual code blocks, allowing cache reuse when
//!    edits occur outside that injection region.
//!
//! ## Architecture
//!
//! ```text
//! Document edit arrives
//!        |
//!        v
//! InjectionMap.get(uri) -> Vec<CacheableInjectionRegion>
//!        |
//!        v
//! Find regions overlapping edit (byte_range intersection)
//!        |
//!   +----+----+
//!   |         |
//!   v         v
//! Overlapping:     Unchanged:
//! Invalidate &     Reuse tokens from
//! re-tokenize      InjectionTokenCache
//! ```
//!
//! All caches use DashMap for thread-safe concurrent access.

use crate::language::injection::CacheableInjectionRegion;
use dashmap::DashMap;
use tower_lsp::lsp_types::SemanticTokens;
use url::Url;

/// Thread-safe semantic token cache.
pub struct SemanticTokenCache {
    cache: DashMap<Url, SemanticTokens>,
}

impl SemanticTokenCache {
    /// Create a new empty cache.
    pub fn new() -> Self {
        Self {
            cache: DashMap::new(),
        }
    }

    /// Store semantic tokens for a document.
    pub fn store(&self, uri: Url, tokens: SemanticTokens) {
        self.cache.insert(uri, tokens);
    }

    /// Retrieve semantic tokens for a document.
    pub fn get(&self, uri: &Url) -> Option<SemanticTokens> {
        self.cache.get(uri).map(|entry| entry.clone())
    }

    /// Get cached tokens if the result_id matches.
    ///
    /// Returns None if:
    /// - No tokens are cached for this URI
    /// - The cached result_id doesn't match the expected one
    pub fn get_if_valid(&self, uri: &Url, expected_result_id: &str) -> Option<SemanticTokens> {
        self.cache.get(uri).and_then(|entry| {
            if entry.result_id.as_deref() == Some(expected_result_id) {
                Some(entry.clone())
            } else {
                None
            }
        })
    }

    /// Remove cached tokens for a document (e.g., on document close).
    pub fn remove(&self, uri: &Url) {
        self.cache.remove(uri);
    }
}

impl Default for SemanticTokenCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Thread-safe map of injection regions per document.
///
/// Tracks all `CacheableInjectionRegion`s for each document URI,
/// enabling targeted cache invalidation when only specific injections change.
pub struct InjectionMap {
    regions: DashMap<Url, Vec<CacheableInjectionRegion>>,
}

impl InjectionMap {
    /// Create a new empty injection map.
    pub fn new() -> Self {
        Self {
            regions: DashMap::new(),
        }
    }

    /// Store injection regions for a document, replacing any existing regions.
    pub fn insert(&self, uri: Url, regions: Vec<CacheableInjectionRegion>) {
        self.regions.insert(uri, regions);
    }

    /// Retrieve injection regions for a document.
    pub fn get(&self, uri: &Url) -> Option<Vec<CacheableInjectionRegion>> {
        self.regions.get(uri).map(|entry| entry.clone())
    }

    /// Remove all injection regions for a document (e.g., on document close).
    pub fn clear(&self, uri: &Url) {
        self.regions.remove(uri);
    }
}

impl Default for InjectionMap {
    fn default() -> Self {
        Self::new()
    }
}

/// Thread-safe cache for per-injection semantic tokens.
///
/// Unlike `SemanticTokenCache` which stores tokens per document, this cache
/// stores tokens keyed by (uri, region_id), enabling injection-level caching.
pub struct InjectionTokenCache {
    cache: DashMap<(Url, String), SemanticTokens>,
}

impl InjectionTokenCache {
    /// Create a new empty cache.
    pub fn new() -> Self {
        Self {
            cache: DashMap::new(),
        }
    }

    /// Store semantic tokens for a specific injection region.
    pub fn store(&self, uri: &Url, region_id: &str, tokens: SemanticTokens) {
        self.cache
            .insert((uri.clone(), region_id.to_string()), tokens);
    }

    /// Retrieve semantic tokens for a specific injection region.
    ///
    /// Returns `Some(tokens)` on cache hit, `None` on cache miss.
    /// Cache hits are logged at debug level to help verify stable region ID optimization.
    pub fn get(&self, uri: &Url, region_id: &str) -> Option<SemanticTokens> {
        let result = self
            .cache
            .get(&(uri.clone(), region_id.to_string()))
            .map(|entry| entry.clone());

        if result.is_some() {
            log::debug!(
                target: "treesitter_ls::injection_cache",
                "Cache HIT for injection region '{}' in {}",
                region_id,
                uri.path()
            );
        } else {
            log::trace!(
                target: "treesitter_ls::injection_cache",
                "Cache MISS for injection region '{}' in {}",
                region_id,
                uri.path()
            );
        }

        result
    }

    /// Remove cached tokens for a specific injection region.
    pub fn remove(&self, uri: &Url, region_id: &str) {
        self.cache.remove(&(uri.clone(), region_id.to_string()));
    }

    /// Remove all cached tokens for a document (all its injection regions).
    pub fn clear_document(&self, uri: &Url) {
        self.cache.retain(|key, _| &key.0 != uri);
    }
}

impl Default for InjectionTokenCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::SemanticToken;

    #[test]
    fn test_semantic_cache_store_retrieve() {
        let cache = SemanticTokenCache::new();
        let uri = Url::parse("file:///test.rs").unwrap();
        let tokens = SemanticTokens {
            result_id: Some("1".to_string()),
            data: vec![SemanticToken {
                delta_line: 0,
                delta_start: 0,
                length: 5,
                token_type: 0,
                token_modifiers_bitset: 0,
            }],
        };

        // Store tokens
        cache.store(uri.clone(), tokens.clone());

        // Retrieve tokens
        let retrieved = cache.get(&uri);
        assert!(retrieved.is_some(), "Should retrieve stored tokens");
        let retrieved = retrieved.unwrap();
        assert_eq!(
            retrieved.result_id,
            Some("1".to_string()),
            "result_id should match"
        );
        assert_eq!(retrieved.data.len(), 1, "Should have 1 token");
        assert_eq!(retrieved.data[0].length, 5, "Token length should match");

        // Non-existent URI returns None
        let other_uri = Url::parse("file:///other.rs").unwrap();
        assert!(
            cache.get(&other_uri).is_none(),
            "Non-existent URI should return None"
        );
    }

    #[test]
    fn test_semantic_cache_invalid_result_id() {
        let cache = SemanticTokenCache::new();
        let uri = Url::parse("file:///test.rs").unwrap();
        let tokens = SemanticTokens {
            result_id: Some("42".to_string()),
            data: vec![SemanticToken {
                delta_line: 0,
                delta_start: 0,
                length: 10,
                token_type: 1,
                token_modifiers_bitset: 0,
            }],
        };

        cache.store(uri.clone(), tokens);

        // Matching result_id returns tokens
        let valid = cache.get_if_valid(&uri, "42");
        assert!(
            valid.is_some(),
            "Should return tokens when result_id matches"
        );
        assert_eq!(valid.unwrap().data[0].length, 10);

        // Mismatched result_id returns None
        let invalid = cache.get_if_valid(&uri, "99");
        assert!(
            invalid.is_none(),
            "Should return None when result_id doesn't match"
        );

        // Non-existent URI returns None
        let other_uri = Url::parse("file:///other.rs").unwrap();
        assert!(
            cache.get_if_valid(&other_uri, "42").is_none(),
            "Non-existent URI should return None"
        );
    }

    #[test]
    fn test_semantic_cache_remove_on_close() {
        let cache = SemanticTokenCache::new();
        let uri = Url::parse("file:///test.rs").unwrap();
        let tokens = SemanticTokens {
            result_id: Some("1".to_string()),
            data: vec![SemanticToken {
                delta_line: 0,
                delta_start: 0,
                length: 5,
                token_type: 0,
                token_modifiers_bitset: 0,
            }],
        };

        // Store tokens
        cache.store(uri.clone(), tokens);
        assert!(cache.get(&uri).is_some(), "Should have cached tokens");

        // Remove on close
        cache.remove(&uri);
        assert!(cache.get(&uri).is_none(), "Should return None after remove");

        // Removing non-existent URI is safe
        let other_uri = Url::parse("file:///other.rs").unwrap();
        cache.remove(&other_uri); // Should not panic
    }

    #[test]
    fn test_injection_map_store_retrieve() {
        use crate::language::injection::CacheableInjectionRegion;

        let map = InjectionMap::new();
        let uri = Url::parse("file:///test.md").unwrap();

        let regions = vec![
            CacheableInjectionRegion {
                language: "lua".to_string(),
                byte_range: 10..50,
                line_range: 2..5,
                result_id: "region-1".to_string(),
                content_hash: 12345,
            },
            CacheableInjectionRegion {
                language: "python".to_string(),
                byte_range: 100..200,
                line_range: 10..20,
                result_id: "region-2".to_string(),
                content_hash: 67890,
            },
        ];

        // Insert regions
        map.insert(uri.clone(), regions.clone());

        // Retrieve regions
        let retrieved = map.get(&uri);
        assert!(retrieved.is_some(), "Should retrieve stored regions");
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.len(), 2, "Should have 2 regions");
        assert_eq!(retrieved[0].language, "lua");
        assert_eq!(retrieved[1].language, "python");

        // Non-existent URI returns None
        let other_uri = Url::parse("file:///other.md").unwrap();
        assert!(
            map.get(&other_uri).is_none(),
            "Non-existent URI should return None"
        );
    }

    #[test]
    fn test_injection_map_clear() {
        use crate::language::injection::CacheableInjectionRegion;

        let map = InjectionMap::new();
        let uri = Url::parse("file:///test.md").unwrap();

        let regions = vec![CacheableInjectionRegion {
            language: "lua".to_string(),
            byte_range: 10..50,
            line_range: 2..5,
            result_id: "region-1".to_string(),
            content_hash: 12345,
        }];

        // Insert and verify
        map.insert(uri.clone(), regions);
        assert!(map.get(&uri).is_some(), "Should have stored regions");

        // Clear removes regions
        map.clear(&uri);
        assert!(map.get(&uri).is_none(), "Should return None after clear");

        // Clearing non-existent URI is safe
        let other_uri = Url::parse("file:///other.md").unwrap();
        map.clear(&other_uri); // Should not panic
    }

    #[test]
    fn test_injection_token_cache_store_retrieve() {
        let cache = InjectionTokenCache::new();
        let uri = Url::parse("file:///test.md").unwrap();

        let tokens1 = SemanticTokens {
            result_id: Some("lua-region-1".to_string()),
            data: vec![SemanticToken {
                delta_line: 0,
                delta_start: 0,
                length: 5,
                token_type: 0,
                token_modifiers_bitset: 0,
            }],
        };

        let tokens2 = SemanticTokens {
            result_id: Some("python-region-2".to_string()),
            data: vec![SemanticToken {
                delta_line: 1,
                delta_start: 2,
                length: 10,
                token_type: 1,
                token_modifiers_bitset: 0,
            }],
        };

        // Store tokens for different regions in same document
        cache.store(&uri, "region-1", tokens1.clone());
        cache.store(&uri, "region-2", tokens2.clone());

        // Retrieve by (uri, region_id)
        let retrieved1 = cache.get(&uri, "region-1");
        assert!(retrieved1.is_some(), "Should retrieve tokens for region-1");
        assert_eq!(retrieved1.unwrap().data[0].length, 5);

        let retrieved2 = cache.get(&uri, "region-2");
        assert!(retrieved2.is_some(), "Should retrieve tokens for region-2");
        assert_eq!(retrieved2.unwrap().data[0].length, 10);

        // Non-existent region returns None
        assert!(cache.get(&uri, "region-3").is_none());

        // Non-existent URI returns None
        let other_uri = Url::parse("file:///other.md").unwrap();
        assert!(cache.get(&other_uri, "region-1").is_none());
    }

    #[test]
    fn test_injection_map_get_tokens_via_result_id() {
        use crate::language::injection::CacheableInjectionRegion;

        let injection_map = InjectionMap::new();
        let token_cache = InjectionTokenCache::new();
        let uri = Url::parse("file:///test.md").unwrap();

        // Set up injection regions
        let regions = vec![
            CacheableInjectionRegion {
                language: "lua".to_string(),
                byte_range: 10..50,
                line_range: 2..5,
                result_id: "lua-region-1".to_string(),
                content_hash: 11111,
            },
            CacheableInjectionRegion {
                language: "python".to_string(),
                byte_range: 100..200,
                line_range: 10..20,
                result_id: "python-region-2".to_string(),
                content_hash: 22222,
            },
        ];
        injection_map.insert(uri.clone(), regions);

        // Set up cached tokens for each region
        let lua_tokens = SemanticTokens {
            result_id: Some("lua-tokens".to_string()),
            data: vec![SemanticToken {
                delta_line: 0,
                delta_start: 0,
                length: 3,
                token_type: 0,
                token_modifiers_bitset: 0,
            }],
        };
        token_cache.store(&uri, "lua-region-1", lua_tokens);

        // Find region containing byte offset and get its cached tokens
        let regions = injection_map.get(&uri).unwrap();
        let region_at_byte_30 = regions.iter().find(|r| r.byte_range.contains(&30));
        assert!(region_at_byte_30.is_some(), "Should find region at byte 30");

        let region = region_at_byte_30.unwrap();
        assert_eq!(region.language, "lua");

        // Use result_id to get cached tokens
        let cached = token_cache.get(&uri, &region.result_id);
        assert!(cached.is_some(), "Should have cached tokens for lua region");
        assert_eq!(cached.unwrap().data[0].length, 3);
    }
}
