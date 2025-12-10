# Selection Range Refactoring Plan - Phase 2

## Current State Analysis (Post-Phase 1-3.1)

The initial refactoring extracted 13 pure functions into three submodules:
- `hierarchy_chain.rs` (360 LOC, 6 functions) - Range comparison and chaining
- `range_builder.rs` (87 LOC, 3 functions) - AST to SelectionRange conversion
- `injection_aware.rs` (155 LOC, 4 functions) - Coordinate translation utilities

**What Remains in `selection.rs` (~460 LOC):**
```
├── handle_selection_range()                    # LSP entry point (4 params) ✓ Good
├── build_selection_range_with_parsed_injection() # 8 params ⚠️ Too many
├── build_recursive_injection_selection()       # 11 params ⚠️ Too many
├── build_injected_selection_range()            # 3 params ✓ Good
├── build_unparsed_injection_selection()        # 4 params ✓ Good
├── replace_range_in_chain()                    # Private helper
└── splice_effective_range_into_hierarchy()     # Private helper
```

**Key Problems Identified:**
1. **Parameter Explosion**: Functions with 8-11 parameters indicate missing abstractions
2. **Mixed Concerns**: Parser acquisition/release mixed with selection building
3. **Implicit Coupling**: `(coordinator, parser_pool)` always passed together
4. **Context Repetition**: `(text, mapper, root)` form a logical unit but passed separately

## Goal

Introduce context structs to:
1. Reduce parameter counts from 8-11 to 3-4
2. Separate resource management from selection logic
3. Make dependencies explicit through types
4. Enable easier testing through mock injection

## Target Architecture

```
src/analysis/
├── selection.rs              # Public API: handle_selection_range + re-exports
└── selection/
    ├── hierarchy_chain.rs    # Pure range utilities (DONE)
    ├── range_builder.rs      # Pure AST→SelectionRange (DONE)
    ├── injection_aware.rs    # Coordinate translation (DONE)
    ├── context.rs            # NEW: InjectionContext, DocumentContext
    └── injection_builder.rs  # NEW: Injection-aware selection building
```

---

## Phase 4: Introduce Context Structs (Structural)

**Rationale:** Group related parameters into cohesive structs to reduce function signatures.

### Cycle 4.1: DocumentContext Struct

Bundle document-level information that's always passed together.

- [ ] **Iteration 1: Define `DocumentContext` struct**
  - [ ] RED: Create `src/analysis/selection/context.rs` with struct definition
  - [ ] GREEN: Define struct with `text`, `mapper`, `root` fields
  - [ ] REFACTOR: Add documentation and derive traits
  - [ ] COMMIT: `refactor(selection): add DocumentContext struct`

- [ ] **Iteration 2: Add constructor methods**
  - [ ] RED: Add test for `DocumentContext::new()`
  - [ ] GREEN: Implement constructor
  - [ ] REFACTOR: Consider adding `From<&DocumentHandle>` impl
  - [ ] COMMIT: `refactor(selection): add DocumentContext constructor`

- [ ] Run `cargo test`

### Cycle 4.2: InjectionContext Struct

Bundle injection-related resources and state.

- [ ] **Iteration 1: Define `InjectionContext` struct**
  - [ ] RED: Add `InjectionContext` struct definition
  - [ ] GREEN: Define struct with `coordinator`, `parser_pool`, `depth` fields
  - [ ] REFACTOR: Use lifetime annotations for references
  - [ ] COMMIT: `refactor(selection): add InjectionContext struct`

- [ ] **Iteration 2: Add helper methods**
  - [ ] RED: Add test for `InjectionContext::acquire_parser()`
  - [ ] GREEN: Implement parser acquisition wrapper
  - [ ] REFACTOR: Add `release_parser()` method
  - [ ] COMMIT: `refactor(selection): add InjectionContext parser methods`

- [ ] **Iteration 3: Add depth tracking**
  - [ ] RED: Add test for `InjectionContext::descend()`
  - [ ] GREEN: Implement depth increment with MAX_DEPTH check
  - [ ] REFACTOR: Return `Option` to handle depth limit
  - [ ] COMMIT: `refactor(selection): add InjectionContext depth tracking`

- [ ] Run `cargo test && cargo clippy -- -D warnings`

### Phase 4 Checkpoint
- [ ] `context.rs` contains `DocumentContext` and `InjectionContext`
- [ ] Structs have constructor methods and documentation
- [ ] All existing tests pass

---

## Phase 5: Refactor Injection Builder Functions (Behavioral)

**Rationale:** Use context structs to simplify function signatures.

### Cycle 5.1: Extract Injection Builder Module

- [ ] **Iteration 1: Create injection_builder.rs**
  - [ ] RED: Create module with placeholder
  - [ ] GREEN: Add module declaration in selection.rs
  - [ ] REFACTOR: Add module documentation
  - [ ] COMMIT: `refactor(selection): create injection_builder module`

### Cycle 5.2: Migrate `build_injected_selection_range`

This function is already small (3 params) but should move to the new module.

- [ ] **Iteration 1: Move function**
  - [ ] RED: Move to `injection_builder.rs`
  - [ ] GREEN: Update imports in selection.rs
  - [ ] REFACTOR: None needed
  - [ ] COMMIT: `refactor(selection): move build_injected_selection_range to injection_builder`

### Cycle 5.3: Refactor `build_selection_range_with_parsed_injection`

Reduce from 8 params to 3 using context structs.

**Current signature:**
```rust
fn build_selection_range_with_parsed_injection(
    node: Node,
    root: &Node,
    text: &str,
    mapper: &PositionMapper,
    base_language: &str,
    coordinator: &LanguageCoordinator,
    parser_pool: &mut DocumentParserPool,
    cursor_byte: usize,
) -> SelectionRange
```

**Target signature:**
```rust
fn build_selection_range_with_parsed_injection(
    node: Node,
    doc_ctx: &DocumentContext,
    inj_ctx: &mut InjectionContext,
    cursor_byte: usize,
) -> SelectionRange
```

- [ ] **Iteration 1: Add new signature alongside old**
  - [ ] RED: Add `_with_context` variant
  - [ ] GREEN: Implement by delegating to original
  - [ ] REFACTOR: None yet
  - [ ] COMMIT: `refactor(selection): add build_selection_range_with_parsed_injection_with_context`

- [ ] **Iteration 2: Move logic to new function**
  - [ ] RED: Ensure tests still pass
  - [ ] GREEN: Move implementation to `_with_context` variant
  - [ ] REFACTOR: Update original to delegate to new version
  - [ ] COMMIT: `refactor(selection): migrate build_selection_range_with_parsed_injection to context`

- [ ] **Iteration 3: Remove old signature**
  - [ ] RED: Update all callers to use context version
  - [ ] GREEN: Remove deprecated function
  - [ ] REFACTOR: Rename `_with_context` to original name
  - [ ] COMMIT: `refactor(selection): complete migration to context-based injection builder`

- [ ] Run `cargo test`

### Cycle 5.4: Refactor `build_recursive_injection_selection`

Reduce from 11 params to 4 using context structs.

**Current signature:**
```rust
fn build_recursive_injection_selection(
    node: &Node,
    root: &Node,
    text: &str,
    injection_query: &Query,
    base_language: &str,
    coordinator: &LanguageCoordinator,
    parser_pool: &mut DocumentParserPool,
    cursor_byte: usize,
    parent_start_byte: usize,
    mapper: &PositionMapper,
    depth: usize,
) -> SelectionRange
```

**Target signature:**
```rust
fn build_recursive_injection_selection(
    node: &Node,
    text: &str,                      // Current injection's text (changes per level)
    doc_ctx: &DocumentContext,       // Host document context (stable)
    inj_ctx: &mut InjectionContext,  // Manages coordinator, pool, depth
    cursor_byte: usize,
    parent_start_byte: usize,
) -> SelectionRange
```

- [ ] **Iteration 1-3: Same pattern as Cycle 5.3**
  - [ ] Add `_with_context` variant
  - [ ] Move implementation
  - [ ] Remove old signature
  - [ ] COMMIT each step

- [ ] Run `cargo test && cargo clippy -- -D warnings`

### Phase 5 Checkpoint
- [ ] `injection_builder.rs` contains injection-aware selection functions
- [ ] No functions with >6 parameters
- [ ] Removed all `#[allow(clippy::too_many_arguments)]`
- [ ] All existing tests pass

---

## Phase 6: Move Hierarchy Splicing Functions (Structural)

**Rationale:** Complete the extraction of injection-related helpers.

### Cycle 6.1: Move `replace_range_in_chain`

- [ ] **Iteration 1: Move to injection_aware.rs**
  - [ ] RED: Move function
  - [ ] GREEN: Update imports
  - [ ] REFACTOR: Make public if needed by injection_builder
  - [ ] COMMIT: `refactor(selection): move replace_range_in_chain to injection_aware`

### Cycle 6.2: Move `splice_effective_range_into_hierarchy`

- [ ] **Iteration 1: Move to injection_aware.rs**
  - [ ] RED: Move function
  - [ ] GREEN: Update imports
  - [ ] REFACTOR: Consider renaming for clarity
  - [ ] COMMIT: `refactor(selection): move splice_effective_range_into_hierarchy to injection_aware`

### Cycle 6.3: Move `build_unparsed_injection_selection`

- [ ] **Iteration 1: Move to injection_builder.rs**
  - [ ] RED: Move function
  - [ ] GREEN: Update imports
  - [ ] REFACTOR: None needed
  - [ ] COMMIT: `refactor(selection): move build_unparsed_injection_selection to injection_builder`

- [ ] Run `cargo test && cargo clippy -- -D warnings`

### Phase 6 Checkpoint
- [ ] `selection.rs` reduced to <100 lines (facade only)
- [ ] All helper functions moved to appropriate modules
- [ ] Clean module hierarchy

---

## Phase 7: Final Cleanup (Structural)

### Cycle 7.1: Organize Exports

- [ ] **Iteration 1: Review public API**
  - [ ] RED: Check which functions need to be public
  - [ ] GREEN: Minimize public surface
  - [ ] REFACTOR: Add `pub(crate)` where appropriate
  - [ ] COMMIT: `refactor(selection): minimize public API surface`

### Cycle 7.2: Documentation Update

- [ ] **Iteration 1: Module documentation**
  - [ ] RED: Review all module docs
  - [ ] GREEN: Update to reflect new architecture
  - [ ] REFACTOR: Add architecture diagram in selection.rs
  - [ ] COMMIT: `docs(selection): update module documentation`

### Cycle 7.3: Remove Dead Code

- [ ] **Iteration 1: Check for unused code**
  - [ ] RED: Run `cargo clippy -- -D dead_code`
  - [ ] GREEN: Remove any unused functions
  - [ ] REFACTOR: None needed
  - [ ] COMMIT: `refactor(selection): remove dead code`

- [ ] Run full validation suite

### Phase 7 Checkpoint
- [ ] `selection.rs` < 100 lines
- [ ] No `#[allow(clippy::too_many_arguments)]` remaining
- [ ] All modules < 200 lines
- [ ] Clean public API

---

## Success Criteria

- [ ] `selection.rs` reduced to facade only (< 100 lines)
- [ ] No functions with > 6 parameters
- [ ] All `#[allow(clippy::too_many_arguments)]` removed
- [ ] Each module has single responsibility:
  - `context.rs`: Context struct definitions
  - `hierarchy_chain.rs`: Pure range utilities
  - `range_builder.rs`: Pure AST → SelectionRange
  - `injection_aware.rs`: Coordinate translation
  - `injection_builder.rs`: Injection-aware selection building
- [ ] All 161+ tests pass
- [ ] No new clippy warnings

## Validation Commands

Run after each cycle:
```bash
cargo test
cargo clippy -- -D warnings
cargo fmt --check
```

---

## Alternative Approaches Considered

### A. Builder Pattern
```rust
SelectionRangeBuilder::new(document)
    .with_injection_support(coordinator, parser_pool)
    .at_position(cursor_byte)
    .build()
```
**Rejected:** Adds complexity for marginal benefit. Context structs are simpler.

### B. Trait-based Abstraction
```rust
trait SelectionBuilder {
    fn build(&self, node: Node) -> SelectionRange;
}
```
**Rejected:** Over-engineering for internal module. No external consumers.

### C. Keep Current Structure
**Rejected:** 8-11 parameter functions are a maintenance burden and indicate missing abstractions.
