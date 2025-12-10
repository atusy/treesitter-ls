# Comprehensive Review: `selectionRange` Feature

**Date:** 2025-12-10
**Reviewer:** Claude (Opus 4.5)
**Specification Reference:** LSP 3.17 (`textDocument/selectionRange`)

## Executive Summary

The `selectionRange` implementation in treesitter-ls is a **well-engineered, feature-rich implementation** that goes beyond basic LSP requirements by supporting language injections and nested injection hierarchies. However, there are several areas requiring attention ranging from minor improvements to potential issues.

| Category | Rating | Summary |
|----------|--------|---------|
| LSP Protocol Compliance |  Excellent | Full compliance with LSP 3.17 spec |
| Error Handling |   Needs Fix | One critical lock poisoning issue |
| Edge Cases |  Good | Comprehensive coverage |
| Performance |  Good | Proper caching, some optimization opportunities |
| Test Coverage |  Comprehensive | 14 unit tests + integration tests |

---

## 1. LSP Protocol Compliance  EXCELLENT

The LSP 3.17 specification has specific requirements for selectionRange that are often overlooked:
1. Results must have 1:1 correspondence with input positions (positions[i] ’ result[i])
2. Empty ranges are valid fallbacks for positions that can't be resolved
3. Parent ranges must be strictly expanding (contain but not equal child ranges)

### Compliance Analysis

| Requirement | Status | Evidence |
|-------------|--------|----------|
| **1:1 position alignment** |  | Uses `map()` not `filter_map()` at `selection.rs:1139-1180`, `selection.rs:1216-1259` |
| **Empty range fallback** |  | Returns `Range::new(*pos, *pos)` for failed positions (`selection.rs:1172-1178`) |
| **Strictly expanding parents** |  | `find_distinct_parent()` skips same-range nodes (`selection.rs:65-79`) |
| **parent.range contains this.range** |  | `is_range_strictly_larger()` enforces containment (`selection.rs:773-785`) |
| **UTF-16 column positions** |  | Uses `PositionMapper` for byte”UTF-16 conversion (`selection.rs:37-45`) |

### Spec Quote Verification

The implementation correctly documents the LSP spec requirement at `selection.rs:1130-1136`:

```rust
// LSP Spec 3.17 requires 1:1 correspondence between positions and results:
// "A selection range in the return array is for the position in the provided
// parameters at the same index. Therefore positions[i] must be contained in
// result[i].range. To allow for results where some positions have selection
// ranges and others do not, result[i].range is allowed to be the empty range
// at positions[i]."
```

---

## 2. Error Handling   NEEDS IMPROVEMENT

### 2.1 CRITICAL: Lock Poison Handling Missing

**Location:** `src/lsp/lsp_impl.rs:780`

```rust
let mut pool = self.parser_pool.lock().unwrap();
```

**Issue:** This `unwrap()` on a mutex lock will panic if another thread panicked while holding the lock, causing the server to crash.

**Required Fix (per CLAUDE.md):**

```rust
let mut pool = match self.parser_pool.lock() {
    Ok(guard) => guard,
    Err(poisoned) => {
        log::warn!(target: "treesitter_ls::lock_recovery",
                   "Recovered from poisoned parser pool lock in selection_range");
        poisoned.into_inner()
    }
};
```

**Severity:** HIGH - Production code should handle lock poisoning gracefully.

**Note:** The same issue exists at `lsp_impl.rs:91`.

### 2.2 Safe Usage of unwrap()

**Location:** `src/analysis/selection.rs:464`

```rust
let nested_lang = hierarchy.last().unwrap().clone();
```

**Context:** This is preceded by a guard check:
```rust
if hierarchy.len() < 2 {
    return build_injected_selection_range(*node, root, parent_start_byte, mapper);
}
```

**Verdict:**  Safe - the length check guarantees `last()` returns `Some`.

### 2.3 Graceful Fallback Pattern 

The code consistently uses fallback patterns for failures:
```rust
let Some(mut parser) = parser_pool.acquire(injected_lang) else {
    return build_fallback();
};
```

---

## 3. Edge Cases Handling  GOOD

### 3.1 Invalid Positions

- **Test:** `test_selection_range_maintains_position_alignment` (`selection.rs:2460-2532`)
- **Behavior:** Returns empty range at requested position, maintaining array alignment

### 3.2 Multi-byte UTF-8 Characters

UTF-8 to UTF-16 conversion is a common source of bugs in LSP servers:
- Japanese "B" = 3 bytes (UTF-8) = 1 code unit (UTF-16)
- The `PositionMapper` correctly handles this using the `line_index` crate
- Both input (position’byte) and output (byte’position) paths are covered

**Tests:**
- `test_selection_range_handles_multibyte_utf8` (`selection.rs:2114-2178`)
- `test_selection_range_output_uses_utf16_columns` (`selection.rs:2195-2249`)
- `test_injected_selection_range_uses_utf16_columns` (`selection.rs:2264-2450`)

### 3.3 Negative Offset Handling

- **Test:** `test_calculate_nested_start_position_handles_negative_offsets` (`selection.rs:1854-1937`)
- **Implementation:** Uses `i64` with `.max(0)` to prevent underflow:

```rust
let row = (base_row + offset_rows as i64).max(0) as usize;
```

### 3.4 Same-Range Node Deduplication

- **Test:** `test_selection_range_deduplicates_same_range_nodes` (`selection.rs:2033-2094`)
- **Implementation:** `find_distinct_parent()` and `find_next_distinct_parent()` skip nodes with identical ranges

### 3.5 Maximum Recursion Depth

```rust
const MAX_INJECTION_DEPTH: usize = 10;
```

Protection against infinite recursion in deeply nested injections (`selection.rs:182`).

---

## 4. Performance Considerations  MOSTLY GOOD

### 4.1 PositionMapper Caching 

The handler functions reuse the document's cached `PositionMapper`:

```rust
// Reuse the document's cached position mapper instead of creating a new one per position.
// This avoids O(file_size × positions) work from rebuilding LineIndex for each cursor.
let mapper = document.position_mapper();
```

**Note:** Test code creates `PositionMapper::new(text)` which is acceptable for tests.

### 4.2 Parser Pool 

Uses `DocumentParserPool` for parser reuse:

```rust
let Some(mut parser) = parser_pool.acquire(injected_lang) else { ... };
// ... use parser ...
parser_pool.release(injected_lang.to_string(), parser);
```

### 4.3 Recursion Overhead  

The recursive `build_selection_range_with_parsed_injection_recursive` and `build_nested_injection_selection` functions could be expensive for deeply nested injections. The `MAX_INJECTION_DEPTH = 10` limit helps, but:

- Each recursion level requires parsing the injected content
- Selection hierarchy rebuilding involves multiple tree traversals

**Recommendation:** Consider caching parsed injection trees if this becomes a bottleneck.

### 4.4 Function Argument Count  

Several functions have 8+ parameters:

```rust
#[allow(clippy::too_many_arguments)]
pub fn build_selection_range_with_parsed_injection(
    node: Node,
    root: &Node,
    text: &str,
    mapper: &PositionMapper,
    injection_query: Option<&Query>,
    base_language: &str,
    coordinator: &LanguageCoordinator,
    parser_pool: &mut DocumentParserPool,
    cursor_byte: usize,
) -> SelectionRange
```

**Impact:** Makes the code harder to maintain and test. Consider introducing a context struct.

---

## 5. Test Coverage  COMPREHENSIVE

### Unit Tests (14 tests in selection.rs)

| Test | Purpose |
|------|---------|
| `test_position_to_point` | Basic ASCII conversion |
| `test_point_to_position` | Basic ASCII conversion |
| `test_selection_range_detects_injection` | Injection detection |
| `test_selection_range_respects_offset_directive` | Offset parsing |
| `test_selection_range_handles_nested_injection` | Nested injections |
| `test_nested_injection_includes_content_node_boundary` | Content node in chain |
| `test_selection_range_parses_injected_content` | YAML injection parsing |
| `test_calculate_nested_start_position_handles_negative_offsets` | Underflow prevention |
| `test_column_alignment_when_row_offset_skips_lines` | Column offset semantics |
| `test_selection_range_deduplicates_same_range_nodes` | Duplicate range removal |
| `test_selection_range_handles_multibyte_utf8` | UTF-8/UTF-16 input |
| `test_selection_range_output_uses_utf16_columns` | UTF-16 output |
| `test_injected_selection_range_uses_utf16_columns` | Injected UTF-16 |
| `test_selection_range_maintains_position_alignment` | Invalid position handling |

### Integration Tests (tests/test_lsp_select.lua)

- Lua file tests (no injection)
- Markdown file tests with YAML frontmatter
- Markdown code block injection
- Nested injection (markdown ’ markdown ’ lua)

The test suite is particularly strong in areas that commonly cause issues:
1. UTF-16 encoding (3 dedicated tests)
2. Nested injections (3 dedicated tests)
3. Offset directives (2 dedicated tests)
4. LSP spec compliance (alignment test)

### Missing Test Coverage  

1. **Empty document** - What happens with `""`?
2. **Single character document** - Edge case for position calculations
3. **Very long lines** - Performance with large column values
4. **Concurrent access** - Parser pool under contention
5. **Malformed injection queries** - Query syntax errors

---

## 6. Code Quality  GOOD

### 6.1 Documentation

Functions have clear doc comments explaining:
- Purpose and behavior
- Parameter meanings
- Return values
- Historical context (Sprint references)

Example:
```rust
/// Build selection range hierarchy with injection awareness and offset support
///
/// This version takes a cursor_byte parameter to check if the cursor is within
/// the effective range of the injection after applying offset directives.
```

### 6.2 Helper Function Organization

Well-factored helpers:
- `node_to_range()` - Node to LSP Range
- `find_distinct_parent()` - Skip same-range parents
- `is_range_strictly_larger()` - Containment check
- `ranges_equal()` - Range equality
- `range_contains()` - Range containment

### 6.3 Test-Only Code Isolation

```rust
#[cfg(test)]
fn position_to_point(pos: &Position) -> Point { ... }
```

Properly marked to prevent accidental production use.

---

## 7. Recommendations

### Priority 1 (High) - Bug Fixes

1. **Fix lock poisoning** in `lsp_impl.rs:780` and `lsp_impl.rs:91`:

```rust
let mut pool = match self.parser_pool.lock() {
    Ok(guard) => guard,
    Err(poisoned) => {
        log::warn!(target: "treesitter_ls::lock_recovery",
                   "Recovered from poisoned parser pool lock");
        poisoned.into_inner()
    }
};
```

### Priority 2 (Medium) - Improvements

2. **Add empty document test** to cover edge case
3. **Consider context struct** to reduce function parameter count:

```rust
struct SelectionContext<'a> {
    text: &'a str,
    mapper: &'a PositionMapper<'a>,
    injection_query: Option<&'a Query>,
    base_language: &'a str,
    coordinator: &'a LanguageCoordinator,
    parser_pool: &'a mut DocumentParserPool,
}
```

4. **Add concurrent access test** for parser pool

### Priority 3 (Low) - Enhancements

5. **Consider caching** parsed injection trees for frequently accessed positions
6. **Add metrics/tracing** for injection parsing performance

---

## 8. Conclusion

The `selectionRange` implementation is **production-ready with one critical fix needed** (lock poisoning). The feature goes beyond basic LSP requirements by supporting:

- Language injections (Rust ’ YAML, Markdown ’ Lua, etc.)
- Nested injections up to 10 levels deep
- Offset directives for trimming injection boundaries
- Proper UTF-16 encoding throughout

The test coverage is comprehensive, and the code follows clean architecture principles with proper separation of concerns.

### Key Strengths

1. **Spec Compliance**: Full adherence to LSP 3.17 requirements
2. **Injection Support**: Sophisticated nested injection handling
3. **UTF-16 Handling**: Correct encoding throughout the pipeline
4. **Test Coverage**: Strong coverage of edge cases

### Action Items

| Priority | Item | Effort |
|----------|------|--------|
| P1 | Fix lock poisoning (2 locations) | Small |
| P2 | Add empty document test | Small |
| P2 | Consider context struct refactor | Medium |
| P3 | Add injection tree caching | Large |
