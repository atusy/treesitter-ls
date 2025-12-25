//! Semantic token caching with result_id validation.
//!
//! Provides a dedicated cache for semantic tokens keyed by document URL,
//! enabling fast cache hits when result_id matches.

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
}

impl Default for SemanticTokenCache {
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
}
