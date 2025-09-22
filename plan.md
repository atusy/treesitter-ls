# User request

* Goal: Let `Inspect token` code action in `src/analysis/refactor.rs` support offset-drective when detecting language from nested injections.

* Tips:
    * We already implemented offset detection logic in recent commits and the process is recorded in `docs/records/development/0001-code-action-inspect-token-show-injection-offsets.md` and `docs/records/development/0002-code-action-inspect-token-be-aware-of-offset-on-language-detection.md`

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

### Sprint planning notes

<!-- 
Only Sprint 1 requires be filled at the initial planning.
After that, fill this section after each sprint retrospective.

The content should reflect the actual codebase.
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

<--
This section may include considerations on the requirements to refine (or change) the subsequent sprint
-->

#### Adaption plan

```

# AI's plan

## Sprint 19: Apply offsets to nested injection detection

* User story: As a developer, when I inspect code with nested injections that have offsets (like Lua comments with luadoc), I want the language hierarchy to correctly respect offset boundaries at each nesting level

### Sprint planning notes

Current state analysis:
- `handle_nested_injection` in refactor.rs detects nested injections
- Line 412-419: Calls `detect_injection_with_content` and gets `_nested_pattern_index`
- **BUG**: The pattern index is ignored (prefixed with `_`)
- `process_nested_injection` uses `nested_content_node.byte_range()` without applying offsets
- This means nested injections don't respect offset directives

The fix needed:
1. Use the `nested_pattern_index` to get the offset for that pattern
2. Apply the offset when determining if cursor is within the nested injection
3. Pass offset-adjusted ranges to the nested parsing

### Tasks

#### Task 1: Pass pattern_index through nested injection detection

DoD: Nested injection detection uses pattern_index to apply offsets correctly

* [x] RED: Write test showing nested injection with offset incorrectly detected
* [x] GREEN: Use pattern_index to get offset and check cursor position
* [x] CHECK: Run `make format lint test`
* [x] COMMIT
* [x] SELF-REVIEW: Check for any cleanup needed
* [x] REFACTOR (extracted `is_within_effective_range` helper)
* [x] COMMIT

### Sprint retrospective

#### Inspections of decisions in the previous retrospective

N/A - This is the first sprint for nested injection offset support.

#### Inspections of the current sprint (KPT)

**Keep:**
- TDD approach worked well - wrote tests first to document expected behavior
- Reused existing offset calculation functions effectively
- Pattern-aware offset detection now works consistently across all injection levels

**Problem:**
- Initial test for markdown failed due to wrong node selection
- Some code duplication between parent and nested injection offset checks

**Try:**
- Extracted helper function to reduce duplication
- Fixed test to properly find the injection content nodes

**What was delivered:**
- Nested injection detection now respects offset directives
- Used pattern_index (was previously ignored with underscore prefix)
- Added offset checking before processing nested injections
- Passes pattern_index and injection_query through the recursion chain
- Extracted `is_within_effective_range` helper to reduce duplication
- Added comprehensive tests for both with and without offset scenarios

**Technical implementation:**
- `handle_nested_injection` now uses `nested_pattern_index` instead of `_nested_pattern_index`
- Calls `parse_offset_directive_for_pattern` with the correct pattern index
- Checks cursor position against effective range before processing
- `process_nested_injection` now accepts and propagates offset information

#### Adaption plan

- Sprint 19 successfully completed
- Core functionality for offset-aware nested injection detection is now working
- Consider future improvements for semantic tokens and selection ranges with injection support
- Main goal achieved: Inspect token action now correctly respects offsets at all injection levels

---

# Historical Planning (Archived)
