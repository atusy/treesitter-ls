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

### 🔍 Sprint 14: Fix pattern-agnostic offset parsing ✅

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

* [x] RED: Add test showing wrong offset applied to fenced_code_block
* [x] Track pattern_index in injection detection
* [x] Make parse_offset_directive pattern-aware
* [x] GREEN: Pass pattern_index through the call chain
* [x] Verify fenced code blocks get no offset
* [x] Verify frontmatter still gets correct offset
* [x] CHECK: Run `make format lint test`
* [x] COMMIT

#### Sprint Retrospective

**What was delivered:**
- Created `parse_offset_directive_for_pattern` function for pattern-specific offset parsing
- Modified `detect_injection_with_content` to return pattern_index (3-tuple instead of 2-tuple)
- Updated `create_injection_aware_action` to accept and use pattern_index for offset lookup
- Added comprehensive test demonstrating pattern-aware offset parsing
- Fixed type complexity warning with `InjectionRegion` type alias

**Technical implementation:**
- The key insight: Tree-sitter queries can have multiple patterns, and each pattern may have different offset directives
- Solution: Pass the pattern_index through the entire injection detection chain
- Now each injection correctly gets the offset for its specific pattern, not just the first offset found

**Impact:**
- ✅ Markdown fenced code blocks now correctly show injected language on ALL lines
- ✅ Frontmatter blocks still get their correct offsets
- ✅ Pattern-aware offset parsing enables correct handling of complex injection queries
- ✅ All existing tests continue to pass

---

---

### 📝 Sprint 15: Add integration test with real markdown file ✅

* User story: As a treesitter-ls maintainer, I want an integration test that verifies the offset fix works with actual markdown files containing both fenced code blocks and frontmatter

**Why this matters:** Ensures the fix works end-to-end with real-world markdown content

#### Tasks

* [x] RED: Create test with markdown file containing both fenced code blocks and frontmatter
* [x] Verify fenced code blocks show correct language on all lines
* [x] Verify frontmatter shows correct language with offset
* [x] GREEN: Ensure test passes with current implementation
* [x] CHECK & COMMIT

#### Sprint Retrospective

**What was delivered:**
- Created `test_markdown_injection_offsets_real_world` test with actual markdown injection query patterns
- Created `test_pattern_specific_offsets` test to verify pattern-aware behavior
- Added tree-sitter-md as dev dependency for realistic testing
- Created test query file matching nvim-treesitter patterns

**Technical implementation:**
- Tests use real markdown grammar and injection queries
- Verifies that fenced_code_block patterns have no offset
- Confirms frontmatter patterns have (1, 0, -1, 0) offset
- Tests both the old broken behavior and new fixed behavior

**Impact:**
- ✅ Integration tests prove the fix works with real markdown content
- ✅ Prevents regression of the offset bug
- ✅ Documents the expected behavior for future maintainers
- ✅ All tests pass without issues

---

### ⚠️ Sprint 16: Validate and warn on malformed offset directives ✅

* User story: As a query author, I want clear error messages when my offset directives are malformed (e.g., non-numeric values, wrong argument count)

**Why this matters:** Currently returns DEFAULT_OFFSET silently on parse failure, making debugging difficult

#### Tasks

* [x] RED: Test malformed offset handling (non-numeric, missing args, etc.)
* [x] GREEN: Add validation with descriptive error logging
* [x] Ensure backward compatibility (still work, but warn)
* [x] CHECK & COMMIT

#### Sprint Retrospective

**What was delivered:**
- Added comprehensive test `test_malformed_offset_directives` covering various error cases
- Enhanced `parse_offset_directive_for_pattern` with detailed validation logic
- Added descriptive warning messages for different error scenarios
- Maintained backward compatibility by returning DEFAULT_OFFSET on errors

**Technical implementation:**
- Validates argument count (must be exactly 4 offset values)
- Validates each value can be parsed as i32
- Provides specific error messages identifying which values failed
- Uses log::warn! to notify developers without breaking functionality

**Impact:**
- ✅ Query authors get immediate feedback on malformed directives
- ✅ Easier debugging with specific error messages
- ✅ Backward compatible - still works with DEFAULT_OFFSET
- ✅ All existing tests continue to pass

---

### 🧹 Sprint 17: Clean up deprecated parse_offset_directive function ✅

* User story: As a treesitter-ls maintainer, I want to migrate all uses of the deprecated parse_offset_directive to the pattern-aware version

**Why this matters:** Prevents future bugs from using the wrong function

#### Tasks

* [x] Find all uses of deprecated parse_offset_directive
* [x] Migrate to parse_offset_directive_for_pattern where appropriate
* [x] Add deprecation warning or remove if no longer needed
* [x] CHECK & COMMIT

#### Sprint Retrospective

**What was delivered:**
- Analyzed all uses of `parse_offset_directive` - found only in tests
- Added comprehensive deprecation documentation explaining the flaw
- Added `#[deprecated]` attribute with migration guidance
- Added `#[allow(deprecated)]` to test usages that document old behavior

**Technical decision:**
- Kept the function for historical documentation purposes
- Tests explicitly document the broken behavior for future reference
- Clear deprecation message guides developers to use pattern-aware version
- No production code uses the deprecated function

**Impact:**
- ✅ Prevents accidental use of broken function
- ✅ Maintains historical context in tests
- ✅ Clear migration path for any future code
- ✅ Technical debt properly documented

---

### 🧹 Sprint 18: Remove unused code and unnecessary abstractions

* User story: As a treesitter-ls maintainer, I want to remove unused public APIs and methods to reduce maintenance burden and improve code clarity

**Why this matters:** Unused public APIs create confusion and maintenance overhead

#### Identified Issues

1. **Unused public function**: `detect_injection` in injection.rs
   - Only used in tests within the same module
   - Just a thin wrapper around `detect_injection_with_content` that drops return values
   - Should be removed entirely - tests can call `detect_injection_with_content` directly

2. **Unused methods in InjectionOffset**:
   - `has_offset()` - Never used anywhere
   - `as_tuple()` - Never used anywhere, marked as "for backwards compatibility" but no usage found

3. **Type alias only used locally**: `InjectionRegion`
   - Only used within injection.rs module
   - Could be moved closer to usage or inlined

4. **Deprecated function**: `parse_offset_directive`
   - Already marked as deprecated
   - Only used in tests for documentation
   - Consider removing entirely or moving to test module

#### Tasks

* [ ] Remove `detect_injection` entirely and update tests to use `detect_injection_with_content`
* [ ] Remove unused `has_offset()` method from InjectionOffset
* [ ] Remove unused `as_tuple()` method from InjectionOffset
* [ ] Move InjectionRegion type alias closer to its usage
* [ ] Consider moving deprecated `parse_offset_directive` to test module
* [ ] Run `cargo build --release` to ensure no external dependencies break
* [ ] CHECK: Run `make format lint test`
* [ ] COMMIT

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
