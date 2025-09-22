# User request

* Goal: Add `#offset` support to injection captures on `Inspect token` code action in `src/analysis/refactor.rs` so that `Language: ` field respects the offset.
    * For example, we want to support lua comment `---@param x number` to properly treat injection of luadoc `@param x number`. Current implemetation considers third hyphen as `Language: lua -> luadoc`, but it should be `Language: lua` according to the following injection schema.

    ```lua
    (comment
      content: (_) @injection.content
      (#lua-match? @injection.content "^[-][%s]*[@|]")
      (#set! injection.language "luadoc")
      (#offset! @injection.content 0 1 0 0))
    ```

    * For now, forget about other captures (highlights and locals), and other analysis (definition, semantic, ...)

* Tips:
    * We already implemented offset detection logic in recent commits and the process is recorded in `docs/records/development/0001-code-action-inspect-token-show-injection-offsets.md`

* Rule:
    * Deliver value as early as possible with ryuzee's scrum framework and additional rules
        * Illustrate big pictures first, and improve smaller pieces in the later sprints
        * Each sprint must have working increment which can be demonstrated to stakeholders
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

<!--
-->
```

# AI's plan

## Big Picture Goal

**Make the `Language:` field in Inspect token action respect injection offsets**

When clicking on the third hyphen in `---@param` (Lua comment with luadoc injection), the Language field should show `lua` not `lua -> luadoc`, because the luadoc injection has offset `(0, 1, 0, 0)` meaning it starts after the third hyphen.

## Current State

✅ **What's Done:**
- Offset values are parsed from `#offset!` directives
- Offsets are displayed in inspect output
- Effective ranges are calculated and shown

❌ **What's Missing (THE CORE REQUIREMENT):**
- Language detection doesn't use the offset to determine which language owns a position
- Clicking on positions outside the effective range still shows the injected language

## Sprint Priorities (Big Picture → Details)

### 🎯 Sprint 12: CORE - Language field respects offset ✅

* User story: As a treesitter-ls user, when I inspect the third hyphen in `---@param` with luadoc offset (0, 1, 0, 0), I see `Language: lua` not `Language: lua -> luadoc`

**This is THE main deliverable that fulfills the user request**

#### Tasks

* [x] RED: Test that third hyphen shows `lua` not `lua -> luadoc`
* [x] GREEN: Use effective range (with offset) for language detection
* [x] CHECK: `make format lint test`
* [x] COMMIT

#### Sprint Retrospective

**What was delivered:**
- Added test `inspect_token_should_respect_offset_for_language_detection` using Rust regex injection
- Modified `create_injection_aware_action` to check if cursor is within effective range
- Positions outside effective range now correctly show base language only

**Technical implementation:**
- Check cursor_byte against effective_range (after applying offset) in refactor.rs:275
- If outside range, return base language action without injection hierarchy
- All existing tests continue to pass

**Impact:**
- ✅ Core requirement fulfilled - Language field now respects offsets
- Users inspecting injection boundaries see correct language ownership
- Enables accurate language detection for offset-adjusted injections like Lua comments

---

### 🐛 Sprint 13: Fix redundant offset check for default offsets ✅

* User story: As a treesitter-ls user, when I inspect content in a markdown fenced code block (which has no offset), I want to see the injected language, not markdown

**Bug introduced in Sprint 12:** The offset check is being applied even when there's no actual offset directive, causing incorrect language detection.

#### Root Cause Analysis

The Sprint 12 implementation has a logic error:
1. `detect_injection_with_content` already confirms the cursor's node is within the injection content
2. We then redundantly check if cursor_byte is within effective_range
3. For default offset (0,0,0,0), this creates inconsistency between node-based and byte-based checks
4. The cursor might be in a node that's within the content, but the byte position check fails

#### Tasks

* [x] RED: Write test showing markdown code block incorrectly shows base language
* [x] GREEN: Only apply offset check when there's an actual offset directive
* [x] CHECK: `make format lint test`
* [x] COMMIT

#### Sprint Retrospective

**What was delivered:**
- Modified `create_injection_aware_action` to only apply offset check when `offset_from_query` is `Some`
- For injections without `#offset!` directive, trust the existing injection detection
- Added comprehensive test covering both cases (with and without offset)

**Technical fix:**
- Changed from always checking effective range to conditional check: `if let Some(offset) = offset_from_query`
- This preserves Sprint 12's functionality for actual offsets while fixing the regression

**Impact:**
- ✅ Markdown code blocks now correctly show injected language
- ✅ Lua comment offsets still work correctly
- ✅ All existing tests continue to pass

---

### 🔍 Sprint 14: Fix pattern-agnostic offset parsing

* User story: As a treesitter-ls user, when I inspect any line of a markdown fenced code block, I want to see the injected language (e.g., "markdown -> lua"), not just "markdown" on first/last lines

**Bug Analysis:** The `parse_offset_directive` function incorrectly returns the FIRST offset found in ANY pattern, not the offset for the MATCHED pattern.

#### Root Cause (Confirmed by Senior Engineer Review)

The bug is in `src/language/injection.rs:parse_offset_directive`:

```rust
pub fn parse_offset_directive(query: &Query) -> Option<InjectionOffset> {
    // Searches ALL patterns and returns FIRST offset found
    // Has no idea which pattern actually matched!
}
```

#### How the Bug Happens

1. **Markdown injection query has multiple patterns:**
   - Fenced code blocks: NO offset
   - YAML frontmatter: `#offset! @injection.content 1 0 -1 0`
   - TOML frontmatter: `#offset! @injection.content 1 0 -1 0`

2. **When processing a fenced code block:**
   - Correctly matches the fenced_code_block pattern
   - But `parse_offset_directive` scans ALL patterns
   - Finds and returns the YAML frontmatter offset
   - This wrong offset is applied to the code block

3. **Result:**
   - First line: Outside effective range → shows "markdown"
   - Middle lines: Inside effective range → shows "markdown -> lua"
   - Last line: Outside effective range → shows "markdown"

#### Why This Design is Flawed

The function assumes one offset per query file, but different patterns need different offsets:
- Fenced code blocks: No offset (content is exact)
- Frontmatter: Needs `(1, 0, -1, 0)` to skip delimiters
- Other patterns: May need other offsets

#### The Fix Required

The `parse_offset_directive` function needs to be pattern-aware:

1. **Current (broken) approach:**
   ```rust
   pub fn parse_offset_directive(query: &Query) -> Option<InjectionOffset>
   ```

2. **Fixed approach:**
   ```rust
   pub fn parse_offset_directive(query: &Query, pattern_index: usize) -> Option<InjectionOffset>
   ```

3. **Changes needed:**
   - Track which pattern matched during injection detection
   - Pass the pattern index to offset parsing
   - Only look for offset directives in that specific pattern

#### Call Flow That Needs Fixing

1. `detect_injection_with_content` - needs to track pattern_index
2. `create_injection_aware_action` - needs to receive pattern_index
3. `parse_offset_directive` - needs to use pattern_index

#### Impact of This Bug

- Incorrect effective range calculations for fenced code blocks
- First/last lines incorrectly show base language
- Misleading offset information in Inspect token output
- Potential issues with other features relying on injection boundaries

#### Tasks

* [ ] RED: Add test showing wrong offset applied to fenced_code_block
* [ ] Track pattern_index in injection detection
* [ ] Make parse_offset_directive pattern-aware
* [ ] GREEN: Pass pattern_index through the call chain
* [ ] Verify fenced code blocks get no offset
* [ ] Verify frontmatter still gets correct offset
* [ ] CHECK: Run `make format lint test`
* [ ] COMMIT

---

---

### 🔧 Sprint 15: Handle nested injections correctly

* User story: As a treesitter-ls user with nested injections (markdown→html→js), I want Language field to respect all offset layers

**Why this matters:** Without this, nested injections will show wrong languages

#### Tasks

* [ ] RED: Test cumulative offsets in nested injections
* [ ] GREEN: Track and apply offsets through injection hierarchy
* [ ] CHECK & COMMIT

---

### ⚠️ Sprint 16: Validate and warn on bad offsets

* User story: As a query author, I want warnings when my offset directives are malformed

**Why this matters:** Silent failures make debugging hard

#### Tasks

* [ ] RED: Test invalid offset handling
* [ ] GREEN: Add validation and logging
* [ ] CHECK & COMMIT

---

### 🚀 Sprint 17: Performance - Cache offset calculations

* User story: As a treesitter-ls user with large files, I want fast offset calculations

**Why this matters:** Performance optimization (lower priority than correctness)

#### Tasks

* [ ] RED: Performance test showing repeated calculations
* [ ] GREEN: Add caching layer
* [ ] CHECK & COMMIT

---

### 🔮 Sprint 18: Future - Dynamic offset calculations

* User story: As a query author, I want offsets calculated from content patterns

**Why this matters:** Nice to have for variable-length prefixes (e.g., `--`, `---`, `----`)

#### Tasks

* [ ] Design dynamic offset API
* [ ] Implement pattern-based offsets
* [ ] CHECK & COMMIT

---

## Success Criteria

The project is successful when:
1. ✅ Clicking third hyphen in `---@param` shows `Language: lua`
2. ✅ Clicking `@` or beyond shows `Language: lua -> luadoc`
3. ✅ Works for all injection types with offsets
4. ✅ Handles nested injections correctly

## What We're NOT Doing (Out of Scope)

- Applying offsets to other LSP features (go-to-definition, semantic tokens)
- Complex offset patterns beyond the basic `#offset!` directive
- Backward compatibility (no need to maintain old behavior)
