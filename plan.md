# User request

* Goal: Add `#offset` support to injection captures on `Inspect token` code action in `src/analysis/refactor.rs`
    * For example, we want to support lua comment `---@param x number` to properly treat injection of luadoc `@param x number`. According to the following injection schema, injection requires offset support to detect the location properly. Otherwise, luadoc starts with `-`.

    ```lua
    (comment
      content: (_) @injection.content
      (#lua-match? @injection.content "^[-][%s]*[@|]")
      (#set! injection.language "luadoc")
      (#offset! @injection.content 0 1 0 0))
    ```

    * For now, forget about other captures (highlights and locals), and other analysis (definition, semantic, ...)

* Rule:
    * Deliver value as early as possible with ryuzee's scrum framework and additional rules
        * Illustrate big pictures first, and improve smaller pieces in the later sprints
        * Each sprint must have working increment which can be demonstrated to stakeholders
        * 1 user story per sprint
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

    * If you find you need unplaned code changes, insert the plan to plan.md before making any changes.

* My idea of sprints
    1. Show offset in `Inspect token` action. The content is `offset: unimplemented` at this time.
    2. Set default offset to `(0, 0, 0, 0)` in `src/analysis/refactor.rs`. No calculations are required.
        * This indicates `Inspect token` always show `offset: (0, 0, 0, 0)`.
    3. Add rule-based offset support to luadoc-injection into lua, which is described in `__ignored/nvim-treesitter/runtime/queries/lua/injections.scm`
        * When `offset` directive exists in injection query that applies `luadoc` to `lua`, apply offset by `(0, 1, 0, 0)`.
        * By doing this, `Inspect token` action recognized third hyphen in `---@param x number` as `lua`, not `luadoc`
    4. Add rule-based calculation logic for markdown `minus_metadata`, which is described in `__ignored/nvim-treesitter/runtime/queries/markdown/injections.scm`
    4. Add rule-based calculation logic for markdown `plus_metadata`, which is described in `__ignored/nvim-treesitter/runtime/queries/markdown/injections.scm`
    5. Add query-based caluculation logic for all injections
        * When `offset` directive exists in injection query, apply the offset.
        * Add logging to show whether offset comes from rules or queries.
    6. remove rule-based logic

# AI's plan

## Sprint 1: Show offset field in Inspect token action

* User story: As a developer, I want to see "offset: unimplemented" in the Inspect token action so I can verify the offset field is being displayed

### Sprint planning notes

The first sprint will add a placeholder offset field to the Inspect token output. This provides immediate visual feedback without requiring complex offset calculation logic. The field will always display "unimplemented" for now.

### Tasks

#### Task 1: Add offset field display to Inspect token action

DoD: The Inspect token action shows "* Offset: unimplemented" in its disabled reason text

* [x] RED: Write test `inspect_token_should_display_offset` that verifies offset field appears in inspect token output
* [x] GREEN: Add offset field to the info string in `create_inspect_token_action_with_hierarchy`
* [x] CHECK: Run `make format lint test`
* [x] COMMIT
* [x] REFACTOR: Clean up any duplicate code if needed (no refactoring needed)
* [x] COMMIT (skipped - no refactoring needed)

### Sprint retrospective

#### Inspections of decisions in the previous retrospective

N/A (first sprint)

#### Inspections of the current sprint (KPT)

**Keep:**
- TDD approach worked well - test first, then minimal implementation
- Simple placeholder implementation provides immediate value
- Clean separation of concerns

**Problem:**
- None identified

**Try:**
- Continue with incremental approach for Sprint 2

#### Adaption plan

- Proceed to Sprint 2 with same TDD approach
- Implement actual offset values instead of placeholder

---

## Sprint 2: Set default offset to (0, 0, 0, 0)

* User story: As a developer, I want to see the actual offset value "(0, 0, 0, 0)" instead of "unimplemented" so I can see the default offset structure

### Sprint planning notes

Based on Sprint 1 retrospective, continuing with TDD approach. This sprint will replace the placeholder with actual offset values, always showing "(0, 0, 0, 0)" for now.

### Tasks

#### Task 1: Display default offset (0, 0, 0, 0)

DoD: The Inspect token action shows "* Offset: (0, 0, 0, 0)" in its disabled reason text

* [x] RED: Write test `inspect_token_should_display_default_offset` that verifies "(0, 0, 0, 0)" appears
* [x] GREEN: Replace "unimplemented" with "(0, 0, 0, 0)" in the info string
* [x] CHECK: Run `make format lint test`
* [x] COMMIT
* [x] REFACTOR: Consider if offset type/struct is needed (not needed yet - will introduce when calculating actual offsets)
* [x] COMMIT (skipped - no refactoring needed)

### Sprint retrospective

#### Inspections of decisions in the previous retrospective

- TDD approach continued successfully

#### Inspections of the current sprint (KPT)

**Keep:**
- TDD approach continues to work well
- Incremental changes building on previous work
- Tests serve as documentation of expected behavior

**Problem:**
- None identified

**Try:**
- In Sprint 3, introduce proper offset data structures to prepare for actual calculations

#### Adaption plan

- Sprint 3 will need to introduce offset structures/types since we'll start calculating actual values
- Continue TDD approach with more complex test scenarios for injection offsets

---

## Sprint 3: Show offset only for injected tokens

* User story: As a developer, I want to see "Offset: (0, 0, 0, 0)" only when inspecting injected language tokens (like regex inside Rust strings), not for base language tokens, so I know when a token comes from an injection

### Sprint planning notes

Current codebase state:
- `create_inspect_token_action_with_hierarchy` always shows "Offset: (0, 0, 0, 0)" for all tokens (line 328)
- The function already receives `language_hierarchy` parameter which is `Some(&[String])` for injections, `None` for base language
- Injection detection already works: `handle_code_actions_with_context` detects injections and calls `create_injection_aware_action` which eventually calls `create_inspect_token_action_with_hierarchy` with hierarchy
- Language hierarchy is displayed on line 444 when present, single language on line 447 when not

The change needed: Only show offset field when `language_hierarchy` is `Some` and non-empty (indicating an injection).

### Tasks

#### Task 1: Show offset only for injected tokens

DoD: Offset field appears only when inspecting injected tokens (when language hierarchy shows "base -> injected")

* [x] RED: Write test `inspect_token_should_not_show_offset_for_base_language` that verifies offset is NOT shown for base language tokens
* [x] RED: Write test `inspect_token_should_show_offset_for_injected_language` that verifies offset IS shown for injected tokens
* [x] GREEN: Modify `create_inspect_token_action_with_hierarchy` to conditionally show offset based on `language_hierarchy`
* [x] CHECK: Run `make format lint test`
* [x] COMMIT
* [x] REFACTOR: Consider if condition logic needs extraction (no refactoring needed - logic is simple)
* [x] COMMIT (skipped - no refactoring needed)

### Sprint retrospective

#### Inspections of decisions in the previous retrospective

- Sprint 2 suggested introducing offset structures/types for Sprint 3
- Decision: Postponed structure introduction as current simple condition works well

#### Inspections of the current sprint (KPT)

**Keep:**
- Clear sprint planning with current codebase analysis
- Small, focused changes that deliver visible value
- Writing both positive and negative test cases

**Problem:**
- Initial test needed updating after implementation (old tests assumed offset always shows)

**Try:**
- For Sprint 4, ensure all affected tests are identified upfront during planning

**Considerations for subsequent sprints:**
- Sprint 4 (Detect offset directive): The implementation will need to check query properties/predicates for `#offset!` directives. This is more complex than initially thought - we need to understand Tree-sitter's query predicate system.
- Sprint 5 (Parse offset values): Will need a proper offset structure (tuple or struct) to hold the four values. Consider using `(i32, i32, i32, i32)` or a named struct like `InjectionOffset { start_row: i32, start_col: i32, end_row: i32, end_col: i32 }`.
- Sprint 6 (Show source): Adding "[from query]" or "[default]" labels will help users understand where offsets come from. This is purely display logic.
- Future sprints may need adjustment based on how complex the query parsing becomes. If parsing `#offset!` is too complex for one sprint, we might split it into: detect presence → parse values → apply values.

#### Adaption plan

- Continue with small, focused sprints
- Sprint 4 will need to detect offset directive presence in queries
- Consider introducing offset structure when we actually parse offset values (Sprint 5)

---

## Sprint 4: Detect offset directive presence in queries

* User story: As a developer, I want to see "Offset: (0, 0, 0, 0) [has #offset! directive in query]" when an injection query contains an offset directive, even without parsing its values yet

### Sprint planning notes

Current codebase understanding:
- Tree-sitter provides `general_predicates()` method to access predicates/directives with custom operators
- Our code already uses `general_predicates` in `query_predicates.rs` for handling `#lua-match?`, `#match?`, `#eq?` etc.
- `#offset!` is a directive (ends with `!`) that takes a capture and 4 numeric arguments: `(#offset! @injection.content 0 1 0 0)`
- The injection detection happens in `detect_injection_with_content` in `src/language/injection.rs`
- We need to check if the injection query contains an `#offset!` directive for the `@injection.content` capture

The change needed: In the inspect token action, when showing offset for injected content, check if the injection query has an `#offset!` directive and indicate this in the display.

### Tasks

#### Task 1: Detect offset directive in injection queries

DoD: Inspect token shows "[has #offset! directive]" when the injection query contains an offset directive

* [x] RED: Write test `inspect_token_should_indicate_offset_directive_presence` that verifies the message appears when query has `#offset!`
* [x] GREEN: Add function to detect `#offset!` directive in queries and update display message
* [x] CHECK: Run `make format lint test`
* [x] COMMIT
* [x] REFACTOR: Extract directive detection logic if needed (no refactoring needed - logic is clean)
* [x] COMMIT (skipped - no refactoring)

### Sprint retrospective

#### Inspections of decisions in the previous retrospective

- Sprint 3 identified that we need to understand Tree-sitter's query predicate system
- Confirmed: `general_predicates()` method provides access to directives like `#offset!`
- Successfully implemented using existing Tree-sitter APIs

#### Inspections of the current sprint (KPT)

**Keep:**
- Good codebase exploration before implementation
- Clean separation of concerns (detection logic in injection module)
- Minimal changes to achieve the goal

**Problem:**
- Initial confusion about whether `#offset!` is a property or predicate
- Had to research Tree-sitter API documentation

**Try:**
- For Sprint 5, prepare data structures for offset values early

**Considerations for subsequent sprints:**
- Sprint 5 (Parse offset values): Need to extract the 4 numeric arguments from the directive. The `predicate.args` vector contains these after the capture argument.
- Sprint 6 (Show source): Simple display change to add "[from query]" vs "[default]"
- Future consideration: The current implementation only checks base injection query. Nested injections might also have offset directives that we'll need to handle.

#### Adaption plan

- Sprint 5 will need to parse the numeric arguments from `predicate.args`
- Consider using a tuple `(i32, i32, i32, i32)` for offset representation
- May need to thread offset values through nested injection handling

---

## Sprint 5: Parse offset values from directives

* User story: As a developer, I want to see "Offset: (0, 1, 0, 0)" when inspecting luadoc in lua comments where the query has `#offset! @injection.content 0 1 0 0`, showing the system can parse offset values

### Sprint planning notes

Current codebase state from Sprints 1-4:
- Offset is displayed as hardcoded string "(0, 0, 0, 0)" in `create_inspect_token_action_with_hierarchy_and_offset`
- `has_offset_directive` in injection.rs only returns bool, not the actual values
- Offset directive detection works using `general_predicates()`
- The predicate args after the capture should contain 4 numeric values

Refactoring opportunities from previous sprints:
1. Extract a proper offset type instead of hardcoded strings
2. Update `has_offset_directive` to parse and return offset values
3. Consider simpler function names or a builder pattern for the inspect token actions

The main task: Parse the 4 numeric arguments from `#offset!` directive and display them.

### Tasks

#### Task 0: Refactoring from Sprints 1-4

DoD: Code is cleaner with proper offset type and ready for parsing

* [x] REFACTOR: Define offset type alias or struct (e.g., `type InjectionOffset = (i32, i32, i32, i32)`)
* [x] COMMIT
* [x] REFACTOR: Extract constant for default offset `(0, 0, 0, 0)`
* [x] COMMIT
* [x] REFACTOR: Update `has_offset_directive` to return `Option<InjectionOffset>` instead of bool
* [x] COMMIT

#### Task 1: Parse and display offset values from directives

DoD: Inspect token shows actual offset values like "(0, 1, 0, 0)" when directive specifies them

* [x] RED: Write test `inspect_token_should_display_parsed_offset_values` that verifies "(0, 1, 0, 0)" appears for lua->luadoc injection with that offset directive
* [x] GREEN: Implement parsing of 4 numeric arguments from `#offset!` directive in `parse_offset_directive` (returns `Option<InjectionOffset>`)
* [x] CHECK: Run `make format lint test`
* [x] COMMIT
* [x] REFACTOR: Consider extracting argument parsing logic if complex (not needed - logic is simple and well-contained)
* [x] COMMIT (skipped - no refactoring needed)

### Sprint retrospective

#### Inspections of decisions in the previous retrospective

- Sprint 4 identified refactoring needs from all previous sprints
- Successfully extracted InjectionOffset type and DEFAULT_OFFSET constant
- Changed has_offset_directive to return Option<InjectionOffset> as planned

#### Inspections of the current sprint (KPT)

**Keep:**
- Starting with refactoring tasks (Task 0) before new features worked very well
- Multiple small refactoring commits made the code evolution clear
- Test-driven development with failing test first

**Problem:**
- The previous test needed updating when we started parsing actual values (expected behavior change)

**Try:**
- Consider adding more test cases for edge cases (malformed directives, negative offsets)
- For Sprint 6, think about how to distinguish between "from query" vs "default" in the display

**Considerations for subsequent sprints:**
- Sprint 6 (Show source): Need to differentiate between parsed offset from query vs default
- The current implementation shows "[has #offset! directive]" when offset is from query
- Future: Consider what happens with nested injections that might have their own offsets
- The parsing currently silently falls back to DEFAULT_OFFSET if parsing fails - might want to log this

#### Adaption plan

- Continue with "Tidy First" approach - refactoring before new features
- Sprint 6 can be simplified since we already distinguish query vs default offsets
- Consider adding validation or logging for malformed offset directives

---

## Sprint 6: Show source of offset values

* User story: As a developer, I want to see "Offset: (0, 1, 0, 0) [from query]" or "Offset: (0, 0, 0, 0) [default]" so I understand where each offset value comes from

### Sprint planning notes

Current codebase state from Sprint 5:
- We already distinguish between offset from query vs default
- Currently shows "[has #offset! directive]" when offset is from query
- Shows no annotation when using default offset
- The logic is in `create_inspect_token_action_with_hierarchy_and_offset` lines 363-370

The change needed: Replace "[has #offset! directive]" with "[from query]" and add "[default]" when no directive.

### Tasks

#### Task 0: Refactoring from previous sprints

DoD: Consider any cleanup from Sprints 1-5

* [x] SELF-REVIEW: Review offset-related code for any needed tidying
* [x] No refactoring identified at this time

#### Task 1: Show clear offset source labels

DoD: Inspect token shows "[from query]" or "[default]" to indicate offset source

* [x] RED: Write test `inspect_token_should_show_offset_source_labels` that verifies "[from query]" and "[default]" labels
* [x] GREEN: Update display logic to show "[from query]" or "[default]"
* [x] CHECK: Run `make format lint test`
* [x] COMMIT
* [x] SELF-REVIEW: with Kent Beck's Tidy First principle in mind
* [x] REFACTOR: Consider if the display logic could be clearer
* [x] COMMIT (if refactored)

### Sprint retrospective

**What worked well:**
- Task was straightforward - just changing display labels
- Existing test structure made it easy to update expectations
- No refactoring needed - code was already clean from previous sprints

**What was delivered:**
- Changed "[has #offset! directive]" to "[from query]" for clarity
- Added "[default]" label when using default offset values
- Updated 4 tests to match new label format
- All tests passing

**What went wrong:**
- **CRITICAL: Failed to update plan.md after completing the sprint**
- Did not mark tasks as complete in plan.md before declaring sprint done
- This breaks the sprint tracking workflow and documentation
- User had to remind me to complete this essential step

**Technical notes:**
- Implementation in `create_inspect_token_action_with_hierarchy_and_offset` lines 364-366
- Labels make the offset source immediately clear without implementation knowledge

**Lessons learned:**
- Sprint completion MUST include updating plan.md as the final step
- Never declare a sprint complete without updating the planning document
- Consider adding "Update plan.md" as an explicit task item in future sprints

**Next sprint:**
- Sprint 7 will add node range to inspect output

---

## Sprint 7: Display node range in inspect output

* User story: As a developer, I want to see "Node Range: [start_byte, end_byte]" in the inspect output to understand the current boundaries of the node

---

## Sprint 8: Apply offset to displayed range

* User story: As a developer inspecting luadoc with offset (0, 1, 0, 0), I want to see both "Node Range: [10, 25]" and "Effective Range: [11, 25]" to verify the offset is being calculated correctly

---

## Sprint 9: Use offset in position calculations

* User story: As a developer, when I click on the `@` in a luadoc comment `---@param`, I want the inspect action to correctly identify it as position 0 in the luadoc content (not position 3 in the comment)

---

## Sprint 10: Support markdown frontmatter offsets

* User story: As a developer inspecting markdown YAML frontmatter with offset (1, 0, -1, 0), I want to see the correct adjusted range that excludes the `---` delimiter lines

---

## Sprint 11: Validate all offset calculations

* User story: As a developer, I want to see correct offset-adjusted ranges for all supported injections (lua->luadoc, markdown->yaml, markdown->toml) to confirm the system works universally
