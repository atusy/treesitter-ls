use std::collections::HashMap;
use std::sync::Arc;
use tree_sitter::Parser;

use crate::state::LanguageService;

// REFACTOR NOTE (Step 3-1): Parser management is currently duplicated:
// - LanguageService::parsers maintains a global parser cache
// - DocumentParserPool maintains per-document parser pools
// - These two systems don't coordinate, leading to inefficient memory usage
// TODO: Unify parser management in Step 3-2

/// Factory for creating Tree-sitter parsers with proper language configuration
pub struct ParserFactory {
    language_service: Arc<LanguageService>,
}

impl ParserFactory {
    /// Create a new ParserFactory with a reference to the language service
    pub fn new(language_service: Arc<LanguageService>) -> Self {
        Self { language_service }
    }

    /// Create a new parser for the specified language
    pub fn create_parser(&self, language_id: &str) -> Option<Parser> {
        let languages = self.language_service.languages.lock().unwrap();
        languages.get(language_id).map(|lang| {
            let mut parser = Parser::new();
            parser.set_language(lang).unwrap();
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

    /// Acquire a parser configured for injection
    pub fn acquire_injection(
        &mut self,
        language_id: &str,
        timeout_micros: Option<u64>,
    ) -> Option<Parser> {
        // FIXME: Injection parsers are always created fresh, never pooled
        // This is inefficient for documents with many injections
        // TODO: Pool injection parsers with configuration tracking in Step 3-4
        self.factory
            .create_injection_parser(language_id, timeout_micros)
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
    use crate::state::LanguageService;

    fn create_test_language_service() -> Arc<LanguageService> {
        let service = LanguageService::new();
        // Add test language (Rust) to the service
        let mut languages = service.languages.lock().unwrap();
        languages.insert("rust".to_string(), tree_sitter_rust::LANGUAGE.into());
        drop(languages);
        Arc::new(service)
    }

    #[test]
    fn test_parser_factory_create_parser() {
        let language_service = create_test_language_service();
        let factory = ParserFactory::new(language_service);

        // Should create parser for known language
        let parser = factory.create_parser("rust");
        assert!(parser.is_some());

        // Should return None for unknown language
        let parser = factory.create_parser("unknown");
        assert!(parser.is_none());
    }

    #[test]
    fn test_parser_factory_create_injection_parser() {
        let language_service = create_test_language_service();
        let factory = ParserFactory::new(language_service);

        // Should create injection parser with timeout
        let parser = factory.create_injection_parser("rust", Some(1000));
        assert!(parser.is_some());

        // Should create injection parser without timeout
        let parser = factory.create_injection_parser("rust", None);
        assert!(parser.is_some());
    }

    #[test]
    fn test_document_parser_pool_acquire_release() {
        let language_service = create_test_language_service();
        let factory = Arc::new(ParserFactory::new(language_service));
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
        let language_service = create_test_language_service();
        let factory = Arc::new(ParserFactory::new(language_service));
        let mut pool = DocumentParserPool::new(factory);

        // Injection parsers are always created fresh
        let parser1 = pool.acquire_injection("rust", Some(5000));
        assert!(parser1.is_some());

        let parser2 = pool.acquire_injection("rust", None);
        assert!(parser2.is_some());
    }

    #[test]
    fn test_document_parser_pool_clear() {
        let language_service = create_test_language_service();
        let factory = Arc::new(ParserFactory::new(language_service));
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
