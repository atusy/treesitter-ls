//! Benchmark for semantic token processing with multiple injection blocks.
//!
//! This benchmark measures injection processing performance with different
//! concurrency levels. It uses a markdown document with multiple Lua code blocks.
//!
//! Benchmarks:
//! - `sequential`: Concurrency limit of 1 (baseline for comparison)
//! - `parallel`: Default concurrency limit (10 concurrent parsers)

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use kakehashi::analysis::handle_semantic_tokens_full;
use kakehashi::config::WorkspaceSettings;
use kakehashi::language::LanguageCoordinator;
use std::sync::Arc;

/// Returns the search path for tree-sitter grammars.
/// Uses TREE_SITTER_GRAMMARS env var if set (Nix), otherwise falls back to deps/tree-sitter.
fn test_search_path() -> String {
    std::env::var("TREE_SITTER_GRAMMARS").unwrap_or_else(|_| "deps/tree-sitter".to_string())
}

/// Generate a markdown document with N Lua code blocks
fn generate_markdown_with_lua_blocks(num_blocks: usize) -> String {
    let mut content = String::from("# Document with multiple injection blocks\n\n");

    for i in 0..num_blocks {
        content.push_str(&format!(
            r#"## Section {i}

```lua
local var_{i} = {i}
local function func_{i}()
    return var_{i} * 2
end
print(func_{i}())
```

"#
        ));
    }

    content
}

fn setup_coordinator() -> LanguageCoordinator {
    let coordinator = LanguageCoordinator::new();
    let settings = WorkspaceSettings {
        search_paths: vec![test_search_path()],
        ..Default::default()
    };
    let _summary = coordinator.load_settings(settings);

    // Load required languages
    let load_result = coordinator.ensure_language_loaded("markdown");
    if !load_result.success {
        panic!("Failed to load markdown language - check TREE_SITTER_GRAMMARS");
    }

    let load_result = coordinator.ensure_language_loaded("lua");
    if !load_result.success {
        panic!("Failed to load lua language - check TREE_SITTER_GRAMMARS");
    }

    coordinator
}

fn benchmark_injection_processing(c: &mut Criterion) {
    // Wrap coordinator in Arc for sharing with parallel handler
    let coordinator = Arc::new(setup_coordinator());

    let mut group = c.benchmark_group("injection_processing");

    // Test with different numbers of injection blocks
    for num_blocks in [1, 3, 5, 10].iter() {
        let text = generate_markdown_with_lua_blocks(*num_blocks);
        let mut parser_pool = coordinator.create_document_parser_pool();

        // Pre-parse the document (we're benchmarking token processing, not parsing)
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

        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");

        // Sequential benchmark (concurrency limit = 1) as baseline
        let sequential_pool = Arc::new(coordinator.create_concurrent_parser_pool_with_limit(1));

        group.bench_with_input(
            BenchmarkId::new("sequential", num_blocks),
            &(*num_blocks, &text, &tree),
            |b, (_, text, tree)| {
                b.iter(|| {
                    rt.block_on(handle_semantic_tokens_full(
                        text,
                        tree,
                        &md_highlight_query,
                        Some("markdown"),
                        None,
                        Arc::clone(&coordinator),
                        &sequential_pool,
                        false, // supports_multiline
                    ))
                })
            },
        );

        // Parallel benchmark using default concurrency (10 concurrent parsers)
        let parallel_pool = Arc::new(coordinator.create_concurrent_parser_pool());

        group.bench_with_input(
            BenchmarkId::new("parallel", num_blocks),
            &(*num_blocks, &text, &tree),
            |b, (_, text, tree)| {
                b.iter(|| {
                    rt.block_on(handle_semantic_tokens_full(
                        text,
                        tree,
                        &md_highlight_query,
                        Some("markdown"),
                        None,
                        Arc::clone(&coordinator),
                        &parallel_pool,
                        false, // supports_multiline
                    ))
                })
            },
        );
    }

    group.finish();
}

criterion_group!(benches, benchmark_injection_processing);
criterion_main!(benches);
