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
