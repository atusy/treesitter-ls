//! Benchmark for semantic token injection processing.
//!
//! This benchmark measures the performance of Rayon-based parallel injection processing
//! for documents with varying numbers of code blocks.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use kakehashi::analysis::semantic::{
    handle_semantic_tokens_full, handle_semantic_tokens_full_parallel,
};
use kakehashi::config::WorkspaceSettings;
use kakehashi::language::LanguageCoordinator;

/// Returns the search path for tree-sitter grammars.
fn search_path() -> String {
    std::env::var("TREE_SITTER_GRAMMARS").unwrap_or_else(|_| "deps/tree-sitter".to_string())
}

/// Generate a Markdown document with N Lua code blocks.
fn generate_markdown_with_injections(num_blocks: usize) -> String {
    let mut doc = String::with_capacity(num_blocks * 50);
    doc.push_str("# Benchmark Document\n\n");

    for i in 0..num_blocks {
        doc.push_str(&format!(
            "## Section {}\n\n```lua\nlocal var_{} = {}\n```\n\n",
            i, i, i
        ));
    }

    doc
}

/// Set up coordinator with markdown and lua languages loaded.
fn setup_coordinator() -> Option<LanguageCoordinator> {
    let coordinator = LanguageCoordinator::new();
    let settings = WorkspaceSettings {
        search_paths: vec![search_path()],
        ..Default::default()
    };
    coordinator.load_settings(settings);

    // Load required languages
    let md_result = coordinator.ensure_language_loaded("markdown");
    let lua_result = coordinator.ensure_language_loaded("lua");

    if md_result.success && lua_result.success {
        Some(coordinator)
    } else {
        None
    }
}

fn benchmark_injection_processing(c: &mut Criterion) {
    let Some(coordinator) = setup_coordinator() else {
        eprintln!("Skipping benchmark: Could not load markdown/lua parsers");
        return;
    };

    let Some(query) = coordinator.get_highlight_query("markdown") else {
        eprintln!("Skipping benchmark: Could not get markdown highlight query");
        return;
    };

    let mut parser_pool = coordinator.create_document_parser_pool();

    let mut group = c.benchmark_group("injection_processing");

    // Test with different numbers of code blocks
    for num_blocks in [10, 50, 100, 500].iter() {
        let doc = generate_markdown_with_injections(*num_blocks);

        // Parse the document once
        let tree = {
            let mut parser = parser_pool
                .acquire("markdown")
                .expect("Should get markdown parser");
            let result = parser.parse(&doc, None).expect("Should parse markdown");
            parser_pool.release("markdown".to_string(), parser);
            result
        };

        // Benchmark sequential (non-parallel) implementation
        group.bench_with_input(
            BenchmarkId::new("sequential", num_blocks),
            &(&doc, &tree),
            |b, (doc, tree)| {
                b.iter(|| {
                    let mut pool = coordinator.create_document_parser_pool();
                    handle_semantic_tokens_full(
                        doc,
                        tree,
                        &query,
                        Some("markdown"),
                        None,
                        Some(&coordinator),
                        Some(&mut pool),
                    )
                })
            },
        );

        // Benchmark Rayon parallel implementation
        group.bench_with_input(
            BenchmarkId::new("rayon_parallel", num_blocks),
            &(&doc, &tree),
            |b, (doc, tree)| {
                b.iter(|| {
                    handle_semantic_tokens_full_parallel(
                        doc,
                        tree,
                        &query,
                        Some("markdown"),
                        None,
                        &coordinator,
                        false,
                    )
                })
            },
        );
    }

    group.finish();
}

fn benchmark_large_document(c: &mut Criterion) {
    let Some(coordinator) = setup_coordinator() else {
        eprintln!("Skipping benchmark: Could not load markdown/lua parsers");
        return;
    };

    let Some(query) = coordinator.get_highlight_query("markdown") else {
        eprintln!("Skipping benchmark: Could not get markdown highlight query");
        return;
    };

    let mut parser_pool = coordinator.create_document_parser_pool();

    // Generate a large document similar to the problem statement (2000+ blocks)
    let num_blocks = 2000;
    let doc = generate_markdown_with_injections(num_blocks);

    let tree = {
        let mut parser = parser_pool
            .acquire("markdown")
            .expect("Should get markdown parser");
        let result = parser.parse(&doc, None).expect("Should parse markdown");
        parser_pool.release("markdown".to_string(), parser);
        result
    };

    let mut group = c.benchmark_group("large_document");
    group.sample_size(10); // Fewer samples for large documents

    group.bench_function("2000_blocks_rayon", |b| {
        b.iter(|| {
            handle_semantic_tokens_full_parallel(
                &doc,
                &tree,
                &query,
                Some("markdown"),
                None,
                &coordinator,
                false,
            )
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    benchmark_injection_processing,
    benchmark_large_document
);
criterion_main!(benches);
