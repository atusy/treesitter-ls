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
