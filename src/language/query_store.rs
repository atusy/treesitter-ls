use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tree_sitter::Query;

/// Stores and manages Tree-sitter queries for different languages
pub struct QueryStore {
    highlight_queries: RwLock<HashMap<String, Arc<Query>>>,
    locals_queries: RwLock<HashMap<String, Arc<Query>>>,
    injections_queries: RwLock<HashMap<String, Arc<Query>>>,
}

impl QueryStore {
    pub fn new() -> Self {
        Self {
            highlight_queries: RwLock::new(HashMap::new()),
            locals_queries: RwLock::new(HashMap::new()),
            injections_queries: RwLock::new(HashMap::new()),
        }
    }

    // ========== Highlight Queries ==========
    pub fn insert_highlight_query(&self, lang_name: String, query: Arc<Query>) {
        self.highlight_queries
            .write()
            .unwrap()
            .insert(lang_name, query);
    }

    pub fn get_highlight_query(&self, lang_name: &str) -> Option<Arc<Query>> {
        self.highlight_queries
            .read()
            .unwrap()
            .get(lang_name)
            .cloned()
    }

    pub fn has_highlight_query(&self, lang_name: &str) -> bool {
        self.highlight_queries
            .read()
            .unwrap()
            .contains_key(lang_name)
    }

    // ========== Locals Queries ==========
    pub fn insert_locals_query(&self, lang_name: String, query: Arc<Query>) {
        self.locals_queries
            .write()
            .unwrap()
            .insert(lang_name, query);
    }

    pub fn get_locals_query(&self, lang_name: &str) -> Option<Arc<Query>> {
        self.locals_queries.read().unwrap().get(lang_name).cloned()
    }

    // ========== Injections Queries ==========
    pub fn insert_injections_query(&self, lang_name: String, query: Arc<Query>) {
        self.injections_queries
            .write()
            .unwrap()
            .insert(lang_name, query);
    }

    pub fn get_injections_query(&self, lang_name: &str) -> Option<Arc<Query>> {
        self.injections_queries
            .read()
            .unwrap()
            .get(lang_name)
            .cloned()
    }

    /// Clear all queries for a specific language
    pub fn clear_language(&self, lang_name: &str) {
        self.highlight_queries.write().unwrap().remove(lang_name);
        self.locals_queries.write().unwrap().remove(lang_name);
        self.injections_queries.write().unwrap().remove(lang_name);
    }

    /// Clear all queries
    pub fn clear_all(&self) {
        self.highlight_queries.write().unwrap().clear();
        self.locals_queries.write().unwrap().clear();
        self.injections_queries.write().unwrap().clear();
    }
}

impl Default for QueryStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Language;

    unsafe extern "C" {
        fn tree_sitter_rust() -> Language;
    }

    #[test]
    fn test_query_store_operations() {
        let store = QueryStore::new();
        let lang = unsafe { tree_sitter_rust() };

        // Create a simple query
        let query_str = "(identifier) @variable";
        let query = Arc::new(Query::new(&lang, query_str).unwrap());

        // Test highlight queries
        assert!(!store.has_highlight_query("rust"));
        store.insert_highlight_query("rust".to_string(), query.clone());
        assert!(store.has_highlight_query("rust"));
        assert_eq!(store.get_highlight_query("rust").unwrap(), query);

        // Test locals queries
        store.insert_locals_query("rust".to_string(), query.clone());
        assert_eq!(store.get_locals_query("rust").unwrap(), query);

        // Test injections queries
        store.insert_injections_query("rust".to_string(), query.clone());
        assert_eq!(store.get_injections_query("rust").unwrap(), query);

        // Test clear language
        store.clear_language("rust");
        assert!(!store.has_highlight_query("rust"));
        assert!(store.get_locals_query("rust").is_none());
        assert!(store.get_injections_query("rust").is_none());
    }

    #[test]
    fn test_query_store_clear_all() {
        let store = QueryStore::new();
        let lang = unsafe { tree_sitter_rust() };

        let query = Arc::new(Query::new(&lang, "(identifier) @variable").unwrap());

        store.insert_highlight_query("rust".to_string(), query.clone());
        store.insert_highlight_query("python".to_string(), query.clone());
        store.insert_locals_query("rust".to_string(), query.clone());

        store.clear_all();

        assert!(!store.has_highlight_query("rust"));
        assert!(!store.has_highlight_query("python"));
        assert!(store.get_locals_query("rust").is_none());
    }
}
