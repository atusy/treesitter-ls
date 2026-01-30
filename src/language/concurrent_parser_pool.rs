//! Concurrent parser pool with semaphore-based concurrency control.
//!
//! This module provides a thread-safe parser pool that limits concurrent
//! parsing operations using a semaphore. Unlike `DocumentParserPool` which
//! requires exclusive mutable access via `Mutex`, this pool can be shared
//! across tasks and uses async-aware concurrency control.
//!
//! # Design
//!
//! - Uses `DashMap` for lock-free per-language parser storage
//! - `Semaphore` limits total concurrent parsers (default: [`DEFAULT_CONCURRENCY_LIMIT`])
//! - `PooledParser` RAII guard ensures proper cleanup on drop
//! - Parsers are returned to pool when the guard is dropped, releasing the permit

use std::collections::VecDeque;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tree_sitter::Parser;

use super::parser_pool::ParserFactory;

/// Type alias for the shared parser storage
type ParserStorage = Arc<DashMap<String, VecDeque<Parser>>>;

/// Default concurrency limit for parser pool.
///
/// This constant limits the number of concurrent parsing operations.
/// Value of 10 balances parallelism against resource usage for typical
/// documents with multiple injection blocks.
pub const DEFAULT_CONCURRENCY_LIMIT: usize = 10;

/// A concurrent parser pool with bounded concurrency.
///
/// This pool manages Tree-sitter parsers and limits concurrent access using
/// a semaphore. Multiple tasks can acquire parsers simultaneously (up to the
/// concurrency limit), enabling parallel parsing of injection blocks.
///
/// # Thread Safety
///
/// The pool is `Send + Sync` and can be safely shared across tasks via `Arc`.
/// Each `acquire()` call waits for a semaphore permit before returning a parser.
pub struct ConcurrentParserPool {
    /// Per-language parser storage (lock-free concurrent map, shared via Arc)
    pools: ParserStorage,
    /// Factory for creating new parsers
    factory: ParserFactory,
    /// Semaphore limiting total concurrent parsers
    semaphore: Arc<Semaphore>,
    /// Maximum concurrent parsers (stored for reporting via concurrency_limit())
    max_concurrent: usize,
}

impl ConcurrentParserPool {
    /// Create a new concurrent parser pool with the given factory.
    ///
    /// Uses the default concurrency limit ([`DEFAULT_CONCURRENCY_LIMIT`]).
    pub fn new(factory: ParserFactory) -> Self {
        Self::with_concurrency_limit(factory, DEFAULT_CONCURRENCY_LIMIT)
    }

    /// Create a new concurrent parser pool with a custom concurrency limit.
    ///
    /// # Arguments
    /// * `factory` - Factory for creating new parsers
    /// * `concurrency_limit` - Maximum number of concurrent parsers
    pub fn with_concurrency_limit(factory: ParserFactory, concurrency_limit: usize) -> Self {
        Self {
            pools: Arc::new(DashMap::new()),
            factory,
            semaphore: Arc::new(Semaphore::new(concurrency_limit)),
            max_concurrent: concurrency_limit,
        }
    }

    /// Acquire a parser for the specified language.
    ///
    /// This method:
    /// 1. Waits for a semaphore permit (respecting concurrency limit)
    /// 2. Gets an existing parser from the pool or creates a new one
    /// 3. Returns a `PooledParser` guard that releases the parser and permit on drop
    ///
    /// Returns `None` if no parser could be created for the language
    /// (e.g., language not registered in the factory).
    ///
    /// # Example
    /// ```ignore
    /// let pool = ConcurrentParserPool::new(factory);
    /// if let Some(mut parser) = pool.acquire("lua").await {
    ///     let tree = parser.parse("local x = 1", None);
    /// }
    /// // Parser automatically returned to pool here
    /// ```
    pub async fn acquire(&self, language_id: &str) -> Option<PooledParser> {
        // Wait for semaphore permit
        let permit = self.semaphore.clone().acquire_owned().await.ok()?;

        // Try to get parser from pool
        let parser = {
            let mut entry = self.pools.entry(language_id.to_string()).or_default();
            entry.pop_front()
        };

        // Get from pool or create new
        let parser = parser.or_else(|| self.factory.create_parser(language_id))?;

        Some(PooledParser {
            parser: Some(parser),
            language_id: language_id.to_string(),
            pool: Arc::clone(&self.pools),
            _permit: permit,
        })
    }

    /// Get the current concurrency limit.
    pub fn concurrency_limit(&self) -> usize {
        self.max_concurrent
    }

    /// Get the number of available permits (for testing).
    #[cfg(test)]
    pub(crate) fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }
}

/// RAII guard for a pooled parser.
///
/// When dropped, returns the parser to the pool and releases the semaphore permit.
/// Provides `Deref` and `DerefMut` for transparent access to the underlying `Parser`.
pub struct PooledParser {
    parser: Option<Parser>,
    language_id: String,
    pool: ParserStorage,
    _permit: OwnedSemaphorePermit,
}

impl Deref for PooledParser {
    type Target = Parser;

    fn deref(&self) -> &Self::Target {
        self.parser.as_ref().expect("parser should exist")
    }
}

impl DerefMut for PooledParser {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.parser.as_mut().expect("parser should exist")
    }
}

impl Drop for PooledParser {
    fn drop(&mut self) {
        // Return parser to pool
        if let Some(parser) = self.parser.take() {
            let mut entry = self.pool.entry(self.language_id.clone()).or_default();
            entry.push_back(parser);
        }
        // Permit is automatically released when _permit is dropped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::LanguageRegistry;
    use std::time::Duration;

    fn create_test_factory() -> ParserFactory {
        let registry = LanguageRegistry::new();
        // Register test language (Rust) to the registry
        registry.register_unchecked("rust".to_string(), tree_sitter_rust::LANGUAGE.into());
        ParserFactory::new(registry)
    }

    /// Test: Pool creation with hardcoded concurrency limit
    #[tokio::test]
    async fn test_pool_creation_with_default_limit() {
        let factory = create_test_factory();
        let pool = ConcurrentParserPool::new(factory);

        // Concurrency limit should match DEFAULT_CONCURRENCY_LIMIT
        assert_eq!(
            pool.available_permits(),
            DEFAULT_CONCURRENCY_LIMIT,
            "Default pool should have {} permits",
            DEFAULT_CONCURRENCY_LIMIT
        );
    }

    /// Test: Pool creation with custom concurrency limit
    #[tokio::test]
    async fn test_pool_creation_with_custom_limit() {
        let factory = create_test_factory();
        let pool = ConcurrentParserPool::with_concurrency_limit(factory, 3);

        assert_eq!(pool.available_permits(), 3, "Pool should have 3 permits");
    }

    /// Test: concurrency_limit() returns configured limit, not available permits
    #[tokio::test]
    async fn test_concurrency_limit_returns_configured_value() {
        let factory = create_test_factory();
        let pool = ConcurrentParserPool::with_concurrency_limit(factory, 5);

        // Before acquiring any parsers
        assert_eq!(
            pool.concurrency_limit(),
            5,
            "concurrency_limit() should return configured value"
        );

        // Acquire some parsers
        let _p1 = pool.acquire("rust").await;
        let _p2 = pool.acquire("rust").await;

        // concurrency_limit() should still return 5, not 3 (available permits)
        assert_eq!(
            pool.concurrency_limit(),
            5,
            "concurrency_limit() should return configured value even after acquiring parsers"
        );
        assert_eq!(
            pool.available_permits(),
            3,
            "available_permits() should reflect current usage"
        );
    }

    /// Test: acquire() returns a parser for a known language
    #[tokio::test]
    async fn test_acquire_returns_parser_for_known_language() {
        let factory = create_test_factory();
        let pool = ConcurrentParserPool::new(factory);

        let parser = pool.acquire("rust").await;
        assert!(parser.is_some(), "Should acquire parser for known language");
    }

    /// Test: acquire() returns None for unknown language
    #[tokio::test]
    async fn test_acquire_returns_none_for_unknown_language() {
        let factory = create_test_factory();
        let pool = ConcurrentParserPool::new(factory);

        let parser = pool.acquire("unknown_language").await;
        assert!(parser.is_none(), "Should return None for unknown language");
    }

    /// Test: Multiple acquires for same language work (multiple parsers per language)
    #[tokio::test]
    async fn test_multiple_acquires_for_same_language() {
        let factory = create_test_factory();
        let pool = ConcurrentParserPool::with_concurrency_limit(factory, 3);

        // Acquire 3 parsers for the same language
        let p1 = pool.acquire("rust").await;
        let p2 = pool.acquire("rust").await;
        let p3 = pool.acquire("rust").await;

        assert!(p1.is_some(), "First acquire should succeed");
        assert!(p2.is_some(), "Second acquire should succeed");
        assert!(p3.is_some(), "Third acquire should succeed");

        // All permits should be used
        assert_eq!(pool.available_permits(), 0, "All permits should be used");
    }

    /// Test: Acquired parser can be used to parse code
    #[tokio::test]
    async fn test_acquired_parser_can_parse() {
        let factory = create_test_factory();
        let pool = ConcurrentParserPool::new(factory);

        let mut parser = pool.acquire("rust").await.expect("Should acquire parser");
        let tree = parser.parse("fn main() {}", None);
        assert!(tree.is_some(), "Parser should be able to parse code");
    }

    /// Test: Parser is returned to pool on drop
    #[tokio::test]
    async fn test_parser_returned_to_pool_on_drop() {
        let factory = create_test_factory();
        let pool = ConcurrentParserPool::with_concurrency_limit(factory, 2);

        let initial_permits = pool.available_permits();

        {
            let _parser = pool.acquire("rust").await;
            assert_eq!(
                pool.available_permits(),
                initial_permits - 1,
                "Permit should be consumed"
            );
        }

        // After drop, permit should be released
        assert_eq!(
            pool.available_permits(),
            initial_permits,
            "Permit should be released after drop"
        );
    }

    /// Test: acquire() blocks when semaphore is exhausted
    #[tokio::test]
    async fn test_acquire_blocks_when_semaphore_exhausted() {
        let factory = create_test_factory();
        let pool = Arc::new(ConcurrentParserPool::with_concurrency_limit(factory, 1));

        // Acquire the only permit
        let _held_parser = pool.acquire("rust").await.expect("Should acquire parser");

        // Clone pool for async block
        let pool_clone = pool.clone();

        // Try to acquire another parser with timeout
        let result = tokio::time::timeout(Duration::from_millis(50), async move {
            pool_clone.acquire("rust").await
        })
        .await;

        // Should timeout because the permit is held
        assert!(
            result.is_err(),
            "acquire() should block when semaphore is exhausted"
        );
    }

    /// Test: Blocked acquire succeeds when permit is released
    #[tokio::test]
    async fn test_blocked_acquire_succeeds_after_release() {
        let factory = create_test_factory();
        let pool = Arc::new(ConcurrentParserPool::with_concurrency_limit(factory, 1));

        let pool_clone = pool.clone();

        // Spawn a task that holds the permit briefly then releases
        let handle = tokio::spawn(async move {
            let _parser = pool_clone.acquire("rust").await;
            tokio::time::sleep(Duration::from_millis(10)).await;
            // Parser dropped here, releasing permit
        });

        // Wait for the task to acquire
        tokio::time::sleep(Duration::from_millis(5)).await;

        // This acquire should eventually succeed after the permit is released
        let result = tokio::time::timeout(Duration::from_millis(100), pool.acquire("rust")).await;

        handle.await.unwrap();

        assert!(
            result.is_ok() && result.unwrap().is_some(),
            "Blocked acquire should succeed after permit is released"
        );
    }

    /// Test: Pool works with Arc sharing across tasks
    #[tokio::test]
    async fn test_pool_works_with_arc_sharing() {
        let factory = create_test_factory();
        let pool = Arc::new(ConcurrentParserPool::with_concurrency_limit(factory, 4));

        let mut handles = Vec::new();

        // Spawn multiple tasks that acquire and use parsers
        for i in 0..4 {
            let pool_clone = pool.clone();
            handles.push(tokio::spawn(async move {
                let mut parser = pool_clone.acquire("rust").await.expect("Should acquire");
                let code = format!("fn task_{}() {{}}", i);
                let tree = parser.parse(&code, None);
                tree.is_some()
            }));
        }

        // All tasks should succeed
        for handle in handles {
            assert!(handle.await.unwrap(), "Task should complete successfully");
        }
    }
}
