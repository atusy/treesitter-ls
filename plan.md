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
    * Follow Kent-Beck's tidy first and t-wada's TDD
    * `git commit` on when you achieve GREEN or you make changes on REFACTOR
    * `make format lint test` must pass before `git commit`
    * template of step is

        ``` markdown
        # Step 1: what to achieve

        DoD: ...

        * [ ] RED: implement test
        * [ ] GREEN: implement working code that passes test
        * [ ] CHECK: must pass `make format lint test` without errors and warnings
        * [ ] COMMIT
        * [ ] REFACTOR (tidying)
        * [ ] COMMIT
        ```

    * If you find you need unplaned code changes, insert the plan to plan.md before making any changes.

* My idea of steps
    1. Start adding interfaces to support offset. No calculations are required. offset can be `(0, 0, 0, 0)`
    2. Add rule-based calculation logic for lua, which applies offset by `(0, 1, 0, 0)` when injected language is luadoc.
        * By doing this, `Inspect token` action recognized third hyphen in `---@param x number` as `lua`, not `luadoc`
    3. Add rule-based calculation logic for markdown, which is described in `__ignored/nvim-treesitter/runtime/queries/markdown/injections.scm`
    4. remove rule-based logic
        * this step might be huge and thus can be broken into pieces, but try illustrating from big pictures like the above steps.

# AI's plan

Based on the user request to add `#offset` support for injection captures, I'll break this down into manageable TDD steps following Kent Beck's "Tidy First" approach.

## Step 1: Add offset field to InjectionCapture struct without calculations

DoD: InjectionCapture struct has an offset field that stores (0, 0, 0, 0) by default

* [x] RED: Write test that verifies InjectionCapture has offset field defaulting to (0, 0, 0, 0)
* [x] GREEN: Add offset field to InjectionCapture struct
* [x] CHECK: must pass `make format lint test` without errors and warnings
* [x] COMMIT
* [x] REFACTOR: Extract offset type if needed for clarity
* [x] COMMIT

## Step 2: Integrate offset tracking into injection detection

DoD: Modify `detect_injection_with_content` in `src/language/injection.rs` to return an `InjectionCapture` with offset field (still (0, 0, 0, 0)), and update `refactor.rs` to use this structure. This creates the pipeline for offset data to flow from detection to the inspect action.

* [x] RED: Write test that `detect_injection_with_content` returns InjectionCapture with offset
* [x] GREEN: Modify injection.rs to return InjectionCapture, update refactor.rs to consume it
* [x] CHECK: must pass `make format lint test` without errors and warnings
* [x] COMMIT
* [x] REFACTOR: Simplify the integration if needed
* [x] COMMIT

## Step 3: Apply offset when checking cursor position & implement lua/luadoc rule

DoD:
1. Apply offset to injection boundaries when checking if cursor is within injection
2. Add hardcoded rule: lua->luadoc gets offset (0, 1, 0, 0)
3. Result: third hyphen in `---@param` is recognized as lua, not luadoc

* [x] RED: Write test for lua comment with luadoc injection verifying third hyphen is lua
* [x] GREEN: Add offset application in `is_node_within` check + hardcoded lua->luadoc rule
* [x] CHECK: must pass `make format lint test` without errors and warnings
* [x] COMMIT
* [x] REFACTOR: Extract offset rules to dedicated function
* [x] COMMIT

## Step 3.5: Fix offset implementation to use row/column instead of bytes

DoD: Offsets should be applied to row/column positions, not byte positions directly. The offset (0, 1, 0, 0) means "move start forward by 1 column", not "add 1 byte".

* [x] RED: Write test that verifies offset works correctly with multi-byte characters
* [x] GREEN: Implement proper row/column-based offset application using PositionMapper
* [x] CHECK: must pass `make format lint test` without errors and warnings
* [x] COMMIT
* [x] REFACTOR: Optimize position conversions if needed
* [x] COMMIT

## Step 3.6: Apply offset during injection detection, not after

DoD: The offset should be applied when checking if a node is within injection boundaries during the detection phase, not after. This ensures that positions outside the adjusted boundaries are not detected as being in the injection.

* [x] RED: Write test that verifies position at byte 2 (third hyphen) is NOT detected as being in luadoc injection
* [x] GREEN: Apply offset during `collect_injection_regions` to check adjusted boundaries
* [x] CHECK: must pass `make format lint test` without errors and warnings
* [x] COMMIT
* [ ] REFACTOR: Clean up the detection flow if needed
* [ ] COMMIT

## Step 3.7: Fix refactor.rs to use offset-aware injection detection

DoD: The `src/analysis/refactor.rs` module must use the offset-aware injection detection functions to ensure "Inspect token" code action respects offset boundaries.

* [ ] RED: Write test that verifies refactor.rs respects offset boundaries for "Inspect token" action
* [ ] GREEN: Update refactor.rs to use `detect_injection_with_content_and_offset` and adjusted ranges
* [ ] CHECK: must pass `make format lint test` without errors and warnings
* [ ] COMMIT
* [ ] REFACTOR: Clean up if needed
* [ ] COMMIT

## Step 4: Add rule-based offset calculation for markdown injections

DoD: Support markdown metadata injections (minus_metadata for YAML and plus_metadata for TOML) with offset (1, 0, -1, 0) to exclude the metadata delimiters

* [ ] RED: Write test for markdown minus_metadata (YAML) and plus_metadata (TOML) injections with offset (1, 0, -1, 0)
* [ ] GREEN: Add hardcoded rules for markdown metadata injection offsets
* [ ] CHECK: must pass `make format lint test` without errors and warnings
* [ ] COMMIT
* [ ] REFACTOR: Consolidate rule-based offset logic
* [ ] COMMIT

## Step 5: Parse #offset! directive from injection queries

DoD: Extract offset values from `#offset!` directive in injection.scm queries

* [ ] RED: Write test for parsing #offset! directive from query
* [ ] GREEN: Implement parser for #offset! directive
* [ ] CHECK: must pass `make format lint test` without errors and warnings
* [ ] COMMIT
* [ ] REFACTOR: Improve parsing error handling
* [ ] COMMIT

## Step 6: Apply parsed offset to injection boundaries

DoD: Use parsed offset values to adjust injection capture ranges

* [ ] RED: Write test verifying offset application to capture ranges
* [ ] GREEN: Implement offset application logic
* [ ] CHECK: must pass `make format lint test` without errors and warnings
* [ ] COMMIT
* [ ] REFACTOR: Optimize offset calculations
* [ ] COMMIT

## Step 7: Remove rule-based logic and rely solely on query directives

DoD: All offset logic comes from query directives, no hardcoded rules

* [ ] RED: Write test ensuring no hardcoded rules remain
* [ ] GREEN: Remove rule-based logic, use only parsed directives
* [ ] CHECK: must pass `make format lint test` without errors and warnings
* [ ] COMMIT
* [ ] REFACTOR: Clean up any redundant code
* [ ] COMMIT

