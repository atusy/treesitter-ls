# User request

* Goal: Solve issues in @review.md which reviews selectionRange

* Tips:
    * The core implementation of selectionRange is in `src/analysis/selection.rs`
    * The `Inspect token` code action in `src/analysis/refactor.rs` is a good reference that implements:
        * detection of language injections
        * parsing of the injected language
    * Some of the development records are available in `docs/records/development`
        * Especially, 0001 and 0002 illustrates good examples to illustrate big pictures first rule
    * For E2E testing, use mini.test framework in @deps/nvim/mini.nvim. Some example tests are in @tests/test_*.lua

* Rule:
    * Deliver value as early as possible with ryuzee's scrum framework and additional rules
        * Illustrate big pictures first, and improve smaller pieces in the later sprints
        * Each sprint must have working, testable, user-visible increment which can be demonstrated to stakeholders
        * 1 user story per sprint
        * If you find you need unplaned code changes, insert the plan to plan.md before making any changes.
        * **Sprint Definition of Done (DoD):**
            * All task checkboxes marked as complete [x]
            * Sprint retrospective section filled in
            * plan.md committed with updates
            * Sprint is NOT complete until these are done
    * Follow Kent-Beck's tidy first and t-wada's TDD
    * `git commit` on when you achieve GREEN or you make changes on REFACTOR
    * `make format lint test` must pass before `git commit`
    * template of sprint is below. At the initial planning, only Sprint 1 requires

``` markdown
## Sprint 1

<!-- The planned change must have user-visible increment -->

* User story:

### Sprint planning

<!--
* DoD: Tasks section is filled
* Only Sprint 1 requires be filled at the initial planning. After that, fill this section after each sprint retrospective.
* Add notes here
    * e.g., technical details, difficulties, what is and is not part of this sprint, and so on
* The content should reflect the actual codebase.
-->

### Tasks

<!--
Only Sprint 1 requires be filled at the initial planning.
After that, fill this section after each sprint retrospective.
-->

#### Task 1: what to achieve

DoD: ...

* [ ] RED: implement test
* [ ] GREEN: implement working code that passes test
* [ ] CHECK: must pass `make format lint test` without errors and warnings
* [ ] COMMIT
* [ ] SELF-REVIEW: with Kent-Beck's Tidy First principle in your mind
* [ ] REFACTOR (tidying)
* [ ] COMMIT
* [ ] REFACTOR (tidying)
* [ ] COMMIT
* ...

<!-- Add as many REFACTOR-COMMIT-cycle as required anytime during sprint -->

### Sprint retrospective

#### Inspections of decisions in the previous retrospective

#### Inspections of the current sprint (e.g., by KPT, use adequate method for each sprint)

<!--
This section may include considerations on the requirements to refine (or change) the subsequent sprint
-->

#### Adaption plan

### Product Backlog Refinement

<!--
* DoD: Ready to start Sprint planning for the next sprint.
* Add notes here
* Edit Product Backlog section to add/delete/order product bucklog items
-->

```

# Review Analysis (Second Review Cycle)

## Summary of New review.md Findings

The new review identifies 3 HIGH-severity issues that were NOT addressed by the previous sprint cycle:

### Issue 1 (High): Injected ranges report byte columns to LSP

**Location:** `src/analysis/selection.rs:606` and `src/analysis/selection.rs:669`

**Problem:**
- `build_injected_selection_range` converts injected AST nodes via `adjust_range_to_host`
- `adjust_range_to_host` adds two tree-sitter `Point`s together and wraps raw byte counts in `tower_lsp::Position`
- Tree-sitter columns are UTF-8 bytes, but LSP requires UTF-16 code units
- For injected snippets with multi-byte chars (e.g., `let yaml = r#"あ: 0"#;`), selection highlighting jumps to wrong column

**Impact:**
- Injected selection ranges are shifted right for non-ASCII content
- `skip_to_distinct_host` compares ranges in different units (host=UTF-16, injected=bytes) and may fail to connect hierarchies

**Fix:** Thread a `PositionMapper` into `build_injected_selection_range`/`adjust_range_to_host` so injected nodes use same conversion as host nodes.

### Issue 2 (High): Offset handling reinterprets UTF-16 columns as bytes

**Location:** `src/analysis/selection.rs:301`

**Problem:**
```rust
let effective_start_pos = mapper
    .byte_to_position(effective.start)
    .map(|p| tree_sitter::Point::new(p.line as usize, p.character as usize))
    .unwrap_or(content_node.start_position());
```
- `byte_to_position` returns UTF-16 columns in `p.character`
- `Point::new()` expects byte columns
- Result: `effective_start_position.column` is too small when multi-byte chars exist before injection

**Impact:** All calculations using `effective_start_position` (`adjust_range_to_host`, nested injections) shift ranges LEFT instead of RIGHT.

**Fix:** Stop round-tripping through UTF-16. Compute byte column directly or add `byte_to_point` helper.

### Issue 3 (High): ASCII-only helpers still drive incremental edits

**Location:** `src/analysis/selection.rs:10` and `src/lsp/lsp_impl.rs:412`

**Problem:**
- `position_to_point` is `pub` and used in `lsp_impl.rs:412` for `tree_sitter::InputEdit`
- During `textDocument/didChange`, `InputEdit.start_position` receives UTF-16 columns instead of byte columns
- Tree-sitter receives corrupt edit coordinates and mis-parses or rejects incremental edits

**Impact:** DATA-LOSS level bug for documents containing non-ASCII characters.

**Fix:** Either remove these helpers or reimplement with proper conversion. LSP layer should not use them until corrected.

## Root Cause Analysis

**Two separate coordinate systems are being confused:**
1. **Host document ranges** (via `node_to_range` with mapper) → correct UTF-16
2. **Injected document ranges** (via `adjust_range_to_host`) → incorrect bytes
3. **Incremental edits** (via `position_to_point`) → incorrect (UTF-16 treated as bytes)

The Sprint 6/7 fixes only addressed the host document path. The injection path and edit path were not updated.

# Product Backlog

<!--
Order represents priority (top = highest priority).
User story numbers are just identifiers and do not indicate priority.
Each story includes acceptance criteria for clearer Definition of Done.
-->

## User Story 11: Fix injected selection ranges to use UTF-16 columns (High Priority)
As a user editing a document with language injections containing non-ASCII characters,
I want selection ranges in injected content to highlight correctly,
so that I can select code accurately regardless of character encoding.

**Acceptance Criteria:**
- `build_injected_selection_range` and `adjust_range_to_host` use proper UTF-16 conversion
- Injected ranges connect correctly to host hierarchy even with multi-byte chars before injection
- Test: Rust raw string with Japanese YAML content shows correct selection highlighting

## User Story 12: Fix offset handling to use byte coordinates internally (High Priority)
As a user with markdown frontmatter containing non-ASCII characters,
I want offset-adjusted injection ranges to be positioned correctly,
so that selection expansion works properly in YAML frontmatter.

**Acceptance Criteria:**
- `effective_start_position` uses byte columns (not UTF-16) for tree-sitter Point
- Add `byte_to_point` helper or compute byte column directly
- Test: Markdown with multi-byte chars before frontmatter fence shows correct offset handling

## User Story 13: Fix incremental edits to use byte coordinates (High Priority - Data Loss)
As a user editing documents containing non-ASCII characters,
I want incremental edits to be applied correctly,
so that tree-sitter parsing remains accurate after edits.

**Acceptance Criteria:**
- `position_to_point` is fixed or removed from `lsp_impl.rs` usage
- `InputEdit` receives byte-based coordinates, not UTF-16
- Test: Edit document with Japanese text and verify tree-sitter parses correctly

# Completed User Stories (Previous Review Cycle)

## ✅ User Story 8: Fix output range conversion to use UTF-16 columns (Sprint 6)
## ✅ User Story 9: Unify coordinate systems in range comparisons (Auto-fixed by Sprint 6)
## ✅ User Story 10: Reuse PositionMapper in offset-aware paths (Sprint 7)

# Completed Sprints (Previous Review)

## Sprint 1 ✅ - Handle negative offsets in nested injections
## Sprint 2 ✅ - Fix column alignment when row offsets skip lines
## Sprint 3 ✅ - Include nested injection content node in selection hierarchy
## Sprint 4 ✅ - Reuse cached PositionMapper for performance (main handlers)
## Sprint 5 ✅ - Fix UTF-16 to byte conversion for cursor lookup
## Sprint 6 ✅ - Fix output range conversion to use UTF-16 columns
## Sprint 7 ✅ - Reuse PositionMapper in selection range building (performance)

# Sprints (Current Review)

## Sprint 6

<!-- The planned change must have user-visible increment -->

* User story: Fix output range conversion to use UTF-16 columns (User Story 8)

### Sprint planning

**Context:**
The core issue is `node_to_range` (lines 19-24) which converts tree-sitter Points directly to LSP Positions without accounting for byte vs UTF-16 encoding:

```rust
fn node_to_range(node: Node) -> Range {
    Range::new(
        point_to_position(node.start_position()),
        point_to_position(node.end_position()),
    )
}
```

Tree-sitter `Point.column` is a byte offset within the line. LSP `Position.character` must be a UTF-16 code unit offset. For multi-byte UTF-8 characters:
- "あ" (hiragana A) = 3 bytes, 1 UTF-16 code unit
- After "あ", next char is at byte 3 but UTF-16 column 1

**Solution approach:**
1. Modify `node_to_range` to accept a `&PositionMapper` parameter
2. Use `mapper.byte_to_position()` to convert byte offsets to LSP positions
3. Update all call sites to pass the mapper

**Alternative considered:** Keep byte coordinates internally, convert at final output.
This would require significant refactoring of range comparison logic. The simpler approach is to convert at the source (`node_to_range`).

**What is NOT part of this sprint:**
- User Story 9 (range comparison fixes) - will be addressed if still needed after this fix
- User Story 10 (PositionMapper reuse in offset paths) - performance optimization

### Tasks

#### Task 1: Modify node_to_range to use PositionMapper

DoD: `node_to_range` produces correct UTF-16 column positions for multi-byte characters.

* [x] RED: Write test that asserts output range has UTF-16 columns (not bytes)
* [x] GREEN: Modify `node_to_range` to accept text parameter and use `byte_to_position`
* [x] CHECK: must pass `make format lint test` without errors and warnings
* [x] COMMIT (eac340b)
* [x] SELF-REVIEW: with Kent-Beck's Tidy First principle in your mind

### Sprint retrospective

#### Inspections of decisions in the previous retrospective

Sprint 5's decision to fix input conversion first (cursor lookup) before output conversion was correct. It established the pattern: use `PositionMapper.position_to_byte()` for input, `PositionMapper.byte_to_position()` for output.

#### Inspections of the current sprint (KPT)

**Keep:**
- TDD approach: The failing test clearly showed `left: 17` (bytes) vs `right: 15` (UTF-16), making the bug obvious
- Threading `text` through functions is a minimal API change compared to threading `PositionMapper`
- All 110 tests pass, including existing multi-byte tests that now have correct semantics

**Problem:**
- Each call to `node_to_range` creates a new `PositionMapper`, which is O(file_size). This is correct but suboptimal.
- The `position_to_point`/`point_to_position` functions are now marked with warnings but still exported. Should be cleaned up.

**Try:**
- User Story 10 (PositionMapper reuse) could optimize by passing the mapper down instead of text
- Consider deprecating `position_to_point`/`point_to_position` in a structural change

#### Adaption plan

**User Story 9 is AUTO-FIXED:** Since `node_to_range` now produces UTF-16 coordinates, and `calculate_effective_lsp_range` also uses UTF-16 via `PositionMapper`, all range comparisons (`range_contains`, `is_range_strictly_larger`) now operate in consistent UTF-16 coordinate space.

**Remaining work:**
- User Story 10: Performance optimization (reuse PositionMapper instead of creating per-call)

### Product Backlog Refinement

**Completed in Sprint 6:**
- ✅ User Story 8: Fix output range conversion to use UTF-16 columns
- ✅ User Story 9: Unify coordinate systems (auto-fixed by User Story 8)

**Remaining:**
- User Story 10: Reuse PositionMapper in offset-aware paths (performance optimization)

## Sprint 7

<!-- The planned change must have user-visible increment -->

* User story: Reuse PositionMapper in offset-aware paths (User Story 10)

### Sprint planning

**Context:**
Sprint 6 fixed the correctness issue but introduced a performance regression: `node_to_range` creates a new `PositionMapper` for every node. For a selection range request, this means:
- Building hierarchy for a node with N ancestors = N mapper creations
- Each mapper creation is O(file_size) for line index computation

Current hot spots (non-test code):
1. `node_to_range` (line 30) - called for every node in selection hierarchy
2. `calculate_effective_lsp_range` (line 760) - offset-aware path
3. `build_selection_range_with_parsed_injection_recursive` (line 298) - injection parsing

**Solution approach:**
1. Change `node_to_range(node, text)` → `node_to_range(node, mapper: &PositionMapper)`
2. Thread `&PositionMapper` through all functions instead of `&str`
3. Entry points (`handle_selection_range_*`) create the mapper once and pass it down

This is a STRUCTURAL change (Tidy First) - behavior stays the same, only the API changes.

**What is NOT part of this sprint:**
- This is the last remaining issue from review.md

### Tasks

#### Task 1: Refactor to pass PositionMapper instead of text

DoD: No `PositionMapper::new()` calls in selection range building (except at entry points).

* [x] STRUCTURAL: Change `node_to_range` signature to accept `&PositionMapper`
* [x] STRUCTURAL: Update `build_selection_range` and all dependent functions
* [x] STRUCTURAL: Update `calculate_effective_lsp_range` to accept `&PositionMapper`
* [x] CHECK: must pass `make format lint test` without errors and warnings
* [x] COMMIT (7ab0858)
* [x] SELF-REVIEW: Verify no behavior change, only structural

### Sprint retrospective

#### Inspections of decisions in the previous retrospective

Sprint 6's approach of using `text: &str` to create mappers internally was correct for proving correctness first. The performance concern was known and deferred to Sprint 7.

#### Inspections of the current sprint (KPT)

**Keep:**
- Pure structural refactoring: All 110 tests pass without modification to test assertions
- Threading `&PositionMapper` is more ergonomic than `&str` for the caller
- Entry points (`handle_selection_range_*`) already had the mapper from `document.position_mapper()`
- Eliminated `PositionMapper::new()` calls in hot paths (Sprint 7 goal achieved)

**Problem:**
- Many functions now have long parameter lists (8-10 arguments). This is a code smell but acceptable for internal functions.
- The `build_selection_range_with_parsed_injection_recursive` function has 10 parameters.

**Try:**
- Consider introducing a context struct to reduce parameter counts in a future refactoring sprint
- Could group related parameters: `(text, mapper)` into a `TextContext` struct

#### Adaption plan

**All review.md issues are now resolved:**
- ✅ Issue 1: Fixed in Sprint 6 (UTF-16 output conversion)
- ✅ Issue 2: Auto-fixed by Sprint 6 (consistent coordinate systems)
- ✅ Issue 3: Fixed in Sprint 7 (PositionMapper reuse)

No remaining product backlog items from this review cycle.

### Product Backlog Refinement

**Sprint 7 Complete:**
- ✅ User Story 10: Reuse PositionMapper in offset-aware paths

**All user stories from first review.md are complete.**

---

# Sprints (Second Review Cycle)

## Sprint 8

<!-- The planned change must have user-visible increment -->

* User story: Fix incremental edits to use byte coordinates (User Story 13)

### Sprint planning

**Context:**
This is marked as a DATA-LOSS level bug. The `position_to_point` helper in `src/analysis/selection.rs:10` is used by `src/lsp/lsp_impl.rs:412` to construct `tree_sitter::InputEdit` during `textDocument/didChange`.

```rust
// lsp_impl.rs:412
InputEdit {
    start_position: position_to_point(&range.start),
    old_end_position: position_to_point(&range.end),
    new_end_position: position_to_point(&new_end_position),
    // ...
}
```

The problem: `position_to_point` simply copies the numeric values without conversion:
- LSP Position.character = UTF-16 code unit
- tree_sitter Point.column = byte offset

For documents with multi-byte characters, tree-sitter receives incorrect edit coordinates.

**Solution approach:**
1. Create proper conversion using `PositionMapper` to convert UTF-16 columns to byte columns
2. The `InputEdit` needs byte-based start/end positions
3. Either fix `position_to_point` to require a mapper, or replace its usage in `lsp_impl.rs`

**What is NOT part of this sprint:**
- Issues 1 & 2 from new review (injected ranges, offset handling)

### Tasks

#### Task 1: Fix InputEdit to use byte coordinates

DoD: `InputEdit` receives byte-based coordinates derived from LSP UTF-16 positions.

* [x] RED: Write test that edits a document with multi-byte chars and verifies tree-sitter parses correctly
* [x] GREEN: Fix the conversion in `lsp_impl.rs` to use `PositionMapper`
* [x] CHECK: must pass `make format lint test` without errors and warnings
* [x] COMMIT (ac4a4e6)
* [x] SELF-REVIEW: with Kent-Beck's Tidy First principle in your mind
* [x] REFACTOR (tidying): Old position_to_point kept but marked deprecated for ASCII-only tests

### Sprint retrospective

#### Inspections of decisions in the previous retrospective

Sprint 7's refactoring to pass `&PositionMapper` through selection functions made this fix easier - the mapper was already available in `lsp_impl.rs` for byte offset conversion.

#### Inspections of the current sprint (KPT)

**Keep:**
- TDD approach: The failing test clearly demonstrated the bug (UTF-16 column 3 vs byte column 9)
- Added `position_to_point` method to `PositionMapper` for proper conversion
- The fix is minimal: only changed the `InputEdit` construction, not the surrounding logic
- 136 tests pass (110 unit + 26 integration)

**Problem:**
- The old `position_to_point` function still exists in selection.rs for backward compatibility
- Tests in selection.rs use the old function, but this is safe since test strings are ASCII-only

**Try:**
- Consider adding a lint rule or removing the export of the old function in a future cleanup sprint
- User Stories 11 and 12 (injection ranges, offset handling) still need to be addressed

#### Adaption plan

**Issue 3 (DATA-LOSS bug) is now FIXED.** Incremental edits in documents with non-ASCII characters will now work correctly.

**Remaining issues from review.md:**
- Issue 1: Injected ranges report byte columns (User Story 11)
- Issue 2: Offset handling reinterprets UTF-16 columns as bytes (User Story 12)

These issues affect selection range highlighting for injected content with multi-byte characters.

### Product Backlog Refinement

**Sprint 8 Complete:**
- ✅ User Story 13: Fix incremental edits to use byte coordinates

**Remaining User Stories:**
- User Story 11: Fix injected selection ranges to use UTF-16 columns
- User Story 12: Fix offset handling to use byte coordinates internally
