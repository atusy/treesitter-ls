use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tree_sitter::Language;

/// Registry for managing loaded Tree-sitter languages
/// This is separated from LanguageService to break circular dependency
#[derive(Clone)]
pub struct LanguageRegistry {
    languages: Arc<Mutex<HashMap<String, Language>>>,
}

impl Default for LanguageRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageRegistry {
    pub fn new() -> Self {
        Self {
            languages: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Register a language with the given ID
    pub fn register(&self, language_id: String, language: Language) {
        let mut languages = self.languages.lock().unwrap();
        languages.insert(language_id, language);
    }

    /// Get a language by ID
    pub fn get(&self, language_id: &str) -> Option<Language> {
        let languages = self.languages.lock().unwrap();
        languages.get(language_id).cloned()
    }

    /// Check if a language is registered
    pub fn contains(&self, language_id: &str) -> bool {
        let languages = self.languages.lock().unwrap();
        languages.contains_key(language_id)
    }

    /// Get all registered language IDs
    pub fn language_ids(&self) -> Vec<String> {
        let languages = self.languages.lock().unwrap();
        languages.keys().cloned().collect()
    }
}
