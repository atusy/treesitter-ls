use crate::config::{LanguageConfig, TreeSitterSettings};
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
        *self.filetype_map.write().unwrap() = map;
    }

    /// Build filetype map from language configurations
    pub fn build_from_configs(&self, configs: &HashMap<String, LanguageConfig>) {
        let mut map = HashMap::new();
        for (language, config) in configs {
            for ext in &config.filetypes {
                map.insert(ext.clone(), language.clone());
            }
        }
        *self.filetype_map.write().unwrap() = map;
    }

    /// Set the filetype map directly
    pub fn set_filetype_map(&self, map: HashMap<String, String>) {
        *self.filetype_map.write().unwrap() = map;
    }

    /// Get language for a document URL
    pub fn get_language_for_document(&self, uri: &Url) -> Option<String> {
        let extension = Self::extract_extension(uri.path());
        self.filetype_map.read().unwrap().get(extension).cloned()
    }

    /// Get language for a file extension
    pub fn get_language_for_extension(&self, extension: &str) -> Option<String> {
        self.filetype_map.read().unwrap().get(extension).cloned()
    }

    /// Get a copy of the entire filetype map
    pub fn get_filetype_map(&self) -> HashMap<String, String> {
        self.filetype_map.read().unwrap().clone()
    }

    /// Add a single filetype mapping
    pub fn add_mapping(&self, extension: String, language: String) {
        self.filetype_map
            .write()
            .unwrap()
            .insert(extension, language);
    }

    /// Remove a filetype mapping
    pub fn remove_mapping(&self, extension: &str) -> Option<String> {
        self.filetype_map.write().unwrap().remove(extension)
    }

    /// Clear all mappings
    pub fn clear(&self) {
        self.filetype_map.write().unwrap().clear();
    }

    /// Check if a language is registered for any extension
    pub fn has_language(&self, language: &str) -> bool {
        self.filetype_map
            .read()
            .unwrap()
            .values()
            .any(|l| l == language)
    }

    /// Get all extensions for a language
    pub fn get_extensions_for_language(&self, language: &str) -> Vec<String> {
        self.filetype_map
            .read()
            .unwrap()
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
