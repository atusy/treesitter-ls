use crate::config::{CaptureMappings, LanguageConfig, TreeSitterSettings};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Stores and manages language configurations
pub struct ConfigStore {
    language_configs: RwLock<HashMap<String, LanguageConfig>>,
    capture_mappings: RwLock<CaptureMappings>,
    search_paths: RwLock<Option<Vec<String>>>,
}

impl ConfigStore {
    pub fn new() -> Self {
        Self {
            language_configs: RwLock::new(HashMap::new()),
            capture_mappings: RwLock::new(CaptureMappings::default()),
            search_paths: RwLock::new(None),
        }
    }

    // ========== Language Configs ==========
    pub fn set_language_configs(&self, configs: HashMap<String, LanguageConfig>) {
        *self.language_configs.write().unwrap() = configs;
    }

    pub fn update_from_settings(&self, settings: &TreeSitterSettings) {
        self.set_language_configs(settings.languages.clone());
        self.set_capture_mappings(settings.capture_mappings.clone());
        self.set_search_paths(settings.search_paths.clone());
    }

    pub fn get_language_config(&self, lang_name: &str) -> Option<LanguageConfig> {
        self.language_configs
            .read()
            .unwrap()
            .get(lang_name)
            .cloned()
    }

    pub fn get_all_language_configs(&self) -> HashMap<String, LanguageConfig> {
        self.language_configs.read().unwrap().clone()
    }

    // ========== Capture Mappings ==========
    pub fn set_capture_mappings(&self, mappings: CaptureMappings) {
        *self.capture_mappings.write().unwrap() = mappings;
    }

    pub fn get_capture_mappings(&self) -> CaptureMappings {
        self.capture_mappings.read().unwrap().clone()
    }

    // ========== Search Paths ==========
    pub fn set_search_paths(&self, paths: Option<Vec<String>>) {
        *self.search_paths.write().unwrap() = paths;
    }

    pub fn get_search_paths(&self) -> Option<Vec<String>> {
        self.search_paths.read().unwrap().clone()
    }

    /// Get search paths as a shared reference
    pub fn get_search_paths_ref(&self) -> Arc<Option<Vec<String>>> {
        Arc::new(self.search_paths.read().unwrap().clone())
    }

    /// Clear all configurations
    pub fn clear(&self) {
        self.language_configs.write().unwrap().clear();
        *self.capture_mappings.write().unwrap() = CaptureMappings::default();
        *self.search_paths.write().unwrap() = None;
    }
}

impl Default for ConfigStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{HighlightItem, HighlightSource};

    #[test]
    fn test_config_store_language_configs() {
        let store = ConfigStore::new();

        let mut configs = HashMap::new();
        configs.insert(
            "rust".to_string(),
            LanguageConfig {
                library: Some("/path/to/rust.so".to_string()),
                filetypes: vec!["rs".to_string()],
                highlight: vec![HighlightItem {
                    source: HighlightSource::Query {
                        query: "(identifier) @variable".to_string(),
                    },
                }],
                locals: None,
            },
        );

        store.set_language_configs(configs.clone());

        // Test get individual config
        let rust_config = store.get_language_config("rust").unwrap();
        assert_eq!(rust_config.library, Some("/path/to/rust.so".to_string()));

        // Test get all configs
        let all_configs = store.get_all_language_configs();
        assert_eq!(all_configs.len(), 1);
        assert!(all_configs.contains_key("rust"));
    }

    #[test]
    fn test_config_store_capture_mappings() {
        let store = ConfigStore::new();

        let mappings = CaptureMappings::default();
        store.set_capture_mappings(mappings.clone());

        let retrieved = store.get_capture_mappings();
        // Just check that we can store and retrieve mappings
        assert_eq!(retrieved.len(), mappings.len());
    }

    #[test]
    fn test_config_store_search_paths() {
        let store = ConfigStore::new();

        let paths = vec!["/path/one".to_string(), "/path/two".to_string()];
        store.set_search_paths(Some(paths.clone()));

        let retrieved = store.get_search_paths().unwrap();
        assert_eq!(retrieved.len(), 2);
        assert_eq!(retrieved[0], "/path/one");
    }

    #[test]
    fn test_config_store_update_from_settings() {
        let store = ConfigStore::new();

        let settings = TreeSitterSettings {
            languages: {
                let mut langs = HashMap::new();
                langs.insert(
                    "python".to_string(),
                    LanguageConfig {
                        library: None,
                        filetypes: vec!["py".to_string()],
                        highlight: vec![],
                        locals: None,
                    },
                );
                langs
            },
            search_paths: Some(vec!["/search/path".to_string()]),
            capture_mappings: CaptureMappings::default(),
        };

        store.update_from_settings(&settings);

        assert!(store.get_language_config("python").is_some());
        assert_eq!(
            store.get_search_paths(),
            Some(vec!["/search/path".to_string()])
        );
    }

    #[test]
    fn test_config_store_clear() {
        let store = ConfigStore::new();

        let mut configs = HashMap::new();
        configs.insert(
            "go".to_string(),
            LanguageConfig {
                library: None,
                filetypes: vec!["go".to_string()],
                highlight: vec![],
                locals: None,
            },
        );
        store.set_language_configs(configs);
        store.set_search_paths(Some(vec!["/path".to_string()]));

        store.clear();

        assert!(store.get_language_config("go").is_none());
        assert!(store.get_search_paths().is_none());
    }
}
