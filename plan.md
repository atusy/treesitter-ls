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

# Product Backlog

<!--
Order represents priority (top = highest priority).
User story numbers are just identifiers and do not indicate priority.
Each story includes acceptance criteria for clearer Definition of Done.
-->

## User Story 1: Handle negative offsets in nested injections
As a user editing markdown with YAML frontmatter (which uses `#offset! ... -1 0`),
I want selection range to expand correctly through nested injections,
so that I can select the entire YAML content without broken coordinates.

**Acceptance Criteria:**
- Negative `start_row`/`start_column` values from `InjectionOffset` are handled with saturating arithmetic
- `calculate_nested_start_position` accepts signed parameters
- Tests demonstrate correct behavior with negative offsets

## User Story 2: Fix column alignment when row offsets skip lines
As a user with injected content that starts on a later row (e.g., code after a fence line),
I want the column positions to be calculated correctly,
so that selection ranges land on the correct characters.

**Acceptance Criteria:**
- Column calculation considers the effective row after applying offset
- When `offset_rows > 0`, the column should NOT add parent's column if we've moved to a new row

## User Story 3: Include nested injection content node in selection hierarchy
As a user expanding selection in deeply nested injections,
I want to be able to select the exact boundary of each injection region,
so that I can "select the whole nested snippet" at each level.

**Acceptance Criteria:**
- The host chain for nested injections includes the actual capture node
- Users can expand to select exactly the nested content boundary

# Sprints

## Sprint 1

<!-- The planned change must have user-visible increment -->

* User story: Handle negative offsets in nested injections (User Story 1)

### Sprint planning

**Context:**
The issue is in `build_nested_injection_selection` (lines 445-458) and `calculate_nested_start_position` (lines 536-553).

The `InjectionOffset` struct uses `i32` for its fields (`start_row`, `start_column`, etc.) because offset directives like markdown's `(#offset! @injection.content -1 0 0 0)` use negative values to trim content.

However, the current code casts these `i32` values to `usize`:
```rust
off.start_row as usize,
off.start_column as usize,
```

This causes:
- Debug builds: panic on negative values
- Release builds: astronomically large values (due to two's complement wrapping)

**Solution approach:**
1. Change `calculate_nested_start_position` to accept `i32` parameters for offsets
2. Use saturating arithmetic to handle negative offsets (e.g., `saturating_sub`)
3. Remove the unsafe `as usize` casts at the call site

**What is NOT part of this sprint:**
- Issue 2 (column alignment) - requires its own test and analysis
- Issue 3 (missing injection.content node) - separate concern

### Tasks

#### Task 1: Fix negative offset handling in calculate_nested_start_position

DoD: Negative offsets in `InjectionOffset` are handled with saturating arithmetic, preventing panic/garbage values.

* [x] RED: Write test that demonstrates negative offset handling
* [x] GREEN: Modify `calculate_nested_start_position` to accept `i32` and use saturating arithmetic
* [x] CHECK: must pass `make format lint test` without errors and warnings
* [x] COMMIT
* [x] SELF-REVIEW: with Kent-Beck's Tidy First principle in your mind

### Sprint retrospective

#### Inspections of decisions in the previous retrospective

N/A (first sprint)

#### Inspections of the current sprint (KPT)

**Keep:**
- TDD approach worked well: the test immediately exposed the type mismatch
- Saturating arithmetic pattern is defensive and clear
- Single-purpose commit with behavioral change only

**Problem:**
- None identified

**Try:**
- Consider whether Issue 2 (column alignment) and Issue 3 (missing content node) can be addressed in a single sprint since they're related to the same function

#### Adaption plan

Proceed to Sprint 2 to address Issue 2 (column alignment). The fix in calculate_nested_start_position needs to account for the effective row when determining column behavior.

### Product Backlog Refinement

Issue 2 and Issue 3 both affect nested injection handling. They could potentially be combined into one sprint since they're small and related, but keeping them separate maintains the "one story per sprint" rule and allows for focused testing.

## Sprint 2

<!-- The planned change must have user-visible increment -->

* User story: Fix column alignment when row offsets skip lines (User Story 2)

### Sprint planning

**Context:**
Issue 2 from review.md: When `offset_rows > 0` is applied to skip a line (e.g., skipping a fence line `\`\`\`lua`), the column calculation incorrectly still adds the parent's column because it only checks if `content_start.row == 0`.

Current code (after Sprint 1 fix):
```rust
let col = if content_start.row == 0 {
    // First row of content - add parent's column
    let base_col = (parent_start.column + content_start.column) as i64;
    (base_col + offset_cols as i64).max(0) as usize
} else {
    // Later rows - column is absolute within the parent
    let base_col = content_start.column as i64;
    (base_col + offset_cols as i64).max(0) as usize
};
```

The problem: If `offset_rows > 0`, the effective row is NOT row 0 of the original content. The condition should check the **effective** row (after applying offset), not the raw `content_start.row`.

**Solution approach:**
Change the condition from `content_start.row == 0` to checking if the effective row is 0:
```rust
let effective_row_is_first = (content_start.row as i32 + offset_rows) == 0;
```

Wait, that's still not quite right. The issue is about whether we're on the same row as the parent. Let me reconsider...

Actually the semantics are: if we're parsing starting from row 0 of the *effective* content, we need to consider the host's column position. The offset_rows shifts where we start parsing. So:
- If offset_rows = 0 and content_start.row = 0, we're on the same row as parent → add parent column
- If offset_rows > 0, we've moved to a later row → column is absolute (no parent column needed)
- If offset_rows < 0, we've moved backwards (edge case) → still consider first-row behavior

The fix: check if `content_start.row as i32 + offset_rows == 0` to determine if effective row is the parent's row.

**What is NOT part of this sprint:**
- Issue 3 (missing injection.content node)

### Tasks

#### Task 1: Fix column alignment when row offset is applied

DoD: Column positions are correctly calculated when offset_rows moves the effective start to a different row.

* [x] RED: Write test that demonstrates incorrect column when offset_rows > 0
* [x] GREEN: Fix the condition to check effective row
* [x] CHECK: must pass `make format lint test` without errors and warnings
* [x] COMMIT
* [x] SELF-REVIEW: with Kent-Beck's Tidy First principle in your mind

### Sprint retrospective

#### Inspections of decisions in the previous retrospective

The Sprint 1 retrospective suggested considering combining Issues 2 and 3. We kept them separate, which was the right call - Issue 2 required updating an existing test's expected behavior, which would have complicated a combined sprint.

#### Inspections of the current sprint (KPT)

**Keep:**
- The TDD cycle caught an interaction with an existing test (test case 4 in negative offset test)
- This revealed that the old test was testing incorrect behavior, validating our fix
- Clear documentation in test comments explaining the semantics

**Problem:**
- None

**Try:**
- Issue 3 involves a different code path (`build_nested_injection_selection` at lines 524-529) that chains to `content_node.parent()` instead of including `content_node` itself

#### Adaption plan

Proceed to Sprint 3 for Issue 3. The fix is localized to how we build the host chain for nested injections.

### Product Backlog Refinement

Issue 3 is the last review issue. After this sprint, all three issues from review.md will be resolved.

## Sprint 3

<!-- The planned change must have user-visible increment -->

* User story: Include nested injection content node in selection hierarchy (User Story 3)

### Sprint planning

**Context:**
Issue 3 from review.md: When chaining a nested injection back into its parent, the code starts the host chain at `content_node.parent()` (lines 524-529):

```rust
// Chain nested selection to parent injected content
// Get the parent's selection starting from content_node's parent
let parent_selection = content_node
    .parent()
    .map(|parent| build_injected_selection_range(parent, root, parent_start_position));
```

This skips the actual `content_node` itself, so users cannot expand to "select the whole nested snippet". In contrast, the top-level path includes the content node via `build_selection_range(content_node)` (lines 374-382).

**Solution approach:**
Include `content_node` in the chain before its parent, similar to how the top-level path does it:
```rust
let content_node_selection = build_injected_selection_range(content_node, root, parent_start_position);
let parent_selection = content_node
    .parent()
    .map(|parent| build_injected_selection_range(parent, root, parent_start_position));
// Chain: nested → content_node → parent → ...
```

Or simpler: just start from `content_node` instead of `content_node.parent()`:
```rust
let parent_selection = Some(build_injected_selection_range(content_node, root, parent_start_position));
```

This way `content_node` is included in the chain (with its range adjusted for parent position).

**What is NOT part of this sprint:**
- All three issues will be complete after this sprint

### Tasks

#### Task 1: Include content_node in nested injection selection chain

DoD: The selection hierarchy for nested injections includes the content node boundary.

* [ ] RED: Write test that verifies content_node range is in the selection chain
* [ ] GREEN: Change chain start from content_node.parent() to content_node
* [ ] CHECK: must pass `make format lint test` without errors and warnings
* [ ] COMMIT
* [ ] SELF-REVIEW: with Kent-Beck's Tidy First principle in your mind

### Sprint retrospective

#### Inspections of decisions in the previous retrospective

#### Inspections of the current sprint (e.g., by KPT, use adequate method for each sprint)

#### Adaption plan

### Product Backlog Refinement

