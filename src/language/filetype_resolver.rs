use crate::config::{LanguageConfig, TreeSitterSettings};
use log::warn;
use std::collections::HashMap;
use std::sync::RwLock;
use tower_lsp::lsp_types::Url;

/// Resolves file types to language identifiers
pub struct FiletypeResolver {
    filetype_map: RwLock<HashMap<String, String>>,
}

impl FiletypeResolver {
    pub fn new() -> Self {
        Self {
            filetype_map: RwLock::new(HashMap::new()),
        }
    }

    /// Build filetype map from TreeSitter settings
    pub fn build_from_settings(&self, settings: &TreeSitterSettings) {
        let mut map = HashMap::new();
        for (language, config) in &settings.languages {
            for ext in &config.filetypes {
                map.insert(ext.clone(), language.clone());
            }
        }
        match self.filetype_map.write() {
            Ok(mut guard) => *guard = map,
            Err(poisoned) => {
                warn!(target: "treesitter_ls::lock_recovery", "Recovered from poisoned lock in filetype_resolver::build_from_settings");
                *poisoned.into_inner() = map;
            }
        }
    }

    /// Build filetype map from language configurations
    pub fn build_from_configs(&self, configs: &HashMap<String, LanguageConfig>) {
        let mut map = HashMap::new();
        for (language, config) in configs {
            for ext in &config.filetypes {
                map.insert(ext.clone(), language.clone());
            }
        }
        match self.filetype_map.write() {
            Ok(mut guard) => *guard = map,
            Err(poisoned) => {
                warn!(target: "treesitter_ls::lock_recovery", "Recovered from poisoned lock in filetype_resolver::build_from_configs");
                *poisoned.into_inner() = map;
            }
        }
    }

    /// Set the filetype map directly
    pub fn set_filetype_map(&self, map: HashMap<String, String>) {
        match self.filetype_map.write() {
            Ok(mut guard) => *guard = map,
            Err(poisoned) => {
                warn!(target: "treesitter_ls::lock_recovery", "Recovered from poisoned lock in filetype_resolver::set_filetype_map");
                *poisoned.into_inner() = map;
            }
        }
    }

    /// Get language for a document URL
    pub fn get_language_for_document(&self, uri: &Url) -> Option<String> {
        let extension = Self::extract_extension(uri.path());
        match self.filetype_map.read() {
            Ok(guard) => guard.get(extension).cloned(),
            Err(poisoned) => {
                warn!(target: "treesitter_ls::lock_recovery", "Recovered from poisoned lock in filetype_resolver::get_language_for_document");
                poisoned.into_inner().get(extension).cloned()
            }
        }
    }

    /// Get language for a file extension
    pub fn get_language_for_extension(&self, extension: &str) -> Option<String> {
        match self.filetype_map.read() {
            Ok(guard) => guard.get(extension).cloned(),
            Err(poisoned) => {
                warn!(target: "treesitter_ls::lock_recovery", "Recovered from poisoned lock in filetype_resolver::get_language_for_extension");
                poisoned.into_inner().get(extension).cloned()
            }
        }
    }

    /// Get a copy of the entire filetype map
    pub fn get_filetype_map(&self) -> HashMap<String, String> {
        match self.filetype_map.read() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => {
                warn!(target: "treesitter_ls::lock_recovery", "Recovered from poisoned lock in filetype_resolver::get_filetype_map");
                poisoned.into_inner().clone()
            }
        }
    }

    /// Add a single filetype mapping
    pub fn add_mapping(&self, extension: String, language: String) {
        match self.filetype_map.write() {
            Ok(mut guard) => {
                guard.insert(extension, language);
            }
            Err(poisoned) => {
                warn!(target: "treesitter_ls::lock_recovery", "Recovered from poisoned lock in filetype_resolver::add_mapping");
                poisoned.into_inner().insert(extension, language);
            }
        }
    }

    /// Remove a filetype mapping
    pub fn remove_mapping(&self, extension: &str) -> Option<String> {
        match self.filetype_map.write() {
            Ok(mut guard) => guard.remove(extension),
            Err(poisoned) => {
                warn!(target: "treesitter_ls::lock_recovery", "Recovered from poisoned lock in filetype_resolver::remove_mapping");
                poisoned.into_inner().remove(extension)
            }
        }
    }

    /// Clear all mappings
    pub fn clear(&self) {
        match self.filetype_map.write() {
            Ok(mut guard) => guard.clear(),
            Err(poisoned) => {
                warn!(target: "treesitter_ls::lock_recovery", "Recovered from poisoned lock in filetype_resolver::clear");
                poisoned.into_inner().clear();
            }
        }
    }

    /// Check if a language is registered for any extension
    pub fn has_language(&self, language: &str) -> bool {
        match self.filetype_map.read() {
            Ok(guard) => guard.values().any(|l| l == language),
            Err(poisoned) => {
                warn!(target: "treesitter_ls::lock_recovery", "Recovered from poisoned lock in filetype_resolver::has_language");
                poisoned.into_inner().values().any(|l| l == language)
            }
        }
    }

    /// Get all extensions for a language
    pub fn get_extensions_for_language(&self, language: &str) -> Vec<String> {
        match self.filetype_map.read() {
            Ok(guard) => guard
                .iter()
                .filter_map(|(ext, lang)| {
                    if lang == language {
                        Some(ext.clone())
                    } else {
                        None
                    }
                })
                .collect(),
            Err(poisoned) => {
                warn!(target: "treesitter_ls::lock_recovery", "Recovered from poisoned lock in filetype_resolver::get_extensions_for_language");
                poisoned
                    .into_inner()
                    .iter()
                    .filter_map(|(ext, lang)| {
                        if lang == language {
                            Some(ext.clone())
                        } else {
                            None
                        }
                    })
                    .collect()
            }
        }
    }

    /// Extract file extension from a path
    fn extract_extension(path: &str) -> &str {
        path.split('.').next_back().unwrap_or("")
    }
}

impl Default for FiletypeResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filetype_resolver_basic() {
        let resolver = FiletypeResolver::new();

        resolver.add_mapping("rs".to_string(), "rust".to_string());
        resolver.add_mapping("py".to_string(), "python".to_string());

        assert_eq!(
            resolver.get_language_for_extension("rs"),
            Some("rust".to_string())
        );
        assert_eq!(
            resolver.get_language_for_extension("py"),
            Some("python".to_string())
        );
        assert_eq!(resolver.get_language_for_extension("txt"), None);
    }

    #[test]
    fn test_filetype_resolver_from_settings() {
        let resolver = FiletypeResolver::new();

        let settings = TreeSitterSettings {
            languages: {
                let mut langs = HashMap::new();
                langs.insert(
                    "rust".to_string(),
                    LanguageConfig {
                        library: None,
                        filetypes: vec!["rs".to_string(), "rust".to_string()],
                        highlight: vec![],
                        locals: None,
                    },
                );
                langs.insert(
                    "python".to_string(),
                    LanguageConfig {
                        library: None,
                        filetypes: vec!["py".to_string(), "pyi".to_string()],
                        highlight: vec![],
                        locals: None,
                    },
                );
                langs
            },
            search_paths: None,
            capture_mappings: Default::default(),
        };

        resolver.build_from_settings(&settings);

        assert_eq!(
            resolver.get_language_for_extension("rs"),
            Some("rust".to_string())
        );
        assert_eq!(
            resolver.get_language_for_extension("rust"),
            Some("rust".to_string())
        );
        assert_eq!(
            resolver.get_language_for_extension("py"),
            Some("python".to_string())
        );
        assert_eq!(
            resolver.get_language_for_extension("pyi"),
            Some("python".to_string())
        );
    }

    #[test]
    fn test_filetype_resolver_document_url() {
        let resolver = FiletypeResolver::new();
        resolver.add_mapping("rs".to_string(), "rust".to_string());

        let url = Url::parse("file:///path/to/file.rs").unwrap();
        assert_eq!(
            resolver.get_language_for_document(&url),
            Some("rust".to_string())
        );

        let url = Url::parse("file:///path/to/file").unwrap();
        assert_eq!(resolver.get_language_for_document(&url), None);
    }

    #[test]
    fn test_filetype_resolver_reverse_lookup() {
        let resolver = FiletypeResolver::new();

        resolver.add_mapping("rs".to_string(), "rust".to_string());
        resolver.add_mapping("rust".to_string(), "rust".to_string());
        resolver.add_mapping("py".to_string(), "python".to_string());

        assert!(resolver.has_language("rust"));
        assert!(resolver.has_language("python"));
        assert!(!resolver.has_language("javascript"));

        let rust_exts = resolver.get_extensions_for_language("rust");
        assert_eq!(rust_exts.len(), 2);
        assert!(rust_exts.contains(&"rs".to_string()));
        assert!(rust_exts.contains(&"rust".to_string()));
    }

    #[test]
    fn test_filetype_resolver_remove_and_clear() {
        let resolver = FiletypeResolver::new();

        resolver.add_mapping("rs".to_string(), "rust".to_string());
        resolver.add_mapping("py".to_string(), "python".to_string());

        assert_eq!(resolver.remove_mapping("rs"), Some("rust".to_string()));
        assert_eq!(resolver.get_language_for_extension("rs"), None);

        resolver.clear();
        assert_eq!(resolver.get_language_for_extension("py"), None);
        assert_eq!(resolver.get_filetype_map().len(), 0);
    }
}
