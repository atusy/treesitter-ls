use std::collections::HashMap;
use std::sync::Arc;
use tree_sitter::Parser;

use super::registry::LanguageRegistry;

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

    /// Create a parser configured for injection with optional timeout
    pub fn create_injection_parser(
        &self,
        language_id: &str,
        _timeout_micros: Option<u64>,
    ) -> Option<Parser> {
        // TODO: When tree-sitter updates, use parse_with_options instead of set_timeout_micros
        // For now, we create parsers without timeout to avoid deprecated API
        self.create_parser(language_id)
    }
}

/// Configuration for parsers (e.g., timeout settings)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ParserConfig {
    pub language_id: String,
    pub timeout_micros: Option<u64>,
}

/// Per-document parser pool for efficient parser reuse
pub struct DocumentParserPool {
    /// Available parsers by language ID
    available: HashMap<String, Vec<Parser>>,
    /// Available injection parsers by configuration
    injection_parsers: HashMap<ParserConfig, Vec<Parser>>,
    /// Factory for creating new parsers
    factory: Arc<ParserFactory>,
}

impl DocumentParserPool {
    /// Create a new parser pool with the given factory
    pub fn new(factory: Arc<ParserFactory>) -> Self {
        Self {
            available: HashMap::new(),
            injection_parsers: HashMap::new(),
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

    /// Acquire a parser configured for injection
    pub fn acquire_injection(
        &mut self,
        language_id: &str,
        timeout_micros: Option<u64>,
    ) -> Option<Parser> {
        let config = ParserConfig {
            language_id: language_id.to_string(),
            timeout_micros,
        };

        // Try to get from injection pool first
        if let Some(parsers) = self.injection_parsers.get_mut(&config)
            && let Some(parser) = parsers.pop()
        {
            return Some(parser);
        }

        // Create new injection parser if not in pool
        self.factory
            .create_injection_parser(language_id, timeout_micros)
    }

    /// Release an injection parser back to the pool
    pub fn release_injection(&mut self, config: ParserConfig, parser: Parser) {
        self.injection_parsers
            .entry(config)
            .or_default()
            .push(parser);
    }

    /// Release a parser back to the pool for reuse
    pub fn release(&mut self, language_id: String, parser: Parser) {
        self.available.entry(language_id).or_default().push(parser);
    }

    /// Clear all cached parsers
    pub fn clear(&mut self) {
        self.available.clear();
        self.injection_parsers.clear();
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
    fn test_parser_factory_create_injection_parser() {
        let language_registry = create_test_language_registry();
        let factory = ParserFactory::new(language_registry);

        // Should create injection parser with timeout
        let parser = factory.create_injection_parser("rust", Some(1000));
        assert!(parser.is_some());

        // Should create injection parser without timeout
        let parser = factory.create_injection_parser("rust", None);
        assert!(parser.is_some());
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
    fn test_document_parser_pool_injection() {
        let language_registry = create_test_language_registry();
        let factory = Arc::new(ParserFactory::new(language_registry));
        let mut pool = DocumentParserPool::new(factory);

        // First injection parser acquisition creates new
        let parser1 = pool.acquire_injection("rust", Some(5000));
        assert!(parser1.is_some());

        // Release injection parser to pool
        let config = ParserConfig {
            language_id: "rust".to_string(),
            timeout_micros: Some(5000),
        };
        pool.release_injection(config.clone(), parser1.unwrap());

        // Second acquire with same config should get from pool
        let parser2 = pool.acquire_injection("rust", Some(5000));
        assert!(parser2.is_some());

        // Different config creates new parser
        let parser3 = pool.acquire_injection("rust", None);
        assert!(parser3.is_some());
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
