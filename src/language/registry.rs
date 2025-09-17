use crate::error::LspResult;
use log::warn;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tree_sitter::Language;

/// Registry for managing loaded Tree-sitter languages
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
    pub fn register(&self, language_id: String, language: Language) -> LspResult<()> {
        match self.languages.lock() {
            Ok(mut languages) => {
                languages.insert(language_id, language);
                Ok(())
            }
            Err(poisoned) => {
                warn!(
                    target: "treesitter_ls::lock_recovery",
                    "Recovered from poisoned lock in registry::register for language: {}",
                    language_id
                );
                let mut languages = poisoned.into_inner();
                languages.insert(language_id, language);
                Ok(())
            }
        }
    }

    /// Register a language with the given ID (compatibility version)
    pub fn register_unchecked(&self, language_id: String, language: Language) {
        let _ = self.register(language_id, language);
    }

    /// Get a language by ID
    pub fn get(&self, language_id: &str) -> Option<Language> {
        match self.languages.lock() {
            Ok(languages) => languages.get(language_id).cloned(),
            Err(poisoned) => {
                warn!(
                    target: "treesitter_ls::lock_recovery",
                    "Recovered from poisoned lock in registry::get for language: {}",
                    language_id
                );
                poisoned.into_inner().get(language_id).cloned()
            }
        }
    }

    /// Check if a language is registered
    pub fn contains(&self, language_id: &str) -> bool {
        match self.languages.lock() {
            Ok(languages) => languages.contains_key(language_id),
            Err(poisoned) => {
                warn!(
                    target: "treesitter_ls::lock_recovery",
                    "Recovered from poisoned lock in registry::contains for language: {}",
                    language_id
                );
                poisoned.into_inner().contains_key(language_id)
            }
        }
    }

    /// Get all registered language IDs
    pub fn language_ids(&self) -> Vec<String> {
        match self.languages.lock() {
            Ok(languages) => languages.keys().cloned().collect(),
            Err(poisoned) => {
                warn!(
                    target: "treesitter_ls::lock_recovery",
                    "Recovered from poisoned lock in registry::language_ids"
                );
                poisoned.into_inner().keys().cloned().collect()
            }
        }
    }
}
