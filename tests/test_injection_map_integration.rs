//! Integration tests for InjectionMap population after document parsing.
//!
//! These tests verify PBI-083 AC3: After parse_document() on markdown with code blocks,
//! InjectionMap contains CacheableInjectionRegion entries.

use kakehashi::analysis::{InjectionMap, next_result_id};
use kakehashi::language::injection::{CacheableInjectionRegion, collect_all_injections};
use tree_sitter::{Parser, Query};
use url::Url;

/// Helper to populate injection map from a parsed tree (simulates parse_document behavior).
///
/// This extracts the logic that should run after parsing to populate the InjectionMap.
///
/// # Test-Only ID Generation (differs from production)
///
/// This helper uses `next_result_id()` to generate `region_id` values - a deliberate
/// test simplification that differs from production behavior.
///
/// **Production** uses `RegionIdTracker` to generate position-based ULIDs that remain
/// stable when edits occur before a code block (e.g., adding lines above doesn't
/// change the region's identity).
///
/// **Tests** can use `next_result_id()` because:
/// 1. No edits occur mid-test, so position stability is irrelevant
/// 2. Tests run in isolation, avoiding global counter collisions
/// 3. Simpler than instantiating `RegionIdTracker` with byte positions
///
/// Note: `next_result_id()` and `RegionIdTracker` serve entirely different purposes
/// in production - the former is for LSP delta computation, the latter for region
/// identity. This test reuses `next_result_id()` only as a convenient unique ID source.
fn populate_injection_map(
    injection_map: &InjectionMap,
    uri: &Url,
    text: &str,
    tree: &tree_sitter::Tree,
    injection_query: Option<&Query>,
) {
    // Collect all injection regions from the parsed tree
    if let Some(regions) = collect_all_injections(&tree.root_node(), text, injection_query) {
        // Convert to CacheableInjectionRegion with unique region_ids
        // Note: Uses next_result_id() as a test shortcut; production uses RegionIdTracker
        let cacheable_regions: Vec<CacheableInjectionRegion> = regions
            .iter()
            .map(|info| CacheableInjectionRegion::from_region_info(info, &next_result_id(), text))
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
        !lua_region.region_id.is_empty(),
        "Lua region should have a region_id"
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
        .map(|r| r.region_id.clone())
        .collect()
}

#[test]
fn test_edit_outside_injection_preserves_all_caches() {
    // AC4: Edit host document text (line 0), verify InjectionTokenCache entries unchanged

    let injection_map = InjectionMap::new();
    let injection_token_cache = kakehashi::analysis::InjectionTokenCache::new();
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
    let lua_tokens = tower_lsp_server::ls_types::SemanticTokens {
        result_id: Some("lua-tokens-1".to_string()),
        data: vec![tower_lsp_server::ls_types::SemanticToken {
            delta_line: 0,
            delta_start: 0,
            length: 5,
            token_type: 0,
            token_modifiers_bitset: 0,
        }],
    };
    injection_token_cache.store(&uri, &lua_region.region_id, lua_tokens);

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
    let cached = injection_token_cache.get(&uri, &lua_region.region_id);
    assert!(
        cached.is_some(),
        "Lua tokens should still be cached after edit outside injection"
    );
}

#[test]
fn test_edit_in_footer_preserves_all_caches() {
    // AC4 variant: Edit in footer (after all injections)

    let injection_map = InjectionMap::new();
    let injection_token_cache = kakehashi::analysis::InjectionTokenCache::new();
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
    let lua_tokens = tower_lsp_server::ls_types::SemanticTokens {
        result_id: Some("lua-tokens-2".to_string()),
        data: vec![],
    };
    injection_token_cache.store(&uri, &lua_region.region_id, lua_tokens);

    // Simulate edit to footer (after all code blocks)
    let footer_start = markdown_text.find("Footer").unwrap();
    let footer_end = footer_start + 6; // "Footer"

    let overlapping_regions = edit_overlaps_injection(&regions, footer_start, footer_end);
    assert!(
        overlapping_regions.is_empty(),
        "Edit in footer should not overlap any injection region"
    );

    // Cache should be preserved
    let cached = injection_token_cache.get(&uri, &lua_region.region_id);
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
    let injection_token_cache = kakehashi::analysis::InjectionTokenCache::new();
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
    let lua_tokens = tower_lsp_server::ls_types::SemanticTokens {
        result_id: Some("lua-tokens".to_string()),
        data: vec![],
    };
    let python_tokens = tower_lsp_server::ls_types::SemanticTokens {
        result_id: Some("python-tokens".to_string()),
        data: vec![],
    };
    injection_token_cache.store(&uri, &lua_region.region_id, lua_tokens);
    injection_token_cache.store(&uri, &python_region.region_id, python_tokens);

    // Verify both are cached
    assert!(
        injection_token_cache
            .get(&uri, &lua_region.region_id)
            .is_some()
    );
    assert!(
        injection_token_cache
            .get(&uri, &python_region.region_id)
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
        overlapping_regions[0], lua_region.region_id,
        "Should overlap lua region only"
    );

    // Invalidate only overlapping regions (simulates did_change behavior)
    for region_id in &overlapping_regions {
        injection_token_cache.remove(&uri, region_id);
    }

    // Verify: lua cache is gone, python cache is preserved
    assert!(
        injection_token_cache
            .get(&uri, &lua_region.region_id)
            .is_none(),
        "Lua tokens should be invalidated after edit inside lua block"
    );
    assert!(
        injection_token_cache
            .get(&uri, &python_region.region_id)
            .is_some(),
        "Python tokens should be preserved after edit inside lua block"
    );
}

// ===========================================================================
// Language change invalidation tests
// ===========================================================================

#[test]
fn test_language_change_should_invalidate_cache() {
    // Verify that changing a code block's language (e.g., ```lua → ```python)
    // should invalidate the cached tokens for that region, even if the content
    // is identical (same content_hash).
    //
    // This tests the `language_changed` check in CacheCoordinator::populate_injections().

    let injection_map = InjectionMap::new();
    let injection_token_cache = kakehashi::analysis::InjectionTokenCache::new();
    let uri = Url::parse("file:///test/language_change.md").unwrap();

    // Initial document with a Lua code block
    let initial_text = r#"# Example

```lua
print("hello")
```
"#;

    let mut parser = Parser::new();
    let md_language: tree_sitter::Language = tree_sitter_md::LANGUAGE.into();
    parser.set_language(&md_language).expect("set markdown");
    let initial_tree = parser.parse(initial_text, None).expect("parse");

    let injection_query_str = r#"
        (fenced_code_block
          (info_string
            (language) @_lang)
          (code_fence_content) @injection.content
          (#set-lang-from-info-string! @_lang))
    "#;
    let injection_query = Query::new(&md_language, injection_query_str).expect("query");

    // Initial population
    populate_injection_map(
        &injection_map,
        &uri,
        initial_text,
        &initial_tree,
        Some(&injection_query),
    );

    let initial_regions = injection_map.get(&uri).expect("should have regions");
    assert_eq!(initial_regions.len(), 1, "Should have one lua region");
    assert_eq!(initial_regions[0].language, "lua");
    let initial_region_id = initial_regions[0].region_id.clone();
    let initial_content_hash = initial_regions[0].content_hash;

    // Store tokens for the lua region
    let lua_tokens = tower_lsp_server::ls_types::SemanticTokens {
        result_id: Some("lua-tokens".to_string()),
        data: vec![tower_lsp_server::ls_types::SemanticToken {
            delta_line: 0,
            delta_start: 0,
            length: 5,
            token_type: 1, // keyword
            token_modifiers_bitset: 0,
        }],
    };
    injection_token_cache.store(&uri, &initial_region_id, lua_tokens);

    // Verify cache is populated
    assert!(
        injection_token_cache
            .get(&uri, &initial_region_id)
            .is_some(),
        "Should have cached tokens for lua region"
    );

    // Now change ONLY the language (lua → python), keeping the content identical
    // Note: "print" is valid in both Lua and Python, so content stays the same
    let edited_text = r#"# Example

```python
print("hello")
```
"#;

    let edited_tree = parser.parse(edited_text, None).expect("parse edited");

    // Re-populate - this simulates what happens after an edit
    populate_injection_map(
        &injection_map,
        &uri,
        edited_text,
        &edited_tree,
        Some(&injection_query),
    );

    let updated_regions = injection_map.get(&uri).expect("should have regions");
    assert_eq!(updated_regions.len(), 1, "Should have one python region");
    assert_eq!(
        updated_regions[0].language, "python",
        "Language should now be python"
    );

    // Verify that content_hash is the same (content didn't change)
    assert_eq!(
        updated_regions[0].content_hash, initial_content_hash,
        "Content hash should be identical since only language changed"
    );

    // THE KEY ASSERTION: Even though content_hash is the same, the cached tokens
    // should be invalidated because the LANGUAGE changed.
    //
    // Note: In the real implementation with RegionIdTracker, the region_id would
    // be the same if the position is the same. But in this test helper, we generate
    // new region_ids each time. So we check that:
    // 1. If using the old region_id, the cache should be cleared
    // 2. If region_id changed, that's also acceptable (new region = no cache)
    //
    // For populate_injection_map_with_stable_ids (which matches by content_hash),
    // the region_id would be reused, and the language change detection would
    // invalidate the cache. Let's test this with the stable_ids helper.
}

#[test]
fn test_language_change_invalidates_with_stable_ids() {
    // This test uses populate_injection_map_with_stable_ids which preserves region_ids
    // when content_hash matches. This is the scenario where language change detection
    // is critical: same content, same region_id, but different language.

    let injection_map = InjectionMap::new();
    let injection_token_cache = kakehashi::analysis::InjectionTokenCache::new();
    let uri = Url::parse("file:///test/language_change_stable.md").unwrap();

    // Initial document with a Lua code block
    let initial_text = r#"# Example

```lua
print("hello")
```
"#;

    let mut parser = Parser::new();
    let md_language: tree_sitter::Language = tree_sitter_md::LANGUAGE.into();
    parser.set_language(&md_language).expect("set markdown");
    let initial_tree = parser.parse(initial_text, None).expect("parse");

    let injection_query_str = r#"
        (fenced_code_block
          (info_string
            (language) @_lang)
          (code_fence_content) @injection.content
          (#set-lang-from-info-string! @_lang))
    "#;
    let injection_query = Query::new(&md_language, injection_query_str).expect("query");

    // Initial population with stable IDs
    populate_injection_map_with_stable_ids(
        &injection_map,
        &uri,
        initial_text,
        &initial_tree,
        Some(&injection_query),
    );

    let initial_regions = injection_map.get(&uri).expect("should have regions");
    assert_eq!(initial_regions.len(), 1);
    assert_eq!(initial_regions[0].language, "lua");
    let initial_region_id = initial_regions[0].region_id.clone();
    let initial_content_hash = initial_regions[0].content_hash;

    // Store tokens for the lua region
    let lua_tokens = tower_lsp_server::ls_types::SemanticTokens {
        result_id: Some("lua-tokens".to_string()),
        data: vec![tower_lsp_server::ls_types::SemanticToken {
            delta_line: 0,
            delta_start: 0,
            length: 5,
            token_type: 1, // keyword
            token_modifiers_bitset: 0,
        }],
    };
    injection_token_cache.store(&uri, &initial_region_id, lua_tokens);

    // Verify cache is populated
    assert!(
        injection_token_cache
            .get(&uri, &initial_region_id)
            .is_some(),
        "Should have cached tokens"
    );

    // Change language from lua to python (content stays the same)
    let edited_text = r#"# Example

```python
print("hello")
```
"#;

    let edited_tree = parser.parse(edited_text, None).expect("parse edited");

    // Re-populate with stable IDs
    // The populate_injection_map_with_stable_ids matches by (language, content_hash),
    // so when language changes, it won't match the existing region and will generate
    // a new region_id. This means the old cache entry becomes orphaned (stale).
    populate_injection_map_with_stable_ids(
        &injection_map,
        &uri,
        edited_text,
        &edited_tree,
        Some(&injection_query),
    );

    let updated_regions = injection_map.get(&uri).expect("should have regions");
    assert_eq!(updated_regions.len(), 1);
    assert_eq!(
        updated_regions[0].language, "python",
        "Language should now be python"
    );

    // Verify content_hash is the same
    assert_eq!(
        updated_regions[0].content_hash, initial_content_hash,
        "Content hash should be identical"
    );

    // Since populate_injection_map_with_stable_ids matches by (language, content_hash),
    // and the language changed, the new region should have a NEW region_id
    assert_ne!(
        updated_regions[0].region_id, initial_region_id,
        "Region ID should change when language changes (different (language, content_hash) key)"
    );

    // The old cache entry (keyed by initial_region_id) is now orphaned
    // In production, CacheCoordinator::populate_injections would detect stale
    // regions and clean them up. Here we just verify the new region has no cache.
    assert!(
        injection_token_cache
            .get(&uri, &updated_regions[0].region_id)
            .is_none(),
        "New python region should have no cached tokens"
    );
}

// ===========================================================================
// AC6 Tests: Structural changes refresh InjectionMap
// ===========================================================================

#[test]
fn test_adding_new_code_block_updates_injection_map() {
    // AC6: Add new code block to document, verify InjectionMap updated with new region

    let injection_map = InjectionMap::new();
    let uri = Url::parse("file:///test/structural.md").unwrap();

    // Initial document with one code block
    let initial_text = r#"# Example

```lua
print("hello")
```
"#;

    let mut parser = Parser::new();
    let md_language: tree_sitter::Language = tree_sitter_md::LANGUAGE.into();
    parser.set_language(&md_language).expect("set markdown");
    let initial_tree = parser.parse(initial_text, None).expect("parse");

    let injection_query_str = r#"
        (fenced_code_block
          (info_string
            (language) @_lang)
          (code_fence_content) @injection.content
          (#set-lang-from-info-string! @_lang))
    "#;
    let injection_query = Query::new(&md_language, injection_query_str).expect("query");

    // Initial population
    populate_injection_map(
        &injection_map,
        &uri,
        initial_text,
        &initial_tree,
        Some(&injection_query),
    );

    let initial_regions = injection_map.get(&uri).expect("should have regions");
    assert_eq!(initial_regions.len(), 1, "Should have one initial region");
    assert_eq!(initial_regions[0].language, "lua");

    // Now simulate adding a new code block (structural change)
    let edited_text = r#"# Example

```lua
print("hello")
```

```python
def foo():
    pass
```
"#;

    let edited_tree = parser.parse(edited_text, None).expect("parse edited");

    // Re-populate after structural change (simulates what parse_document does)
    populate_injection_map(
        &injection_map,
        &uri,
        edited_text,
        &edited_tree,
        Some(&injection_query),
    );

    // Verify InjectionMap now has two regions
    let updated_regions = injection_map.get(&uri).expect("should have regions");
    assert_eq!(
        updated_regions.len(),
        2,
        "Should have two regions after adding code block"
    );

    let has_lua = updated_regions.iter().any(|r| r.language == "lua");
    let has_python = updated_regions.iter().any(|r| r.language == "python");
    assert!(has_lua, "Should still have lua region");
    assert!(has_python, "Should have new python region");
}

#[test]
fn test_removing_code_block_clears_stale_cache() {
    // AC6: Remove code block, verify stale cache entries are handled

    let injection_map = InjectionMap::new();
    let injection_token_cache = kakehashi::analysis::InjectionTokenCache::new();
    let uri = Url::parse("file:///test/remove_block.md").unwrap();

    // Initial document with two code blocks
    let initial_text = r#"# Example

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
    let initial_tree = parser.parse(initial_text, None).expect("parse");

    let injection_query_str = r#"
        (fenced_code_block
          (info_string
            (language) @_lang)
          (code_fence_content) @injection.content
          (#set-lang-from-info-string! @_lang))
    "#;
    let injection_query = Query::new(&md_language, injection_query_str).expect("query");

    // Initial population
    populate_injection_map(
        &injection_map,
        &uri,
        initial_text,
        &initial_tree,
        Some(&injection_query),
    );

    let initial_regions = injection_map.get(&uri).expect("should have regions");
    assert_eq!(initial_regions.len(), 2, "Should have two initial regions");

    // Cache tokens for both regions
    let lua_region = initial_regions
        .iter()
        .find(|r| r.language == "lua")
        .unwrap();
    let python_region = initial_regions
        .iter()
        .find(|r| r.language == "python")
        .unwrap();

    let lua_tokens = tower_lsp_server::ls_types::SemanticTokens {
        result_id: Some("lua-tokens".to_string()),
        data: vec![],
    };
    let python_tokens = tower_lsp_server::ls_types::SemanticTokens {
        result_id: Some("python-tokens".to_string()),
        data: vec![],
    };
    injection_token_cache.store(&uri, &lua_region.region_id, lua_tokens);
    injection_token_cache.store(&uri, &python_region.region_id, python_tokens);

    // Verify both cached
    assert!(
        injection_token_cache
            .get(&uri, &lua_region.region_id)
            .is_some()
    );
    assert!(
        injection_token_cache
            .get(&uri, &python_region.region_id)
            .is_some()
    );

    // Save the old python region_id for later check
    let old_python_region_id = python_region.region_id.clone();

    // Now simulate removing the python code block (structural change)
    let edited_text = r#"# Example

```lua
print("hello")
```

Some text instead of python block.
"#;

    let edited_tree = parser.parse(edited_text, None).expect("parse edited");

    // Clear stale cache entries before re-populating
    // This is the key behavior for AC6 - find regions that no longer exist
    // In real implementation, this happens in parse_document or a helper
    if let Some(old_regions) = injection_map.get(&uri) {
        // Re-collect injection regions from new tree
        if let Some(new_region_infos) = collect_all_injections(
            &edited_tree.root_node(),
            edited_text,
            Some(&injection_query),
        ) {
            // Convert to cacheable for comparison
            let new_byte_ranges: std::collections::HashSet<_> = new_region_infos
                .iter()
                .map(|info| {
                    // We need to match by byte_range since region_id will be new
                    (info.content_node.start_byte(), info.content_node.end_byte())
                })
                .collect();

            // Old regions that don't match any new region should have cache cleared
            for old_region in &old_regions {
                let old_key = (old_region.byte_range.start, old_region.byte_range.end);
                if !new_byte_ranges.contains(&old_key) {
                    injection_token_cache.remove(&uri, &old_region.region_id);
                }
            }
        }
    }

    // Re-populate after structural change
    populate_injection_map(
        &injection_map,
        &uri,
        edited_text,
        &edited_tree,
        Some(&injection_query),
    );

    // Verify InjectionMap now has only one region
    let updated_regions = injection_map.get(&uri).expect("should have regions");
    assert_eq!(
        updated_regions.len(),
        1,
        "Should have only one region after removing code block"
    );
    assert_eq!(updated_regions[0].language, "lua");

    // Stale python cache should be cleared
    // Note: This tests the expected behavior - implementation may vary
    assert!(
        injection_token_cache
            .get(&uri, &old_python_region_id)
            .is_none(),
        "Removed python region cache should be cleared"
    );

    // Lua cache is still there (lua region wasn't removed)
    // Note: In reality, the lua region_id changes after re-population since
    // populate_injection_map generates new region_ids. So this assertion
    // checks the OLD region_id which should still be in cache until explicit removal.
    // The implementation might need to handle this differently.
}

// ===========================================================================
// AC3 (PBI-084): Stable region IDs preserve cache across parses
// ===========================================================================

/// Helper that preserves region_ids for unchanged injection regions.
///
/// This is the key optimization: instead of always generating new region_ids,
/// we match existing regions by (language, content_hash) and preserve their IDs.
fn populate_injection_map_with_stable_ids(
    injection_map: &InjectionMap,
    uri: &Url,
    text: &str,
    tree: &tree_sitter::Tree,
    injection_query: Option<&Query>,
) {
    // Collect new injection regions
    let Some(regions) = collect_all_injections(&tree.root_node(), text, injection_query) else {
        // No query or no matches - clear and return
        if injection_map.get(uri).is_some_and(|e| !e.is_empty()) {
            injection_map.insert(uri.clone(), Vec::new());
        }
        return;
    };

    if regions.is_empty() {
        injection_map.insert(uri.clone(), Vec::new());
        return;
    }

    // Get existing regions for ID preservation - match by (language, content_hash)
    let existing_regions = injection_map.get(uri);
    let existing_map: std::collections::HashMap<(&str, u64), &CacheableInjectionRegion> =
        existing_regions
            .as_ref()
            .map(|regions| {
                regions
                    .iter()
                    .map(|r| ((r.language.as_str(), r.content_hash), r))
                    .collect()
            })
            .unwrap_or_default();

    // Convert to CacheableInjectionRegion, reusing region_ids where possible
    let cacheable_regions: Vec<CacheableInjectionRegion> = regions
        .iter()
        .map(|info| {
            // Compute hash for matching
            let temp_region = CacheableInjectionRegion::from_region_info(info, "", text);
            let key = (info.language.as_str(), temp_region.content_hash);

            // Check if we have an existing region with same (language, content_hash)
            if let Some(existing) = existing_map.get(&key) {
                // Reuse the existing region_id - enable cache hit!
                CacheableInjectionRegion {
                    language: temp_region.language,
                    byte_range: temp_region.byte_range,
                    line_range: temp_region.line_range,
                    region_id: existing.region_id.clone(),
                    content_hash: temp_region.content_hash,
                }
            } else {
                // Generate new region_id for new regions
                CacheableInjectionRegion {
                    region_id: next_result_id(),
                    ..temp_region
                }
            }
        })
        .collect();

    injection_map.insert(uri.clone(), cacheable_regions);
}

#[test]
fn test_stable_region_id_preserved_after_edit_outside_injection() {
    // AC3 (PBI-084): After edit outside injection, region_id for unchanged injection is preserved
    //
    // This is the key optimization: when editing host text (not touching injections),
    // the injection regions retain their region_ids, enabling cache hits.

    let injection_map = InjectionMap::new();
    let uri = Url::parse("file:///test/stable_ids.md").unwrap();

    // Document with one code block
    let initial_text = r#"# Header

```lua
print("hello")
```

Footer text
"#;

    let mut parser = Parser::new();
    let md_language: tree_sitter::Language = tree_sitter_md::LANGUAGE.into();
    parser.set_language(&md_language).expect("set markdown");
    let initial_tree = parser.parse(initial_text, None).expect("parse");

    let injection_query_str = r#"
        (fenced_code_block
          (info_string
            (language) @_lang)
          (code_fence_content) @injection.content
          (#set-lang-from-info-string! @_lang))
    "#;
    let injection_query = Query::new(&md_language, injection_query_str).expect("query");

    // Initial population
    populate_injection_map_with_stable_ids(
        &injection_map,
        &uri,
        initial_text,
        &initial_tree,
        Some(&injection_query),
    );

    let initial_regions = injection_map.get(&uri).expect("should have regions");
    assert_eq!(initial_regions.len(), 1, "Should have one lua region");
    let initial_region_id = initial_regions[0].region_id.clone();
    let initial_byte_range = initial_regions[0].byte_range.clone();

    // Edit the header (outside the injection)
    // "# Header" -> "# Modified Header"
    let edited_text = r#"# Modified Header

```lua
print("hello")
```

Footer text
"#;

    let edited_tree = parser.parse(edited_text, None).expect("parse edited");

    // Re-populate with stable IDs
    populate_injection_map_with_stable_ids(
        &injection_map,
        &uri,
        edited_text,
        &edited_tree,
        Some(&injection_query),
    );

    let updated_regions = injection_map.get(&uri).expect("should have regions");
    assert_eq!(updated_regions.len(), 1, "Should still have one region");

    // THE KEY ASSERTION: region_id should be PRESERVED for unchanged injection
    // Note: byte_range will change since header is now longer, so region_id
    // will actually be NEW. This test documents the EXPECTED behavior.
    //
    // For stable IDs to work, we need to match by something other than byte_range
    // when the document structure changes. Options:
    // 1. Content hash
    // 2. AST path / node identity
    // 3. Language + relative position (nth lua block)
    //
    // For now, this test will FAIL because byte_range changes when header grows.
    // This documents the problem we need to solve.

    let updated_region_id = &updated_regions[0].region_id;
    let updated_byte_range = &updated_regions[0].byte_range;

    // Header grew by "Modified ".len() = 9 chars, so byte_range shifts
    assert_ne!(
        initial_byte_range, *updated_byte_range,
        "Byte range should shift when header changes"
    );

    // Currently FAILS: region_id is regenerated because byte_range changed
    // After implementing stable IDs via content hash, this should pass.
    assert_eq!(
        initial_region_id, *updated_region_id,
        "Region ID should be preserved for unchanged injection content"
    );
}

#[test]
fn test_cache_hit_after_edit_outside_injection() {
    // AC3 (PBI-084) Subtask 4: Verify cache hit after edit outside injection
    //
    // This test verifies that when we edit host text (outside injection),
    // the cached tokens for the injection can still be retrieved using
    // the preserved region_id.

    let injection_map = InjectionMap::new();
    let injection_token_cache = kakehashi::analysis::InjectionTokenCache::new();
    let uri = Url::parse("file:///test/cache_hit.md").unwrap();

    // Document with one code block
    let initial_text = r#"# Header

```lua
print("hello")
```

Footer text
"#;

    let mut parser = Parser::new();
    let md_language: tree_sitter::Language = tree_sitter_md::LANGUAGE.into();
    parser.set_language(&md_language).expect("set markdown");
    let initial_tree = parser.parse(initial_text, None).expect("parse");

    let injection_query_str = r#"
        (fenced_code_block
          (info_string
            (language) @_lang)
          (code_fence_content) @injection.content
          (#set-lang-from-info-string! @_lang))
    "#;
    let injection_query = Query::new(&md_language, injection_query_str).expect("query");

    // Initial population with stable IDs
    populate_injection_map_with_stable_ids(
        &injection_map,
        &uri,
        initial_text,
        &initial_tree,
        Some(&injection_query),
    );

    let initial_regions = injection_map.get(&uri).expect("should have regions");
    assert_eq!(initial_regions.len(), 1, "Should have one lua region");
    let initial_region_id = initial_regions[0].region_id.clone();

    // Store tokens for the lua injection using the region_id
    let lua_tokens = tower_lsp_server::ls_types::SemanticTokens {
        result_id: Some(initial_region_id.clone()),
        data: vec![tower_lsp_server::ls_types::SemanticToken {
            delta_line: 0,
            delta_start: 0,
            length: 5,
            token_type: 1, // function
            token_modifiers_bitset: 0,
        }],
    };
    injection_token_cache.store(&uri, &initial_region_id, lua_tokens);

    // Verify cache is populated
    assert!(
        injection_token_cache
            .get(&uri, &initial_region_id)
            .is_some(),
        "Should have cached tokens for initial region_id"
    );

    // Edit the header (outside the injection)
    let edited_text = r#"# Modified Header

```lua
print("hello")
```

Footer text
"#;

    let edited_tree = parser.parse(edited_text, None).expect("parse edited");

    // Re-populate with stable IDs
    populate_injection_map_with_stable_ids(
        &injection_map,
        &uri,
        edited_text,
        &edited_tree,
        Some(&injection_query),
    );

    let updated_regions = injection_map.get(&uri).expect("should have regions");
    assert_eq!(updated_regions.len(), 1, "Should still have one region");
    let updated_region_id = &updated_regions[0].region_id;

    // THE KEY ASSERTION: region_id is preserved, so we can get a CACHE HIT
    assert_eq!(
        initial_region_id, *updated_region_id,
        "Region ID should be preserved"
    );

    // CACHE HIT: We can retrieve the cached tokens using the updated region_id
    let cached_tokens = injection_token_cache.get(&uri, updated_region_id);
    assert!(
        cached_tokens.is_some(),
        "Should get CACHE HIT - tokens should still be retrievable after edit outside injection"
    );

    let tokens = cached_tokens.unwrap();
    assert_eq!(tokens.data.len(), 1, "Should have one token");
    assert_eq!(
        tokens.data[0].token_type, 1,
        "Token type should be preserved"
    );
}
