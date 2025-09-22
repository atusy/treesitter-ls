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

### 🎯 Sprint 12: CORE - Language field respects offset

* User story: As a treesitter-ls user, when I inspect the third hyphen in `---@param` with luadoc offset (0, 1, 0, 0), I see `Language: lua` not `Language: lua -> luadoc`

**This is THE main deliverable that fulfills the user request**

#### Tasks

* [ ] RED: Test that third hyphen shows `lua` not `lua -> luadoc`
* [ ] GREEN: Use effective range (with offset) for language detection
* [ ] CHECK: `make format lint test`
* [ ] COMMIT

---

### 🔧 Sprint 13: Handle nested injections correctly

* User story: As a treesitter-ls user with nested injections (markdown→html→js), I want Language field to respect all offset layers

**Why this matters:** Without this, nested injections will show wrong languages

#### Tasks

* [ ] RED: Test cumulative offsets in nested injections
* [ ] GREEN: Track and apply offsets through injection hierarchy
* [ ] CHECK & COMMIT

---

### ⚠️ Sprint 14: Validate and warn on bad offsets

* User story: As a query author, I want warnings when my offset directives are malformed

**Why this matters:** Silent failures make debugging hard

#### Tasks

* [ ] RED: Test invalid offset handling
* [ ] GREEN: Add validation and logging
* [ ] CHECK & COMMIT

---

### 🚀 Sprint 15: Performance - Cache offset calculations

* User story: As a treesitter-ls user with large files, I want fast offset calculations

**Why this matters:** Performance optimization (lower priority than correctness)

#### Tasks

* [ ] RED: Performance test showing repeated calculations
* [ ] GREEN: Add caching layer
* [ ] CHECK & COMMIT

---

### 🔮 Sprint 16: Future - Dynamic offset calculations

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
