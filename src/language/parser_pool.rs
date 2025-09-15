use std::collections::HashMap;
use std::sync::Arc;
use tree_sitter::Parser;

use crate::language::registry::LanguageRegistry;

// REFACTOR NOTE: Parser management has been unified in the language module
// to break circular dependency between workspace and syntax modules

/// Factory for creating Tree-sitter parsers with proper language configuration
pub struct ParserFactory {
    language_registry: Arc<LanguageRegistry>,
}

impl ParserFactory {
    /// Create a new ParserFactory with a reference to the language registry
    pub fn new(language_registry: Arc<LanguageRegistry>) -> Self {
        Self { language_registry }
    }

    /// Create a new parser for the specified language
    pub fn create_parser(&self, language_id: &str) -> Option<Parser> {
        self.language_registry.get(language_id).map(|lang| {
            let mut parser = Parser::new();
            parser.set_language(&lang).unwrap();
            parser
        })
    }
}

/// Per-document parser pool for efficient parser reuse
pub struct DocumentParserPool {
    /// Available parsers by language ID
    available: HashMap<String, Vec<Parser>>,
    /// Factory for creating new parsers
    factory: Arc<ParserFactory>,
}

impl DocumentParserPool {
    /// Create a new parser pool with the given factory
    pub fn new(factory: Arc<ParserFactory>) -> Self {
        Self {
            available: HashMap::new(),
            factory,
        }
    }

    /// Acquire a parser for the specified language
    /// Returns from pool if available, otherwise creates new
    pub fn acquire(&mut self, language_id: &str) -> Option<Parser> {
        // Try to get from pool first
        if let Some(parsers) = self.available.get_mut(language_id)
            && let Some(parser) = parsers.pop()
        {
            return Some(parser);
        }

        // Create new parser if not in pool
        self.factory.create_parser(language_id)
    }

    /// Release a parser back to the pool for reuse
    pub fn release(&mut self, language_id: String, parser: Parser) {
        self.available.entry(language_id).or_default().push(parser);
    }

    /// Clear all cached parsers
    pub fn clear(&mut self) {
        self.available.clear();
    }

    /// Get the number of cached parsers for a language
    pub fn pool_size(&self, language_id: &str) -> usize {
        self.available
            .get(language_id)
            .map(|v| v.len())
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_language_registry() -> Arc<LanguageRegistry> {
        let registry = LanguageRegistry::new();
        // Add test language (Rust) to the registry
        registry.register("rust".to_string(), tree_sitter_rust::LANGUAGE.into());
        Arc::new(registry)
    }

    #[test]
    fn test_parser_factory_create_parser() {
        let language_registry = create_test_language_registry();
        let factory = ParserFactory::new(language_registry);

        // Should create parser for known language
        let parser = factory.create_parser("rust");
        assert!(parser.is_some());

        // Should return None for unknown language
        let parser = factory.create_parser("unknown");
        assert!(parser.is_none());
    }

    #[test]
    fn test_document_parser_pool_acquire_release() {
        let language_registry = create_test_language_registry();
        let factory = Arc::new(ParserFactory::new(language_registry));
        let mut pool = DocumentParserPool::new(factory);

        // First acquire should create new parser
        let parser1 = pool.acquire("rust");
        assert!(parser1.is_some());
        assert_eq!(pool.pool_size("rust"), 0);

        // Release parser to pool
        pool.release("rust".to_string(), parser1.unwrap());
        assert_eq!(pool.pool_size("rust"), 1);

        // Second acquire should get from pool
        let parser2 = pool.acquire("rust");
        assert!(parser2.is_some());
        assert_eq!(pool.pool_size("rust"), 0);
    }

    #[test]
    fn test_document_parser_pool_clear() {
        let language_registry = create_test_language_registry();
        let factory = Arc::new(ParserFactory::new(language_registry));
        let mut pool = DocumentParserPool::new(factory);

        // Add parsers to pool
        let parser = pool.acquire("rust").unwrap();
        pool.release("rust".to_string(), parser);
        assert_eq!(pool.pool_size("rust"), 1);

        // Clear should remove all cached parsers
        pool.clear();
        assert_eq!(pool.pool_size("rust"), 0);
    }
}
