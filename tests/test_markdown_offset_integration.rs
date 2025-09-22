use tree_sitter::Query;
use treesitter_ls::language::injection::{
    parse_offset_directive, parse_offset_directive_for_pattern,
};

/// This integration test verifies that the pattern-aware offset parsing
/// correctly handles real markdown injection queries with both fenced code
/// blocks (no offset) and frontmatter (with offset).
#[test]
fn test_markdown_injection_offsets_real_world() {
    // This is a simplified version of the actual markdown injection query
    // from nvim-treesitter, containing the patterns that caused the bug
    let markdown_injection_query = r#"
        ; Pattern for fenced code blocks - NO OFFSET
        (fenced_code_block
          (info_string
            (language) @injection.language)
          (code_fence_content) @injection.content)

        ; Pattern for YAML frontmatter - HAS OFFSET
        ((minus_metadata) @injection.content
          (#set! injection.language "yaml")
          (#offset! @injection.content 1 0 -1 0)
          (#set! injection.include-children))

        ; Pattern for TOML frontmatter - HAS OFFSET
        ((plus_metadata) @injection.content
          (#set! injection.language "toml")
          (#offset! @injection.content 1 0 -1 0)
          (#set! injection.include-children))
    "#;

    // Parse the query with markdown grammar
    let language = tree_sitter_md::LANGUAGE.into();
    let query = Query::new(&language, markdown_injection_query)
        .expect("Failed to parse markdown injection query");

    // The old broken function would return the first offset found (frontmatter offset)
    // even when processing a fenced code block
    let first_offset = parse_offset_directive(&query);
    assert!(
        first_offset.is_some(),
        "Should find at least one offset in the query"
    );

    // The broken behavior: returns (1, 0, -1, 0) from frontmatter
    let offset = first_offset.unwrap();
    assert_eq!(
        offset.start_row, 1,
        "Old function returns frontmatter offset"
    );

    // Now test the pattern-aware function
    // We need to find which patterns correspond to which injection type
    let pattern_count = query.pattern_count();

    let mut found_fenced_code_pattern = false;
    let mut found_frontmatter_pattern = false;

    for pattern_idx in 0..pattern_count {
        let offset = parse_offset_directive_for_pattern(&query, pattern_idx);

        if offset.is_none() {
            // This should be the fenced_code_block pattern
            found_fenced_code_pattern = true;
        } else if let Some(off) = offset {
            // This should be a frontmatter pattern
            if off.start_row == 1 && off.end_row == -1 {
                found_frontmatter_pattern = true;
            }
        }
    }

    assert!(
        found_fenced_code_pattern,
        "Should find at least one pattern without offset (fenced code block)"
    );
    assert!(
        found_frontmatter_pattern,
        "Should find at least one pattern with (1, 0, -1, 0) offset (frontmatter)"
    );
}

/// Test that verifies the fix: fenced code blocks get no offset,
/// frontmatter gets the correct offset
#[test]
fn test_pattern_specific_offsets() {
    // Create a minimal query with two patterns to test the fix
    let query_str = r#"
        ; Pattern 0: Code blocks - NO OFFSET
        ((fenced_code_block
          (code_fence_content) @injection.content)
         (#set! injection.language "test"))

        ; Pattern 1: Metadata - HAS OFFSET
        ((minus_metadata) @injection.content
         (#set! injection.language "yaml")
         (#offset! @injection.content 1 0 -1 0))
    "#;

    let language = tree_sitter_md::LANGUAGE.into();
    let query = Query::new(&language, query_str).expect("Failed to create query");

    // Pattern 0 should have no offset
    let offset_0 = parse_offset_directive_for_pattern(&query, 0);
    assert_eq!(
        offset_0, None,
        "Pattern 0 (code block) should have no offset"
    );

    // Find the pattern with offset
    let mut found_offset_pattern = false;
    for i in 0..query.pattern_count() {
        if let Some(offset) = parse_offset_directive_for_pattern(&query, i) {
            assert_eq!(
                offset.start_row, 1,
                "Frontmatter pattern should have start_row offset of 1"
            );
            assert_eq!(
                offset.end_row, -1,
                "Frontmatter pattern should have end_row offset of -1"
            );
            found_offset_pattern = true;
        }
    }

    assert!(
        found_offset_pattern,
        "Should find the frontmatter pattern with offset"
    );
}
