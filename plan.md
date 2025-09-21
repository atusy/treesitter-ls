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

* User story:

### Sprint planning notes

<!-- 
Only Sprint 1 requires be filled at the initial planning.
After that, fill this section after each sprint retrospective.
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
* [ ] REFACTOR (tidying)
* [ ] COMMIT

### Sprint retrospective

#### Inspections of decisions in the previous retrospective

#### Inspections of the current sprint (e.g., by KPT, use adequate method for each sprint)

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
