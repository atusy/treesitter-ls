# Selection Range Refactoring Plan

## Goal

Refactor `src/analysis/selection.rs` (~1300 lines) into smaller, cohesive modules following the Single Responsibility Principle, while preserving all existing behavior through TDD.

## Current State Analysis

The monolithic `selection.rs` handles 6 distinct concerns:
1. AST traversal (finding nodes, building parent chains)
2. Injection detection orchestration
3. Parser acquisition/release
4. Coordinate conversion (byte ↔ UTF-16)
5. Nested injection recursion
6. LSP protocol compliance

**Code Smells Identified:**
- `#[allow(clippy::too_many_arguments)]` on 4 functions
- 20+ helper functions with mixed abstraction levels
- 3 public entry points at different abstraction levels

## Target Architecture

Using Rust 2018+ module style (no `mod.rs`):

```
src/analysis/
├── selection.rs            # Public API facade + re-exports
└── selection/
    ├── range_builder.rs    # Pure AST → SelectionRange (no injection)
    ├── hierarchy_chain.rs  # Hierarchy manipulation utilities
    └── injection_aware.rs  # Injection-aware selection building
```

In `selection.rs`:
```rust
mod selection {
    pub mod hierarchy_chain;
    pub mod range_builder;
    pub mod injection_aware;
}
// Re-export public API
pub use selection::hierarchy_chain::*;
pub use selection::range_builder::*;
pub use selection::injection_aware::*;
```

## TDD Methodology

Each cycle follows the RED-GREEN-REFACTOR pattern. A cycle may contain **multiple iterations** of this pattern for incremental progress:

```
Cycle N:
  Iteration 1: RED → GREEN → REFACTOR → COMMIT
  Iteration 2: RED → GREEN → REFACTOR → COMMIT
  ...
  ✓ Cycle complete when all tests pass and code is clean
```

**Commit Discipline:**
- **COMMIT after EVERY iteration** (after REFACTOR step)
- Each commit must have all tests passing
- Commit message format: `refactor(selection): <iteration description>`
- Example: `refactor(selection): extract is_range_strictly_larger to hierarchy_chain`

---

## Phase 1: Extract Hierarchy Chain Utilities (Structural)

**Rationale:** These are pure functions with no external dependencies—easiest to extract first.

### Cycle 1.1: Range Comparison Utilities

Extract `is_range_strictly_larger`, `range_contains`, `ranges_equal` together since they form a cohesive unit.

- [x] **Iteration 1: `is_range_strictly_larger`**
  - [x] RED: Create `src/analysis/selection/hierarchy_chain.rs` with failing test
  - [x] GREEN: Copy function, make test pass
  - [x] REFACTOR: Add documentation
  - [x] COMMIT: `refactor(selection): extract is_range_strictly_larger to hierarchy_chain`

- [x] **Iteration 2: `range_contains`**
  - [x] RED: Add test for `range_contains`
  - [x] GREEN: Copy function
  - [x] REFACTOR: None needed
  - [x] COMMIT: `refactor(selection): extract range_contains to hierarchy_chain`

- [x] **Iteration 3: `ranges_equal`**
  - [x] RED: Add test for `ranges_equal`
  - [x] GREEN: Copy function
  - [x] REFACTOR: Consider if this should just use `PartialEq`
  - [x] COMMIT: `refactor(selection): extract ranges_equal to hierarchy_chain`

- [x] **Iteration 4: Wire up imports**
  - [x] RED: Change `selection.rs` to use `hierarchy_chain::*`, expect compile errors
  - [x] GREEN: Fix imports, all tests pass
  - [x] REFACTOR: Remove duplicated functions from `selection.rs`
  - [x] COMMIT: `refactor(selection): wire up hierarchy_chain imports in selection.rs`

- [x] Run `cargo test && cargo clippy -- -D warnings`

### Cycle 1.2: Selection Hierarchy Chaining

Extract the functions that manipulate `SelectionRange` parent chains.

- [x] **Iteration 1: `skip_to_distinct_host`**
  - [x] RED: Add test for finding first strictly-larger host range
  - [x] GREEN: Move function to `hierarchy_chain.rs`
  - [x] REFACTOR: Simplify if possible
  - [x] COMMIT: `refactor(selection): extract skip_to_distinct_host to hierarchy_chain`

- [x] **Iteration 2-3: `chain_injected_to_host`** (combined inner helper + main)
  - [x] RED: Add tests for `find_and_connect_tail` and full chaining behavior
  - [x] GREEN: Extract as standalone function with nested helper
  - [x] REFACTOR: Update `selection.rs` imports
  - [x] COMMIT: `refactor(selection): extract chain_injected_to_host to hierarchy_chain`

- [x] **Iteration 4: Wire up imports**
  - [x] Remove duplicate functions from `selection.rs`
  - [x] COMMIT: `refactor(selection): wire up chain_injected_to_host and skip_to_distinct_host imports`

- [x] Run `cargo test && cargo clippy -- -D warnings`

### Phase 1 Checkpoint ✓
- [x] `hierarchy_chain.rs` contains all range/chain utilities
- [x] `selection.rs` imports from `hierarchy_chain`
- [x] All existing tests pass (161 tests)
- [x] No new clippy warnings

---

## Phase 2: Extract Pure Range Builder (Structural)

**Rationale:** Extract the non-injection selection logic that depends only on Tree-sitter AST.

### Cycle 2.1: Node-to-Range Conversion

- [x] **Iteration 1: `node_to_range`**
  - [x] RED: Create `src/analysis/selection/range_builder.rs` with test for `node_to_range`
  - [x] GREEN: Move function
  - [x] REFACTOR: Document UTF-16 conversion behavior
  - [x] COMMIT: `refactor(selection): extract node_to_range to range_builder`

- [x] Run `cargo test`

### Cycle 2.2: Parent Chain Traversal

- [x] **Iteration 1: `find_distinct_parent`**
  - [x] RED: Add test for finding parent with different range
  - [x] GREEN: Move function
  - [x] REFACTOR: None needed (pure function)
  - [x] COMMIT: `refactor(selection): extract find_distinct_parent to range_builder`

- [x] **Iteration 2: `find_next_distinct_parent`**
  - [x] RED: Add test for root-aware parent finding
  - [x] GREEN: Move function
  - [x] REFACTOR: Consider merging with `find_distinct_parent` if similar
  - [x] COMMIT: `refactor(selection): extract find_next_distinct_parent to range_builder`

- [x] Run `cargo test`

### Cycle 2.3: Core Selection Building

- [x] **Iteration 1: `build_selection_range`**
  - [x] RED: Add test for building SelectionRange from AST node
  - [x] GREEN: Move function
  - [x] REFACTOR: Ensure it uses `hierarchy_chain` utilities
  - [x] COMMIT: `refactor(selection): extract build_selection_range to range_builder`

- [x] Run `cargo test && cargo clippy -- -D warnings`

### Phase 2 Checkpoint ✓
- [x] `range_builder.rs` contains pure AST→SelectionRange logic (4 functions)
- [x] No injection-related code in `range_builder.rs`
- [x] `selection.rs` imports from `range_builder`
- [x] All existing tests pass (161 tests)

---

## Phase 3: Extract Injection-Aware Builder (Structural)

**Rationale:** Encapsulate injection-related complexity into a dedicated module.

### Cycle 3.1: Coordinate Adjustment Utilities

- [ ] **Iteration 1: `adjust_range_to_host`**
  - [ ] RED: Create `src/analysis/selection/injection_aware.rs` with test
  - [ ] GREEN: Move function
  - [ ] REFACTOR: Document byte-to-UTF16 conversion
  - [ ] COMMIT: `refactor(selection): extract adjust_range_to_host to injection_aware`

- [ ] **Iteration 2: `calculate_effective_lsp_range`**
  - [ ] RED: Add test for effective range with offset
  - [ ] GREEN: Move function
  - [ ] REFACTOR: Verify it uses `offset_calculator` correctly
  - [ ] COMMIT: `refactor(selection): extract calculate_effective_lsp_range to injection_aware`

- [ ] **Iteration 3: `is_cursor_within_effective_range`**
  - [ ] RED: Add test for cursor position checking
  - [ ] GREEN: Move function
  - [ ] REFACTOR: Keep as pure function
  - [ ] COMMIT: `refactor(selection): extract is_cursor_within_effective_range to injection_aware`

- [ ] Run `cargo test`

### Cycle 3.2: Injected Content Selection

- [ ] **Iteration 1: `build_injected_selection_range`**
  - [ ] RED: Add test for building selection within injected content
  - [ ] GREEN: Move function
  - [ ] REFACTOR: Ensure it uses `range_builder` utilities
  - [ ] COMMIT: `refactor(selection): extract build_injected_selection_range to injection_aware`

- [ ] **Iteration 2: `build_injection_aware_selection`**
  - [ ] RED: Add test for injection boundary inclusion
  - [ ] GREEN: Move function
  - [ ] REFACTOR: None needed
  - [ ] COMMIT: `refactor(selection): extract build_injection_aware_selection to injection_aware`

- [ ] **Iteration 3: `build_injection_aware_selection_with_effective_range`**
  - [ ] RED: Add test for offset-adjusted injection selection
  - [ ] GREEN: Move function
  - [ ] REFACTOR: Consider DRYing with iteration 2
  - [ ] COMMIT: `refactor(selection): extract build_injection_aware_selection_with_effective_range`

- [ ] Run `cargo test`

### Cycle 3.3: Hierarchy Splicing

- [ ] **Iteration 1: `is_node_in_selection_chain`**
  - [ ] RED: Add test for chain membership check
  - [ ] GREEN: Move function
  - [ ] REFACTOR: None needed
  - [ ] COMMIT: `refactor(selection): extract is_node_in_selection_chain to injection_aware`

- [ ] **Iteration 2: `splice_injection_content_into_hierarchy`**
  - [ ] RED: Add test for splicing injection node
  - [ ] GREEN: Move function
  - [ ] REFACTOR: None needed
  - [ ] COMMIT: `refactor(selection): extract splice_injection_content_into_hierarchy`

- [ ] **Iteration 3: `rebuild_with_injection_boundary`**
  - [ ] RED: Add test for hierarchy rebuilding
  - [ ] GREEN: Move function
  - [ ] REFACTOR: None needed
  - [ ] COMMIT: `refactor(selection): extract rebuild_with_injection_boundary`

- [ ] **Iteration 4: `splice_effective_range_into_hierarchy`**
  - [ ] RED: Add test for effective range splicing
  - [ ] GREEN: Move function
  - [ ] REFACTOR: None needed
  - [ ] COMMIT: `refactor(selection): extract splice_effective_range_into_hierarchy`

- [ ] **Iteration 5: `rebuild_with_effective_range`**
  - [ ] RED: Add test for effective range rebuilding
  - [ ] GREEN: Move function
  - [ ] REFACTOR: None needed
  - [ ] COMMIT: `refactor(selection): extract rebuild_with_effective_range`

- [ ] **Iteration 6: `replace_range_in_chain`**
  - [ ] RED: Add test for range replacement
  - [ ] GREEN: Move function
  - [ ] REFACTOR: None needed
  - [ ] COMMIT: `refactor(selection): extract replace_range_in_chain to injection_aware`

- [ ] Run `cargo test && cargo clippy -- -D warnings`

### Cycle 3.4: Public Injection-Aware Functions

- [ ] **Iteration 1: `build_selection_range_with_injection`**
  - [ ] RED: Add integration test
  - [ ] GREEN: Move function
  - [ ] REFACTOR: Simplify using extracted helpers
  - [ ] COMMIT: `refactor(selection): extract build_selection_range_with_injection`

- [ ] **Iteration 2: `build_selection_range_with_injection_and_offset`**
  - [ ] RED: Add integration test with offset
  - [ ] GREEN: Move function
  - [ ] REFACTOR: DRY with iteration 1
  - [ ] COMMIT: `refactor(selection): extract build_selection_range_with_injection_and_offset`

- [ ] Run `cargo test`

### Cycle 3.5: Nested Injection (Complex)

This is the most complex part—handle carefully with multiple iterations.

- [ ] **Iteration 1: `build_selection_range_with_parsed_injection`**
  - [ ] RED: Verify existing test covers this
  - [ ] GREEN: Move function (entry point only)
  - [ ] REFACTOR: None yet
  - [ ] COMMIT: `refactor(selection): extract build_selection_range_with_parsed_injection`

- [ ] **Iteration 2: `build_selection_range_with_parsed_injection_recursive`**
  - [ ] RED: Add test for recursion depth handling
  - [ ] GREEN: Move function
  - [ ] REFACTOR: Consider extracting `InjectionContext` struct to reduce parameters
  - [ ] COMMIT: `refactor(selection): extract recursive injection selection logic`

- [ ] **Iteration 3: `build_nested_injection_selection`**
  - [ ] RED: Add test for nested injection boundary
  - [ ] GREEN: Move function
  - [ ] REFACTOR: Reduce `#[allow(clippy::too_many_arguments)]` by using context struct
  - [ ] COMMIT: `refactor(selection): extract build_nested_injection_selection`

- [ ] **Iteration 4: Introduce `InjectionContext` struct (optional)**
  - [ ] RED: Write test using `InjectionContext`
  - [ ] GREEN: Create struct to bundle (coordinator, parser_pool, mapper, depth)
  - [ ] REFACTOR: Update functions to use context
  - [ ] COMMIT: `refactor(selection): introduce InjectionContext to reduce parameter count`

- [ ] Run `cargo test && cargo clippy -- -D warnings`

### Phase 3 Checkpoint
- [ ] `injection_aware.rs` contains all injection-related selection logic
- [ ] Reduced `#[allow(clippy::too_many_arguments)]` usage
- [ ] `selection.rs` imports from `injection_aware`
- [ ] All existing tests pass

---

## Phase 4: Simplify Public API (Behavioral)

**Rationale:** Provide a cleaner interface while maintaining backward compatibility.

### Cycle 4.1: Unified Handler

- [ ] **Iteration 1: Create `SelectionRangeHandler` struct**
  - [ ] RED: Write test for handler with minimal dependencies
  - [ ] GREEN: Create struct with `handle()` method
  - [ ] REFACTOR: Document usage
  - [ ] COMMIT: `feat(selection): add SelectionRangeHandler struct`

- [ ] **Iteration 2: Add injection support to handler**
  - [ ] RED: Write test for handler detecting injection
  - [ ] GREEN: Implement injection detection in `handle()`
  - [ ] REFACTOR: None needed
  - [ ] COMMIT: `feat(selection): add injection support to SelectionRangeHandler`

- [ ] **Iteration 3: Add parsed injection support**
  - [ ] RED: Write test for full injection parsing
  - [ ] GREEN: Implement `handle_with_injection_parsing()`
  - [ ] REFACTOR: Consider builder pattern
  - [ ] COMMIT: `feat(selection): add parsed injection support to handler`

- [ ] Run `cargo test`

### Cycle 4.2: LSP Integration Update

- [ ] **Iteration 1: Update `lsp_impl.rs`**
  - [ ] RED: Existing LSP tests should still pass
  - [ ] GREEN: Replace direct calls with `SelectionRangeHandler`
  - [ ] REFACTOR: Remove redundant code
  - [ ] COMMIT: `refactor(lsp): use SelectionRangeHandler for selection_range`

- [ ] **Iteration 2: Deprecate old entry points**
  - [ ] RED: Add `#[deprecated]` attributes
  - [ ] GREEN: Ensure no deprecation warnings in main code
  - [ ] REFACTOR: Update any internal usages
  - [ ] COMMIT: `refactor(selection): deprecate intermediate entry points`

- [ ] Run `cargo test && cargo clippy -- -D warnings`

### Phase 4 Checkpoint
- [ ] Single entry point via `SelectionRangeHandler`
- [ ] LSP layer uses new handler
- [ ] Old functions deprecated but still work
- [ ] All tests pass

---

## Phase 5: Final Cleanup (Structural)

### Cycle 5.1: Module Organization

- [ ] **Iteration 1: Finalize `selection.rs` as facade**
  - [ ] RED: Test import `use crate::analysis::selection::*`
  - [ ] GREEN: Ensure `selection.rs` declares submodules and re-exports
  - [ ] REFACTOR: Organize public vs internal items
  - [ ] COMMIT: `refactor(selection): finalize module structure`

- [ ] **Iteration 2: Verify backward compatibility**
  - [ ] RED: Ensure all existing imports still work
  - [ ] GREEN: Fix any broken imports in `lsp_impl.rs` or tests
  - [ ] REFACTOR: Keep only necessary re-exports
  - [ ] COMMIT: `refactor(selection): ensure backward compatible imports`

- [ ] Run `cargo test`

### Cycle 5.2: Dead Code Removal

- [ ] **Iteration 1: Identify dead code**
  - [ ] RED: Run `cargo clippy -- -D dead_code`
  - [ ] GREEN: Remove unused functions
  - [ ] REFACTOR: None needed
  - [ ] COMMIT: `refactor(selection): remove dead code`

- [ ] **Iteration 2: Remove test-only helpers**
  - [ ] RED: Check `#[cfg(test)]` functions still needed
  - [ ] GREEN: Remove if unused
  - [ ] REFACTOR: None needed
  - [ ] COMMIT: `refactor(selection): clean up test-only helpers`

- [ ] Run `cargo test && cargo clippy -- -D warnings`

### Cycle 5.3: Final Validation

- [ ] **Iteration 1: Full test suite**
  - [ ] RED: Run all tests including integration
  - [ ] GREEN: All pass
  - [ ] REFACTOR: Fix any issues
  - [ ] COMMIT: `test(selection): verify all tests pass after refactoring`

- [ ] **Iteration 2: Documentation update**
  - [ ] RED: Check CLAUDE.md accuracy
  - [ ] GREEN: Update architecture section
  - [ ] REFACTOR: Final polish
  - [ ] COMMIT: `docs: update architecture documentation for selection module`

- [ ] Run full validation suite

### Phase 5 Checkpoint
- [ ] `selection.rs` < 200 lines (facade + submodule declarations)
- [ ] Clean Rust 2018+ module structure (no `mod.rs`)
- [ ] Documentation updated

---

## Validation Commands

Run after each cycle:
```bash
cargo test
cargo clippy -- -D warnings
cargo fmt --check
```

Run after each phase:
```bash
cargo test
cargo clippy -- -D warnings
cargo fmt --check
make test_nvim  # if available
```

## Existing Tests to Preserve

### Unit Tests (in `selection.rs` → move to appropriate modules)
- `test_position_to_point`
- `test_point_to_position`
- `test_selection_range_detects_injection`
- `test_selection_range_respects_offset_directive`
- `test_selection_range_handles_nested_injection`
- `test_nested_injection_includes_content_node_boundary`
- `test_selection_range_parses_injected_content`
- `test_calculate_nested_start_position_handles_negative_offsets`
- `test_column_alignment_when_row_offset_skips_lines`
- `test_selection_range_deduplicates_same_range_nodes`
- `test_selection_range_handles_multibyte_utf8`
- `test_selection_range_output_uses_utf16_columns`
- `test_injected_selection_range_uses_utf16_columns`
- `test_selection_range_maintains_position_alignment`
- `test_selection_range_handles_empty_document`

### Integration Tests (in `tests/test_lsp_select.lua`)
- Lua file selection (no injection)
- Markdown frontmatter expansion (YAML injection)
- Lua code block expansion
- Nested injection (markdown → markdown → lua)

## Success Criteria

- [ ] `selection.rs` reduced to < 200 lines (facade only)
- [ ] No `#[allow(clippy::too_many_arguments)]` remaining (or reduced to 1)
- [ ] Each new module < 300 lines
- [ ] All 15+ unit tests pass
- [ ] All integration tests pass
- [ ] No new clippy warnings
