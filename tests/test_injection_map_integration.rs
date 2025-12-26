//! Integration tests for InjectionMap population after document parsing.
//!
//! These tests verify PBI-083 AC3: After parse_document() on markdown with code blocks,
//! InjectionMap contains CacheableInjectionRegion entries.

use tree_sitter::{Parser, Query};
use treesitter_ls::analysis::{InjectionMap, next_result_id};
use treesitter_ls::language::injection::{CacheableInjectionRegion, collect_all_injections};
use url::Url;

/// Helper to populate injection map from a parsed tree (simulates parse_document behavior).
///
/// This extracts the logic that should run after parsing to populate the InjectionMap.
fn populate_injection_map(
    injection_map: &InjectionMap,
    uri: &Url,
    text: &str,
    tree: &tree_sitter::Tree,
    injection_query: Option<&Query>,
) {
    // Collect all injection regions from the parsed tree
    if let Some(regions) = collect_all_injections(&tree.root_node(), text, injection_query) {
        // Convert to CacheableInjectionRegion with unique result_ids
        let cacheable_regions: Vec<CacheableInjectionRegion> = regions
            .iter()
            .map(|info| CacheableInjectionRegion::from_region_info(info, &next_result_id()))
            .collect();

        // Store in injection map
        injection_map.insert(uri.clone(), cacheable_regions);
    }
}

#[test]
fn test_injection_map_populated_after_parse_markdown_with_code_blocks() {
    // AC3: After parse_document() on markdown with code blocks,
    // InjectionMap contains CacheableInjectionRegion entries

    let injection_map = InjectionMap::new();
    let uri = Url::parse("file:///test/example.md").unwrap();

    // Markdown document with two code blocks
    let markdown_text = r#"# Example

```lua
print("hello")
```

Some text.

```python
def foo():
    pass
```
"#;

    // Parse with markdown parser
    let mut parser = Parser::new();
    let md_language: tree_sitter::Language = tree_sitter_md::LANGUAGE.into();
    parser.set_language(&md_language).expect("set markdown");
    let tree = parser.parse(markdown_text, None).expect("parse markdown");

    // Create injection query for markdown code blocks
    // Using nvim-treesitter style injection with set-lang-from-info-string
    let injection_query_str = r#"
        (fenced_code_block
          (info_string
            (language) @_lang)
          (code_fence_content) @injection.content
          (#set-lang-from-info-string! @_lang))
    "#;
    let injection_query =
        Query::new(&md_language, injection_query_str).expect("valid injection query");

    // Before population, injection map should be empty for this URI
    assert!(
        injection_map.get(&uri).is_none(),
        "InjectionMap should be empty before population"
    );

    // Populate the injection map (simulates what parse_document should do)
    populate_injection_map(
        &injection_map,
        &uri,
        markdown_text,
        &tree,
        Some(&injection_query),
    );

    // After population, injection map should contain entries
    let regions = injection_map.get(&uri);
    assert!(
        regions.is_some(),
        "InjectionMap should contain regions after population"
    );

    let regions = regions.unwrap();
    assert_eq!(
        regions.len(),
        2,
        "Should have 2 injection regions (lua and python code blocks)"
    );

    // Verify the first region (lua)
    let lua_region = regions.iter().find(|r| r.language == "lua");
    assert!(lua_region.is_some(), "Should have a lua injection region");
    let lua_region = lua_region.unwrap();
    assert!(
        lua_region.byte_range.start > 0,
        "Lua region should have valid byte range"
    );
    assert!(
        !lua_region.result_id.is_empty(),
        "Lua region should have a result_id"
    );

    // Verify the second region (python)
    let python_region = regions.iter().find(|r| r.language == "python");
    assert!(
        python_region.is_some(),
        "Should have a python injection region"
    );
    let python_region = python_region.unwrap();
    assert!(
        python_region.byte_range.start > lua_region.byte_range.end,
        "Python region should come after lua region"
    );
}

#[test]
fn test_injection_map_empty_when_no_injections() {
    // Edge case: Document with no code blocks should not populate injection map

    let injection_map = InjectionMap::new();
    let uri = Url::parse("file:///test/no_code.md").unwrap();

    // Markdown without code blocks
    let markdown_text = r#"# Just a Header

Some plain text without any code blocks.

- A list item
- Another item
"#;

    // Parse with markdown parser
    let mut parser = Parser::new();
    let md_language: tree_sitter::Language = tree_sitter_md::LANGUAGE.into();
    parser.set_language(&md_language).expect("set markdown");
    let tree = parser.parse(markdown_text, None).expect("parse markdown");

    // Create injection query
    let injection_query_str = r#"
        (fenced_code_block
          (info_string
            (language) @_lang)
          (code_fence_content) @injection.content
          (#set-lang-from-info-string! @_lang))
    "#;
    let injection_query =
        Query::new(&md_language, injection_query_str).expect("valid injection query");

    // Populate (should find no injections)
    populate_injection_map(
        &injection_map,
        &uri,
        markdown_text,
        &tree,
        Some(&injection_query),
    );

    // InjectionMap should remain empty (no insert since no regions found)
    // Note: The implementation might insert an empty Vec - both behaviors are acceptable
    // as long as we can detect "no injections"
    let regions = injection_map.get(&uri);
    if let Some(r) = regions {
        assert!(r.is_empty(), "Should have no injection regions");
    }
    // If None, that's also acceptable (no injections found)
}

#[test]
fn test_injection_map_contains_byte_ranges_for_invalidation() {
    // AC4/AC5 preparation: Verify that regions have correct byte ranges
    // for contains_byte() checks during edit invalidation

    let injection_map = InjectionMap::new();
    let uri = Url::parse("file:///test/ranges.md").unwrap();

    // Document with specific structure for byte range testing
    let markdown_text = "# Header\n\n```lua\nprint(1)\n```\n";
    //                   0         1         2         3
    //                   0123456789012345678901234567890123456789

    // Parse
    let mut parser = Parser::new();
    let md_language: tree_sitter::Language = tree_sitter_md::LANGUAGE.into();
    parser.set_language(&md_language).expect("set markdown");
    let tree = parser.parse(markdown_text, None).expect("parse");

    // Injection query
    let injection_query_str = r#"
        (fenced_code_block
          (info_string
            (language) @_lang)
          (code_fence_content) @injection.content
          (#set-lang-from-info-string! @_lang))
    "#;
    let injection_query = Query::new(&md_language, injection_query_str).expect("query");

    // Populate
    populate_injection_map(
        &injection_map,
        &uri,
        markdown_text,
        &tree,
        Some(&injection_query),
    );

    let regions = injection_map.get(&uri).expect("should have regions");
    assert_eq!(regions.len(), 1, "Should have one lua region");

    let region = &regions[0];

    // Verify byte range is captured correctly
    // The code_fence_content should start after "```lua\n"
    // "# Header\n\n```lua\n" = 10 + 7 = 17 bytes before content
    assert!(
        region.byte_range.start >= 10,
        "Region should start after header: got {}",
        region.byte_range.start
    );

    // Test contains_byte for invalidation scenarios
    let content_middle = (region.byte_range.start + region.byte_range.end) / 2;
    assert!(
        region.contains_byte(content_middle),
        "Should contain byte in middle of range"
    );

    assert!(
        !region.contains_byte(0),
        "Should not contain byte at document start (header)"
    );

    assert!(
        !region.contains_byte(region.byte_range.end + 10),
        "Should not contain byte after range"
    );
}

// ===========================================================================
// AC4 Tests: Edit outside injection preserves cache
// ===========================================================================

/// Helper to check if an edit range overlaps any injection region.
///
/// This simulates the logic that should be in did_change handler.
fn edit_overlaps_injection(
    regions: &[CacheableInjectionRegion],
    edit_start: usize,
    edit_end: usize,
) -> Vec<String> {
    regions
        .iter()
        .filter(|r| {
            // Check if edit range overlaps with region's byte range
            // Overlap occurs when: edit_start < region_end AND edit_end > region_start
            edit_start < r.byte_range.end && edit_end > r.byte_range.start
        })
        .map(|r| r.result_id.clone())
        .collect()
}

#[test]
fn test_edit_outside_injection_preserves_all_caches() {
    // AC4: Edit host document text (line 0), verify InjectionTokenCache entries unchanged

    let injection_map = InjectionMap::new();
    let injection_token_cache = treesitter_ls::analysis::InjectionTokenCache::new();
    let uri = Url::parse("file:///test/edit_outside.md").unwrap();

    // Document structure:
    // Line 0: "# Header\n"          (bytes 0-9)
    // Line 1: "\n"                   (byte 10)
    // Line 2-4: "```lua\nprint(1)\n```\n" (bytes 11-29)
    // Line 5: "\n"                   (byte 30)
    // Line 6: "Footer text\n"        (bytes 31-43)
    let markdown_text = "# Header\n\n```lua\nprint(1)\n```\n\nFooter text\n";

    // Parse
    let mut parser = Parser::new();
    let md_language: tree_sitter::Language = tree_sitter_md::LANGUAGE.into();
    parser.set_language(&md_language).expect("set markdown");
    let tree = parser.parse(markdown_text, None).expect("parse");

    // Injection query
    let injection_query_str = r#"
        (fenced_code_block
          (info_string
            (language) @_lang)
          (code_fence_content) @injection.content
          (#set-lang-from-info-string! @_lang))
    "#;
    let injection_query = Query::new(&md_language, injection_query_str).expect("query");

    // Populate injection map
    populate_injection_map(
        &injection_map,
        &uri,
        markdown_text,
        &tree,
        Some(&injection_query),
    );

    let regions = injection_map.get(&uri).expect("should have regions");
    assert_eq!(regions.len(), 1, "Should have one lua region");

    // Store tokens for the lua injection
    let lua_region = &regions[0];
    let lua_tokens = tower_lsp::lsp_types::SemanticTokens {
        result_id: Some("lua-tokens-1".to_string()),
        data: vec![tower_lsp::lsp_types::SemanticToken {
            delta_line: 0,
            delta_start: 0,
            length: 5,
            token_type: 0,
            token_modifiers_bitset: 0,
        }],
    };
    injection_token_cache.store(&uri, &lua_region.result_id, lua_tokens);

    // Simulate edit to header (line 0, bytes 0-8) - OUTSIDE injection
    let edit_start = 0;
    let edit_end = 8; // "# Header" (before newline)

    // Check which regions overlap with the edit
    let overlapping_regions = edit_overlaps_injection(&regions, edit_start, edit_end);
    assert!(
        overlapping_regions.is_empty(),
        "Edit in header should not overlap any injection region"
    );

    // Since no regions overlap, injection_token_cache should remain unchanged
    // In real implementation, we would NOT call injection_token_cache.remove() for any region
    let cached = injection_token_cache.get(&uri, &lua_region.result_id);
    assert!(
        cached.is_some(),
        "Lua tokens should still be cached after edit outside injection"
    );
}

#[test]
fn test_edit_in_footer_preserves_all_caches() {
    // AC4 variant: Edit in footer (after all injections)

    let injection_map = InjectionMap::new();
    let injection_token_cache = treesitter_ls::analysis::InjectionTokenCache::new();
    let uri = Url::parse("file:///test/edit_footer.md").unwrap();

    let markdown_text = "# Header\n\n```lua\nprint(1)\n```\n\nFooter text\n";

    let mut parser = Parser::new();
    let md_language: tree_sitter::Language = tree_sitter_md::LANGUAGE.into();
    parser.set_language(&md_language).expect("set markdown");
    let tree = parser.parse(markdown_text, None).expect("parse");

    let injection_query_str = r#"
        (fenced_code_block
          (info_string
            (language) @_lang)
          (code_fence_content) @injection.content
          (#set-lang-from-info-string! @_lang))
    "#;
    let injection_query = Query::new(&md_language, injection_query_str).expect("query");

    populate_injection_map(
        &injection_map,
        &uri,
        markdown_text,
        &tree,
        Some(&injection_query),
    );

    let regions = injection_map.get(&uri).expect("should have regions");
    let lua_region = &regions[0];

    // Store tokens
    let lua_tokens = tower_lsp::lsp_types::SemanticTokens {
        result_id: Some("lua-tokens-2".to_string()),
        data: vec![],
    };
    injection_token_cache.store(&uri, &lua_region.result_id, lua_tokens);

    // Simulate edit to footer (after all code blocks)
    let footer_start = markdown_text.find("Footer").unwrap();
    let footer_end = footer_start + 6; // "Footer"

    let overlapping_regions = edit_overlaps_injection(&regions, footer_start, footer_end);
    assert!(
        overlapping_regions.is_empty(),
        "Edit in footer should not overlap any injection region"
    );

    // Cache should be preserved
    let cached = injection_token_cache.get(&uri, &lua_region.result_id);
    assert!(
        cached.is_some(),
        "Lua tokens should still be cached after edit in footer"
    );
}

// ===========================================================================
// AC5 Tests: Edit inside injection invalidates only that region
// ===========================================================================

#[test]
fn test_edit_inside_injection_invalidates_only_that_region() {
    // AC5: Edit inside code block (injection region), verify only that region_id is removed

    let injection_map = InjectionMap::new();
    let injection_token_cache = treesitter_ls::analysis::InjectionTokenCache::new();
    let uri = Url::parse("file:///test/edit_inside.md").unwrap();

    // Document with two code blocks
    let markdown_text = r#"# Example

```lua
print("hello")
```

```python
def foo():
    pass
```
"#;

    let mut parser = Parser::new();
    let md_language: tree_sitter::Language = tree_sitter_md::LANGUAGE.into();
    parser.set_language(&md_language).expect("set markdown");
    let tree = parser.parse(markdown_text, None).expect("parse");

    let injection_query_str = r#"
        (fenced_code_block
          (info_string
            (language) @_lang)
          (code_fence_content) @injection.content
          (#set-lang-from-info-string! @_lang))
    "#;
    let injection_query = Query::new(&md_language, injection_query_str).expect("query");

    populate_injection_map(
        &injection_map,
        &uri,
        markdown_text,
        &tree,
        Some(&injection_query),
    );

    let regions = injection_map.get(&uri).expect("should have regions");
    assert_eq!(regions.len(), 2, "Should have two injection regions");

    // Find lua and python regions
    let lua_region = regions.iter().find(|r| r.language == "lua").unwrap();
    let python_region = regions.iter().find(|r| r.language == "python").unwrap();

    // Store tokens for both
    let lua_tokens = tower_lsp::lsp_types::SemanticTokens {
        result_id: Some("lua-tokens".to_string()),
        data: vec![],
    };
    let python_tokens = tower_lsp::lsp_types::SemanticTokens {
        result_id: Some("python-tokens".to_string()),
        data: vec![],
    };
    injection_token_cache.store(&uri, &lua_region.result_id, lua_tokens);
    injection_token_cache.store(&uri, &python_region.result_id, python_tokens);

    // Verify both are cached
    assert!(
        injection_token_cache
            .get(&uri, &lua_region.result_id)
            .is_some()
    );
    assert!(
        injection_token_cache
            .get(&uri, &python_region.result_id)
            .is_some()
    );

    // Simulate edit inside lua code block
    let lua_edit_start = lua_region.byte_range.start + 2; // Somewhere inside lua
    let lua_edit_end = lua_edit_start + 5;

    // Determine which regions to invalidate
    let overlapping_regions = edit_overlaps_injection(&regions, lua_edit_start, lua_edit_end);
    assert_eq!(
        overlapping_regions.len(),
        1,
        "Edit should overlap exactly one region"
    );
    assert_eq!(
        overlapping_regions[0], lua_region.result_id,
        "Should overlap lua region only"
    );

    // Invalidate only overlapping regions (simulates did_change behavior)
    for region_id in &overlapping_regions {
        injection_token_cache.remove(&uri, region_id);
    }

    // Verify: lua cache is gone, python cache is preserved
    assert!(
        injection_token_cache
            .get(&uri, &lua_region.result_id)
            .is_none(),
        "Lua tokens should be invalidated after edit inside lua block"
    );
    assert!(
        injection_token_cache
            .get(&uri, &python_region.result_id)
            .is_some(),
        "Python tokens should be preserved after edit inside lua block"
    );
}
