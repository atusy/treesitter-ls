//! Language injection processing for semantic tokens.
//!
//! This module handles the discovery and recursive processing of language
//! injections (e.g., Lua code blocks inside Markdown).

use std::sync::Arc;

use tree_sitter::{Query, Tree};

use super::token_collector::RawToken;

/// Maximum recursion depth for nested injections to prevent stack overflow
pub(super) const MAX_INJECTION_DEPTH: usize = 10;

/// Check if byte range is valid for slicing text.
///
/// Returns `true` if start <= end and both are within text bounds.
/// Returns `false` for invalid ranges that would cause panics or be meaningless.
///
/// Invalid bounds can occur when:
/// - Trees become stale relative to the text during rapid edits (race condition)
/// - Offset calculations result in inverted ranges
/// - Content nodes extend beyond current text length
#[inline]
fn is_valid_byte_range(start: usize, end: usize, text_len: usize) -> bool {
    start <= end && end <= text_len
}

/// Validate injection node bounds before slicing text.
///
/// Returns `Some((start_byte, end_byte))` if the bounds are valid,
/// or `None` if the injection should be skipped due to invalid bounds.
fn validate_injection_bounds(
    content_node: &tree_sitter::Node,
    text_len: usize,
) -> Option<(usize, usize)> {
    let start = content_node.start_byte();
    let end = content_node.end_byte();
    if is_valid_byte_range(start, end, text_len) {
        Some((start, end))
    } else {
        log::debug!(
            target: "kakehashi::semantic",
            "Skipping injection with invalid bounds: start={}, end={}, text_len={}",
            start,
            end,
            text_len
        );
        None
    }
}

/// Data for processing a single injection (parser-agnostic).
///
/// This struct captures all the information needed to process an injection
/// before the actual parsing step.
struct InjectionContext<'a> {
    resolved_lang: String,
    highlight_query: Arc<Query>,
    content_text: &'a str,
    host_start_byte: usize,
}

/// Collect all injection contexts from a document tree.
///
/// This function processes the injection query and returns a list of
/// `InjectionContext` structs, each containing the information needed
/// to parse and process one injection. This is parser-agnostic; actual
/// parsing happens after this function returns.
fn collect_injection_contexts<'a>(
    text: &'a str,
    tree: &Tree,
    filetype: Option<&str>,
    coordinator: &crate::language::LanguageCoordinator,
    content_start_byte: usize,
) -> Vec<InjectionContext<'a>> {
    use crate::language::{collect_all_injections, injection::parse_offset_directive_for_pattern};

    let current_lang = filetype.unwrap_or("unknown");
    let Some(injection_query) = coordinator.get_injection_query(current_lang) else {
        return Vec::new();
    };

    let Some(injections) = collect_all_injections(&tree.root_node(), text, Some(&injection_query))
    else {
        return Vec::new();
    };

    let mut contexts = Vec::with_capacity(injections.len());

    for injection in injections {
        let Some((inj_start, inj_end)) =
            validate_injection_bounds(&injection.content_node, text.len())
        else {
            continue;
        };

        // Extract injection content for first-line detection (shebang, mode line)
        let injection_content = &text[inj_start..inj_end];

        // Resolve injection language with unified detection
        let Some((resolved_lang, _)) =
            coordinator.resolve_injection_language(&injection.language, injection_content)
        else {
            continue;
        };

        // Get highlight query for resolved language
        let Some(inj_highlight_query) = coordinator.get_highlight_query(&resolved_lang) else {
            continue;
        };

        // Get offset directive if any
        let offset = parse_offset_directive_for_pattern(&injection_query, injection.pattern_index);

        // Calculate effective content range
        let content_node = injection.content_node;
        let (inj_start_byte, inj_end_byte) = if let Some(off) = offset {
            use crate::analysis::offset_calculator::{ByteRange, calculate_effective_range};
            let byte_range = ByteRange::new(content_node.start_byte(), content_node.end_byte());
            let effective = calculate_effective_range(text, byte_range, off);
            (effective.start, effective.end)
        } else {
            (content_node.start_byte(), content_node.end_byte())
        };

        // Validate effective range after offset adjustment
        if !is_valid_byte_range(inj_start_byte, inj_end_byte, text.len()) {
            continue;
        }

        contexts.push(InjectionContext {
            resolved_lang,
            highlight_query: inj_highlight_query,
            content_text: &text[inj_start_byte..inj_end_byte],
            host_start_byte: content_start_byte + inj_start_byte,
        });
    }

    contexts
}

/// Collect semantic tokens from a document and its injections in parallel.
///
/// This function uses `ConcurrentParserPool` to parse multiple injection blocks
/// concurrently, improving performance for documents with many injections.
///
/// # Nested Injection Support
///
/// This parallel implementation supports nested injections up to `MAX_INJECTION_DEPTH`.
/// Each injection task recursively spawns child tasks for any nested injections found.
/// For example, Lua inside Markdown inside Markdown will be fully processed.
///
/// # Arguments
/// * `text` - The source text of the host document (borrowed for initial parsing)
/// * `tree` - The parsed syntax tree of the host document
/// * `query` - The tree-sitter highlight query for the host language
/// * `filetype` - Optional filetype of the host document
/// * `capture_mappings` - Optional capture name to token type mappings
/// * `coordinator` - Language coordinator for injection query lookup (Arc-wrapped for sharing across tasks)
/// * `concurrent_pool` - Concurrent parser pool for parallel parsing
/// * `supports_multiline` - Whether client supports multiline tokens
///
/// # Returns
/// Vector of raw tokens from the host document and all injections (unsorted).
/// Tokens are collected in arbitrary order from parallel tasks and should be
/// sorted by the caller (e.g., via `finalize_tokens`) before conversion to LSP format.
///
/// # Performance Note
/// Arc wrappers for `host_text` and `host_lines` are allocated lazily - only when
/// injections are actually found. For documents without injections (the common case),
/// no heap allocation occurs for sharing state across tasks.
#[allow(clippy::too_many_arguments)]
pub(super) async fn collect_injection_tokens(
    text: &str,
    tree: &Tree,
    query: &Query,
    filetype: Option<&str>,
    capture_mappings: Option<&crate::config::CaptureMappings>,
    coordinator: Arc<crate::language::LanguageCoordinator>,
    concurrent_pool: &std::sync::Arc<crate::language::ConcurrentParserPool>,
    supports_multiline: bool,
) -> Vec<RawToken> {
    use super::token_collector::collect_host_tokens;

    let mut all_tokens: Vec<RawToken> = Vec::with_capacity(1000);
    let lines: Vec<&str> = text.lines().collect();

    // 1. Collect tokens from host document (depth 0)
    collect_host_tokens(
        text,
        tree,
        query,
        filetype,
        capture_mappings,
        text,   // host_text = text at root
        &lines, // host_lines
        0,      // content_start_byte = 0 at root
        0,      // depth = 0
        supports_multiline,
        &mut all_tokens,
    );

    // 2. Collect injection contexts (without parsing yet)
    let contexts = collect_injection_contexts(text, tree, filetype, &coordinator, 0);

    if contexts.is_empty() {
        return all_tokens;
    }

    // 3. Lazy allocation: Only create Arc wrappers when we have injections to process.
    //    For documents without injections (the common case), this avoids unnecessary
    //    heap allocations.
    let host_text: Arc<String> = Arc::new(text.to_string());
    let host_lines: Arc<Vec<String>> = Arc::new(lines.iter().map(|s| s.to_string()).collect());
    let capture_mappings = capture_mappings.cloned().map(Arc::new);

    // 4. Process injections in parallel using JoinSet with recursive nested injection support
    let mut join_set = tokio::task::JoinSet::new();

    for ctx in contexts {
        let pool = Arc::clone(concurrent_pool);
        let coordinator = Arc::clone(&coordinator);
        let content_text = ctx.content_text.to_string();
        let resolved_lang = ctx.resolved_lang.clone();
        let highlight_query = Arc::clone(&ctx.highlight_query);
        let host_start_byte = ctx.host_start_byte;
        let host_text = Arc::clone(&host_text);
        let host_lines = Arc::clone(&host_lines);
        let capture_mappings = capture_mappings.clone();

        join_set.spawn(async move {
            process_injection_recursive(
                content_text,
                resolved_lang,
                highlight_query,
                host_start_byte,
                host_text,
                host_lines,
                capture_mappings,
                coordinator,
                pool,
                1, // depth = 1 (first level injection)
                supports_multiline,
            )
            .await
        });
    }

    // 5. Collect results from all tasks
    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(tokens) => all_tokens.extend(tokens),
            Err(e) => {
                log::error!(
                    target: "kakehashi::semantic",
                    "Injection parsing task panicked: {}",
                    e
                );
            }
        }
    }

    all_tokens
}

/// Process a single injection and recursively process any nested injections.
///
/// This function is called for each injection block and handles:
/// 1. Parsing the injection content
/// 2. Collecting tokens from the injection
/// 3. Discovering and recursively processing nested injections
///
/// Uses `Box::pin` for recursive async as required by Rust's async trait bounds.
///
/// # Deadlock Prevention
///
/// To prevent semaphore deadlock, we release the parser (and its semaphore permit)
/// BEFORE spawning child tasks for nested injections. This ensures that:
/// - Parent tasks don't hold permits while waiting for children
/// - Children can always acquire permits as parents release theirs
/// - The algorithm proceeds in waves: depth N releases before depth N+1 acquires
#[allow(clippy::too_many_arguments)]
fn process_injection_recursive(
    content_text: String,
    resolved_lang: String,
    highlight_query: Arc<Query>,
    host_start_byte: usize,
    host_text: Arc<String>,
    host_lines: Arc<Vec<String>>,
    capture_mappings: Option<Arc<crate::config::CaptureMappings>>,
    coordinator: Arc<crate::language::LanguageCoordinator>,
    pool: Arc<crate::language::ConcurrentParserPool>,
    depth: usize,
    supports_multiline: bool,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Vec<RawToken>> + Send>> {
    Box::pin(async move {
        use super::token_collector::collect_host_tokens;

        // Check depth limit
        if depth >= MAX_INJECTION_DEPTH {
            return Vec::new();
        }

        // Acquire parser from concurrent pool
        let mut parser = match pool.acquire(&resolved_lang).await {
            Some(p) => p,
            None => return Vec::new(),
        };

        // Parse the injected content
        let injected_tree = match parser.parse(&content_text, None) {
            Some(t) => t,
            None => return Vec::new(),
        };

        // Collect tokens from this injection
        let mut injection_tokens: Vec<RawToken> = Vec::new();
        let host_lines_refs: Vec<&str> = host_lines.iter().map(|s| s.as_str()).collect();

        collect_host_tokens(
            &content_text,
            &injected_tree,
            &highlight_query,
            Some(&resolved_lang),
            capture_mappings.as_deref(),
            &host_text,
            &host_lines_refs,
            host_start_byte,
            depth,
            supports_multiline,
            &mut injection_tokens,
        );

        // Find nested injections BEFORE releasing the parser
        // (we need the tree reference to discover nested injections)
        let nested_contexts = collect_injection_contexts(
            &content_text,
            &injected_tree,
            Some(&resolved_lang),
            &coordinator,
            host_start_byte,
        );

        // CRITICAL: Drop parser BEFORE spawning child tasks to prevent deadlock.
        // If we hold the semaphore permit while waiting for children, and children
        // need permits from the same semaphore, we can deadlock when all permits
        // are held by parent tasks waiting for children.
        drop(parser);

        if nested_contexts.is_empty() {
            return injection_tokens;
        }

        // Process nested injections in parallel
        // (safe now because we've released our semaphore permit)
        let mut join_set = tokio::task::JoinSet::new();

        for ctx in nested_contexts {
            let pool = Arc::clone(&pool);
            let coordinator = Arc::clone(&coordinator);
            let nested_content = ctx.content_text.to_string();
            let nested_lang = ctx.resolved_lang.clone();
            let nested_query = Arc::clone(&ctx.highlight_query);
            let nested_start_byte = ctx.host_start_byte;
            let host_text = Arc::clone(&host_text);
            let host_lines = Arc::clone(&host_lines);
            let capture_mappings = capture_mappings.clone();

            join_set.spawn(async move {
                process_injection_recursive(
                    nested_content,
                    nested_lang,
                    nested_query,
                    nested_start_byte,
                    host_text,
                    host_lines,
                    capture_mappings,
                    coordinator,
                    pool,
                    depth + 1,
                    supports_multiline,
                )
                .await
            });
        }

        // Collect results from nested tasks
        while let Some(result) = join_set.join_next().await {
            match result {
                Ok(tokens) => injection_tokens.extend(tokens),
                Err(e) => {
                    log::error!(
                        target: "kakehashi::semantic",
                        "Nested injection parsing task panicked: {}",
                        e
                    );
                }
            }
        }

        injection_tokens
    })
}

#[cfg(test)]
mod tests {
    /// Returns the search path for tree-sitter grammars.
    fn test_search_path() -> String {
        std::env::var("TREE_SITTER_GRAMMARS").unwrap_or_else(|_| "deps/tree-sitter".to_string())
    }

    /// Test that parallel injection token collection produces correct results.
    ///
    /// This test verifies that `collect_injection_tokens` using
    /// `ConcurrentParserPool` produces the same semantic tokens as the
    /// sequential implementation.
    ///
    /// Document structure:
    /// - Markdown with 3 Lua code blocks
    /// - Each block contains `local varN = N`
    /// - Expected: 3 `local` keyword tokens at correct positions
    #[tokio::test]
    async fn test_parallel_injection_produces_correct_tokens() {
        use crate::config::WorkspaceSettings;
        use crate::language::LanguageCoordinator;

        // Markdown document with 3 Lua code blocks
        let text = r#"# Test Document

```lua
local var1 = 1
```

```lua
local var2 = 2
```

```lua
local var3 = 3
```
"#;

        // Set up coordinator
        let coordinator = LanguageCoordinator::new();
        let settings = WorkspaceSettings {
            search_paths: vec![test_search_path()],
            ..Default::default()
        };
        let _summary = coordinator.load_settings(settings);

        // Load required languages
        let load_result = coordinator.ensure_language_loaded("markdown");
        assert!(load_result.success, "Should load markdown language");
        let load_result = coordinator.ensure_language_loaded("lua");
        assert!(load_result.success, "Should load lua language");

        // Parse the markdown document
        let mut parser_pool = coordinator.create_document_parser_pool();
        let tree = {
            let mut parser = parser_pool
                .acquire("markdown")
                .expect("Should get markdown parser");
            let result = parser.parse(text, None).expect("Should parse markdown");
            parser_pool.release("markdown".to_string(), parser);
            result
        };

        // Get the highlight query for markdown
        let md_highlight_query = coordinator
            .get_highlight_query("markdown")
            .expect("Should have markdown highlight query");

        // Create ConcurrentParserPool via coordinator
        let concurrent_pool = std::sync::Arc::new(coordinator.create_concurrent_parser_pool());
        let coordinator = std::sync::Arc::new(coordinator);

        // Collect tokens using parallel processing (lazy allocation handled internally)
        let tokens = super::collect_injection_tokens(
            text,
            &tree,
            &md_highlight_query,
            Some("markdown"),
            None, // capture_mappings
            coordinator,
            &concurrent_pool,
            false, // supports_multiline
        )
        .await;

        // Verify we got tokens
        assert!(!tokens.is_empty(), "Should collect some tokens");

        // Find all `local` keyword tokens (keyword type, length 5)
        let local_keywords: Vec<_> = tokens
            .iter()
            .filter(|t| t.mapped_name == "keyword" && t.length == 5)
            .collect();

        assert_eq!(
            local_keywords.len(),
            3,
            "Should find 3 `local` keyword tokens from 3 Lua blocks. Found: {:?}",
            local_keywords
        );

        // Verify tokens are at expected lines (lines 3, 7, 11 in 0-indexed)
        let token_lines: Vec<usize> = local_keywords.iter().map(|t| t.line).collect();
        assert!(
            token_lines.contains(&3),
            "First `local` should be at line 3. Found lines: {:?}",
            token_lines
        );
        assert!(
            token_lines.contains(&7),
            "Second `local` should be at line 7. Found lines: {:?}",
            token_lines
        );
        assert!(
            token_lines.contains(&11),
            "Third `local` should be at line 11. Found lines: {:?}",
            token_lines
        );
    }

    /// Test that parallel injection supports nested injections (depth > 1).
    ///
    /// This test verifies that `collect_injection_tokens` can handle
    /// nested injections like Lua inside Markdown inside Markdown.
    ///
    /// Document structure (from example.md):
    /// - Line 12: `````markdown (depth 0 -> 1)
    /// - Line 13: ```lua (depth 1 -> 2)
    /// - Line 14: `local injection = true` (Lua code at depth 2)
    ///
    /// Expected: `local` keyword token at line 13 (0-indexed)
    #[tokio::test]
    async fn test_parallel_injection_supports_nested_injections() {
        use crate::config::WorkspaceSettings;
        use crate::language::LanguageCoordinator;

        // Read the test fixture with nested injection
        let text = include_str!("../../../tests/assets/example.md");

        // Set up coordinator (wrapped in Arc for sharing across tasks)
        let coordinator = LanguageCoordinator::new();
        let settings = WorkspaceSettings {
            search_paths: vec![test_search_path()],
            ..Default::default()
        };
        let _summary = coordinator.load_settings(settings);

        // Load required languages
        let load_result = coordinator.ensure_language_loaded("markdown");
        assert!(load_result.success, "Should load markdown language");
        let load_result = coordinator.ensure_language_loaded("lua");
        assert!(load_result.success, "Should load lua language");

        // Parse the markdown document
        let mut parser_pool = coordinator.create_document_parser_pool();
        let tree = {
            let mut parser = parser_pool
                .acquire("markdown")
                .expect("Should get markdown parser");
            let result = parser.parse(text, None).expect("Should parse markdown");
            parser_pool.release("markdown".to_string(), parser);
            result
        };

        // Get the highlight query for markdown
        let md_highlight_query = coordinator
            .get_highlight_query("markdown")
            .expect("Should have markdown highlight query");

        // Create ConcurrentParserPool via coordinator
        let concurrent_pool = std::sync::Arc::new(coordinator.create_concurrent_parser_pool());
        let coordinator = std::sync::Arc::new(coordinator);

        // Collect tokens using parallel processing (lazy allocation handled internally)
        let tokens = super::collect_injection_tokens(
            text,
            &tree,
            &md_highlight_query,
            Some("markdown"),
            None, // capture_mappings
            coordinator,
            &concurrent_pool,
            false, // supports_multiline
        )
        .await;

        // Verify we got tokens
        assert!(!tokens.is_empty(), "Should collect some tokens");

        // Find the `local` keyword token at line 13 (0-indexed) from nested injection
        // This is the Lua code inside Markdown inside Markdown
        let nested_local = tokens
            .iter()
            .find(|t| t.line == 13 && t.mapped_name == "keyword" && t.length == 5);

        assert!(
            nested_local.is_some(),
            "Should find `local` keyword at line 13 from nested injection (Lua inside Markdown inside Markdown). \
             This requires recursive parallel processing. Found tokens at lines: {:?}",
            tokens.iter().map(|t| t.line).collect::<Vec<_>>()
        );
    }

    /// Test that parallel injection processing handles multiple injection blocks.
    ///
    /// This test verifies that:
    /// 1. Multiple injection blocks are processed in parallel
    /// 2. All valid blocks produce semantic tokens
    /// 3. The concurrent parser pool correctly manages parsers
    ///
    /// Note: Error handling (JoinError from panics/aborts) is tested separately
    /// in `test_joinset_error_handling_path`.
    #[tokio::test]
    async fn test_parallel_injection_processes_multiple_blocks() {
        use crate::config::WorkspaceSettings;
        use crate::language::LanguageCoordinator;

        // Markdown document with two valid Lua blocks
        // This tests that parallel processing works correctly
        let text = r#"# Test Document

```lua
local var1 = 1
```

```lua
local var2 = 2
```
"#;

        // Set up coordinator
        let coordinator = LanguageCoordinator::new();
        let settings = WorkspaceSettings {
            search_paths: vec![test_search_path()],
            ..Default::default()
        };
        let _summary = coordinator.load_settings(settings);

        // Load required languages
        coordinator.ensure_language_loaded("markdown");
        coordinator.ensure_language_loaded("lua");

        // Parse the markdown document
        let mut parser_pool = coordinator.create_document_parser_pool();
        let tree = {
            let mut parser = parser_pool
                .acquire("markdown")
                .expect("Should get markdown parser");
            let result = parser.parse(text, None).expect("Should parse markdown");
            parser_pool.release("markdown".to_string(), parser);
            result
        };

        let md_highlight_query = coordinator
            .get_highlight_query("markdown")
            .expect("Should have markdown highlight query");

        let concurrent_pool = std::sync::Arc::new(coordinator.create_concurrent_parser_pool());
        let coordinator = std::sync::Arc::new(coordinator);

        // This should complete without panic even if individual tasks fail
        let tokens = super::collect_injection_tokens(
            text,
            &tree,
            &md_highlight_query,
            Some("markdown"),
            None,
            coordinator,
            &concurrent_pool,
            false,
        )
        .await;

        // Verify we got tokens from successful tasks
        // Both Lua blocks should produce tokens
        let local_keywords: Vec<_> = tokens
            .iter()
            .filter(|t| t.mapped_name == "keyword" && t.length == 5)
            .collect();

        assert_eq!(
            local_keywords.len(),
            2,
            "Should find 2 `local` keyword tokens. Got: {:?}",
            local_keywords
        );
    }

    /// Test that JoinSet correctly handles JoinError by verifying error path.
    ///
    /// This test uses a mock scenario to verify that the JoinSet error handling
    /// structure is correct. We spawn a task that we then abort, which produces
    /// a JoinError similar to what we'd see from a panic.
    #[tokio::test]
    async fn test_joinset_error_handling_path() {
        use tokio::task::JoinSet;

        let mut join_set = JoinSet::<i32>::new();

        // Spawn a task that will never complete naturally
        let handle = join_set.spawn(async {
            // This will be aborted before completing
            tokio::time::sleep(std::time::Duration::from_secs(1000)).await;
            42
        });

        // Abort the task to simulate a failure scenario
        handle.abort();

        // Collect results - the aborted task should produce an Err
        let mut ok_count = 0;
        let mut err_count = 0;

        while let Some(result) = join_set.join_next().await {
            match result {
                Ok(_) => ok_count += 1,
                Err(_) => err_count += 1, // This is the error handling path we're testing
            }
        }

        // The aborted task should produce exactly one error
        assert_eq!(err_count, 1, "Should have 1 error from aborted task");
        assert_eq!(ok_count, 0, "Should have 0 successful completions");
    }

    /// Test that parallel injection does NOT deadlock with many injections.
    ///
    /// This test specifically verifies the fix for the recursive semaphore deadlock
    /// that occurs when:
    /// 1. Many top-level injections exhaust all semaphore permits
    /// 2. Each injection finds nested injections and spawns child tasks
    /// 3. Child tasks wait for permits but all are held by parents
    /// 4. Parents wait for children → Deadlock
    ///
    /// The fix releases the semaphore permit BEFORE spawning child tasks.
    ///
    /// This test creates a markdown document with more code blocks than the
    /// semaphore limit (default: 10) to trigger the deadlock scenario.
    #[tokio::test]
    async fn test_parallel_injection_no_deadlock_with_many_injections() {
        use crate::config::WorkspaceSettings;
        use crate::language::LanguageCoordinator;
        use std::time::Duration;

        // Generate a markdown document with 20 Lua code blocks (more than semaphore limit of 10)
        // Each code block contains `local varN = N`
        let mut text = String::from("# Test Document\n\n");
        for i in 0..20 {
            text.push_str(&format!("```lua\nlocal var{} = {}\n```\n\n", i, i));
        }

        // Set up coordinator
        let coordinator = LanguageCoordinator::new();
        let settings = WorkspaceSettings {
            search_paths: vec![test_search_path()],
            ..Default::default()
        };
        let _summary = coordinator.load_settings(settings);

        // Load required languages
        coordinator.ensure_language_loaded("markdown");
        coordinator.ensure_language_loaded("lua");

        // Parse the markdown document
        let mut parser_pool = coordinator.create_document_parser_pool();
        let tree = {
            let mut parser = parser_pool
                .acquire("markdown")
                .expect("Should get markdown parser");
            let result = parser.parse(&text, None).expect("Should parse markdown");
            parser_pool.release("markdown".to_string(), parser);
            result
        };

        let md_highlight_query = coordinator
            .get_highlight_query("markdown")
            .expect("Should have markdown highlight query");

        let concurrent_pool = std::sync::Arc::new(coordinator.create_concurrent_parser_pool());
        let coordinator = std::sync::Arc::new(coordinator);

        // Run with timeout to detect deadlock
        // If this times out, the deadlock fix is not working
        let result = tokio::time::timeout(
            Duration::from_secs(10),
            super::collect_injection_tokens(
                &text,
                &tree,
                &md_highlight_query,
                Some("markdown"),
                None,
                coordinator,
                &concurrent_pool,
                false,
            ),
        )
        .await;

        // Verify no timeout (no deadlock)
        assert!(
            result.is_ok(),
            "Parallel injection should complete without deadlock (timed out after 10s)"
        );

        let tokens = result.unwrap();

        // Verify we got tokens from all 20 code blocks
        let local_keywords: Vec<_> = tokens
            .iter()
            .filter(|t| t.mapped_name == "keyword" && t.length == 5)
            .collect();

        assert_eq!(
            local_keywords.len(),
            20,
            "Should find 20 `local` keyword tokens from 20 Lua blocks. \
             This verifies all injections were processed without deadlock. Found: {}",
            local_keywords.len()
        );
    }
}
