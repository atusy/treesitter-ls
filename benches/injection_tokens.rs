//! Benchmark for injection-aware tokenization performance.
//!
//! Tests semantic token generation performance for markdown documents
//! with multiple code block injections.
//!
//! Run with: cargo bench --bench injection_tokens

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use tree_sitter::{Parser, Query, StreamingIterator};

/// Create a markdown document with N code blocks.
fn create_injection_document(num_blocks: usize) -> String {
    let mut doc = String::from("# Benchmark Document\n\n");
    doc.push_str("This document tests injection-aware tokenization performance.\n\n");

    for i in 0..num_blocks {
        // Alternate between different languages
        let (lang, code) = match i % 3 {
            0 => {
                let code = format!(
                    "-- Block {i}\n\
                     local function example{i}()\n\
                     \tprint(\"Hello from Lua block {i}\")\n\
                     \tlocal x = {i}\n\
                     \treturn x * 2\n\
                     end\n"
                );
                ("lua", code)
            }
            1 => {
                let code = format!(
                    "# Block {i}\n\
                     def example{i}():\n\
                     \t\"\"\"Python example function.\"\"\"\n\
                     \tx = {i}\n\
                     \tprint(f\"Hello from Python block\")\n\
                     \treturn x * 2\n"
                );
                ("python", code)
            }
            _ => {
                let code = format!(
                    "// Block {i}\n\
                     fn example{i}() -> i32 {{\n\
                     \tlet x = {i};\n\
                     \tprintln!(\"Hello from Rust block\");\n\
                     \tx * 2\n\
                     }}\n"
                );
                ("rust", code)
            }
        };

        doc.push_str(&format!("## Section {}\n\n", i + 1));
        doc.push_str("Some explanation text before the code block.\n\n");
        doc.push_str(&format!("```{}\n{}\n```\n\n", lang, code));
        doc.push_str("And some text after the code block.\n\n");
    }

    doc
}

/// Benchmark full tokenization of markdown with code blocks.
fn bench_full_tokenization(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_tokenization");

    // Test with different numbers of code blocks
    for num_blocks in [3, 5, 10] {
        let doc = create_injection_document(num_blocks);

        // Parse the markdown document
        let mut parser = Parser::new();
        let md_language: tree_sitter::Language = tree_sitter_md::LANGUAGE.into();
        parser.set_language(&md_language).expect("set markdown");

        // Create highlight query (simplified for benchmark)
        let highlight_query_str = r#"
            (atx_heading) @markup.heading
            (paragraph) @text
            (fenced_code_block) @markup.raw.block
            (code_fence_content) @markup.raw
        "#;
        let highlight_query = Query::new(&md_language, highlight_query_str).expect("valid query");

        group.bench_with_input(
            BenchmarkId::new("blocks", num_blocks),
            &doc,
            |b, doc_text| {
                b.iter(|| {
                    // Parse document
                    let tree = parser.parse(doc_text, None).expect("parse");

                    // Run query to collect matches (simulates tokenization)
                    // Note: QueryMatches implements StreamingIterator, not Iterator
                    let mut cursor = tree_sitter::QueryCursor::new();
                    let mut matches =
                        cursor.matches(&highlight_query, tree.root_node(), doc_text.as_bytes());
                    let mut count = 0;
                    while matches.next().is_some() {
                        count += 1;
                    }

                    black_box(count)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark incremental parsing after edit.
fn bench_incremental_parse(c: &mut Criterion) {
    let mut group = c.benchmark_group("incremental_parse");

    let doc = create_injection_document(5);

    // Parse initial document
    let mut parser = Parser::new();
    let md_language: tree_sitter::Language = tree_sitter_md::LANGUAGE.into();
    parser.set_language(&md_language).expect("set markdown");
    let initial_tree = parser.parse(&doc, None).expect("parse");

    // Edit scenarios
    let header_edit_pos = doc.find("# Benchmark").unwrap();
    let code_edit_pos = doc.find("print(\"Hello").unwrap_or(50);

    group.bench_function("edit_header", |b| {
        b.iter(|| {
            // Simulate edit in header (outside injections)
            let mut edited = doc.clone();
            edited.insert_str(header_edit_pos + 12, " MODIFIED");

            // Create edit info
            let mut edit_tree = initial_tree.clone();
            edit_tree.edit(&tree_sitter::InputEdit {
                start_byte: header_edit_pos + 12,
                old_end_byte: header_edit_pos + 12,
                new_end_byte: header_edit_pos + 12 + 9,
                start_position: tree_sitter::Point::new(0, header_edit_pos + 12),
                old_end_position: tree_sitter::Point::new(0, header_edit_pos + 12),
                new_end_position: tree_sitter::Point::new(0, header_edit_pos + 12 + 9),
            });

            // Incremental parse
            let new_tree = parser.parse(&edited, Some(&edit_tree)).expect("reparse");
            black_box(new_tree.root_node().child_count())
        });
    });

    group.bench_function("edit_code_block", |b| {
        b.iter(|| {
            // Simulate edit inside code block (inside injection)
            let mut edited = doc.clone();
            edited.insert_str(code_edit_pos + 6, "MODIFIED_");

            // Create edit info
            let mut edit_tree = initial_tree.clone();
            edit_tree.edit(&tree_sitter::InputEdit {
                start_byte: code_edit_pos + 6,
                old_end_byte: code_edit_pos + 6,
                new_end_byte: code_edit_pos + 6 + 9,
                start_position: tree_sitter::Point::new(5, 6),
                old_end_position: tree_sitter::Point::new(5, 6),
                new_end_position: tree_sitter::Point::new(5, 15),
            });

            // Incremental parse
            let new_tree = parser.parse(&edited, Some(&edit_tree)).expect("reparse");
            black_box(new_tree.root_node().child_count())
        });
    });

    group.finish();
}

criterion_group!(benches, bench_full_tokenization, bench_incremental_parse);
criterion_main!(benches);
