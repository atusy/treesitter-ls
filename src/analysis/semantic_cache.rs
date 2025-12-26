//! Semantic token caching with result_id validation.
//!
//! Provides a dedicated cache for semantic tokens keyed by document URL,
//! enabling fast cache hits when result_id matches.

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
            },
            CacheableInjectionRegion {
                language: "python".to_string(),
                byte_range: 100..200,
                line_range: 10..20,
                result_id: "region-2".to_string(),
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
}
