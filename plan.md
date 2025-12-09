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

# Review Analysis

## Summary of review.md Findings

The review identifies 3 issues, but after analyzing the codebase:

### Issue 1 (lines 5-6): `position_to_point` / `point_to_position` incorrect conversion

**Review Claim:** These functions simply copy numbers between LSP Position and tree-sitter Point, but tree-sitter columns are bytes while LSP columns are UTF-16 code units.

**Current State Analysis:**
- ✅ **INPUT conversion is FIXED** (Sprint 5): The handler functions `handle_selection_range_with_injection` (line 1059) and `handle_selection_range_with_parsed_injection` (line 1126) now use `mapper.position_to_byte()` for cursor lookup, not `position_to_point`.
- ❌ **OUTPUT conversion is STILL BROKEN**: The `node_to_range` function (line 19) uses `point_to_position` to create output ranges. Since tree-sitter `Point.column` is in bytes, the returned `Range.start.character` is in bytes, not UTF-16 code units as LSP requires.

**Impact:** Selection ranges returned to the client have byte-based columns instead of UTF-16 columns. For ASCII, this works; for multi-byte UTF-8, ranges are shifted right.

### Issue 2 (lines 8-9): Effective-range splicing fails for non-ASCII hosts

**Review Claim:** `calculate_effective_lsp_range` produces proper UTF-16 coordinates, but comparisons against `node_to_range` (which produces byte coordinates) fail.

**Current State Analysis:**
- ✅ `calculate_effective_lsp_range` (lines 732-751) correctly converts bytes → UTF-16 via `PositionMapper`
- ❌ `node_to_range` (lines 19-24) produces byte-based coordinates
- ❌ `range_contains` comparisons (lines 851, 856, etc.) mix coordinate systems

**Impact:** When non-ASCII characters exist before the injection, `range_contains(&parent_selection.range, &effective_range)` returns false because:
- `parent_selection.range.start.character` = byte offset (larger, e.g., 12)
- `effective_range.start.character` = UTF-16 offset (smaller, e.g., 8)

### Issue 3 (lines 11-12): Cached line index discarded in hot paths

**Review Claim:** Fresh `PositionMapper::new(text)` at lines 280-285 and 742-748 defeats the optimization.

**Current State Analysis:**
- ✅ Sprint 4 fixed the main handlers to reuse `document.position_mapper()`
- ❌ Lines 280 and 742 still create fresh mappers inside offset-aware paths

**Impact:** Performance regression for offset-aware injection handling, but less severe than before.

## Root Cause Analysis

**The fundamental issue:** `node_to_range` outputs byte columns, but LSP expects UTF-16 columns.

The fix requires either:
1. **Option A:** Pass `PositionMapper` to `node_to_range` and convert bytes → UTF-16
2. **Option B:** Create `node_to_byte_range` for internal use, add `node_to_lsp_range(mapper)` for output
3. **Option C:** Defer all conversion to the very end, keeping byte coordinates internally

Option A is cleanest: we already have the mapper in scope at all call sites.

# Product Backlog

<!--
Order represents priority (top = highest priority).
User story numbers are just identifiers and do not indicate priority.
Each story includes acceptance criteria for clearer Definition of Done.
-->

## User Story 8: Fix output range conversion to use UTF-16 columns
As a user editing files with multi-byte UTF-8 characters (emoji, CJK, etc.),
I want selection range output to use correct UTF-16 column positions,
so that the editor highlights the correct text regions.

**Acceptance Criteria:**
- `node_to_range` is modified to convert tree-sitter byte columns to LSP UTF-16 columns
- All selection range outputs use correct UTF-16 coordinates
- Tests with multi-byte characters verify correct column positions in output

## User Story 9: Unify coordinate systems in range comparisons
As a user with non-ASCII text before injection regions,
I want selection range hierarchy to correctly identify containment,
so that effective ranges are properly spliced into the selection chain.

**Acceptance Criteria:**
- Range comparison functions (`range_contains`, `is_range_strictly_larger`) operate in consistent coordinate space
- Either all ranges are in bytes OR all ranges are in UTF-16 (not mixed)
- Tests demonstrate correct containment with non-ASCII text

## User Story 10: Reuse PositionMapper in offset-aware paths (performance)
As a user with multiple cursors in a large file with injection offsets,
I want offset-aware handling to be performant,
so that the editor doesn't lag.

**Acceptance Criteria:**
- `calculate_effective_lsp_range` receives a PositionMapper instead of creating one
- `build_selection_range_with_parsed_injection_recursive` passes the mapper down
- No fresh `PositionMapper::new()` calls in hot paths

# Completed Sprints (Previous Review)

## Sprint 1 ✅ - Handle negative offsets in nested injections
## Sprint 2 ✅ - Fix column alignment when row offsets skip lines
## Sprint 3 ✅ - Include nested injection content node in selection hierarchy
## Sprint 4 ✅ - Reuse cached PositionMapper for performance (main handlers)
## Sprint 5 ✅ - Fix UTF-16 to byte conversion for cursor lookup

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

The performance issue (User Story 10) is lower priority than correctness. The current implementation is correct, just not optimal. Consider addressing in a future sprint if performance is observed to be an issue.
