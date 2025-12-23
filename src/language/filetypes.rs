use crate::config::{LanguageConfig, TreeSitterSettings};
use log::warn;
use std::collections::HashMap;
use std::sync::RwLock;

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
    /// DEPRECATED (PBI-061): LanguageConfig no longer has filetypes field.
    /// This method is now a no-op and will be removed.
    #[deprecated(note = "filetypes removed from config in PBI-061")]
    pub fn build_from_settings(&self, _settings: &TreeSitterSettings) {
        // No-op: filetypes field removed from LanguageConfig
    }

    /// Build filetype map from language configurations
    /// DEPRECATED (PBI-061): LanguageConfig no longer has filetypes field.
    /// This method is now a no-op and will be removed.
    #[deprecated(note = "filetypes removed from config in PBI-061")]
    pub fn build_from_configs(&self, _configs: &HashMap<String, LanguageConfig>) {
        // No-op: filetypes field removed from LanguageConfig
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

    /// Get language for a document path (URI path or file path)
    pub fn get_language_for_path(&self, path: &str) -> Option<String> {
        let extension = Self::extract_extension(path);
        match self.filetype_map.read() {
            Ok(guard) => guard.get(extension).cloned(),
            Err(poisoned) => {
                warn!(target: "treesitter_ls::lock_recovery", "Recovered from poisoned lock in filetype_resolver::get_language_for_path");
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
    #[allow(deprecated)]
    fn test_filetype_resolver_from_settings_is_noop() {
        // PBI-061: build_from_settings is now a no-op since filetypes removed from config
        let resolver = FiletypeResolver::new();

        let settings = TreeSitterSettings {
            languages: {
                let mut langs = HashMap::new();
                langs.insert(
                    "rust".to_string(),
                    LanguageConfig {
                        library: None,
                        highlights: None,
                        locals: None,
                        injections: None,
                    },
                );
                langs
            },
            search_paths: None,
            capture_mappings: Default::default(),
            auto_install: None,
        };

        resolver.build_from_settings(&settings);

        // Method is now a no-op, so no mappings are added
        assert_eq!(resolver.get_language_for_extension("rs"), None);
    }

    #[test]
    fn test_filetype_resolver_document_path() {
        let resolver = FiletypeResolver::new();
        resolver.add_mapping("rs".to_string(), "rust".to_string());

        assert_eq!(
            resolver.get_language_for_path("/path/to/file.rs"),
            Some("rust".to_string())
        );

        assert_eq!(resolver.get_language_for_path("/path/to/file"), None);
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
