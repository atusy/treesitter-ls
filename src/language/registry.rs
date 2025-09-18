use crate::error::LspResult;
use dashmap::DashMap;
use std::sync::Arc;
use tree_sitter::Language;

/// Registry for managing loaded Tree-sitter languages
#[derive(Clone)]
pub struct LanguageRegistry {
    languages: Arc<DashMap<String, Language>>,
}

impl Default for LanguageRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageRegistry {
    pub fn new() -> Self {
        Self {
            languages: Arc::new(DashMap::new()),
        }
    }

    /// Register a language with the given ID
    pub fn register(&self, language_id: String, language: Language) -> LspResult<()> {
        self.languages.insert(language_id, language);
        Ok(())
    }

    /// Register a language with the given ID (compatibility version)
    pub fn register_unchecked(&self, language_id: String, language: Language) {
        let _ = self.register(language_id, language);
    }

    /// Get a language by ID
    pub fn get(&self, language_id: &str) -> Option<Language> {
        self.languages
            .get(language_id)
            .map(|entry| entry.value().clone())
    }

    /// Check if a language is registered
    pub fn contains(&self, language_id: &str) -> bool {
        self.languages.contains_key(language_id)
    }

    /// Get all registered language IDs
    pub fn language_ids(&self) -> Vec<String> {
        self.languages
            .iter()
            .map(|entry| entry.key().clone())
            .collect()
    }
}
