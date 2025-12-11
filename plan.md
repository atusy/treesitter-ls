# AI-Agentic Scrum Dashboard

## Rules

### General Principles

1. **Single Source of Truth**: This dashboard is the only place for Scrum artifacts. All agents read from and write to this file.
2. **Git as History**: Do not add timestamps. Git tracks when changes were made.
3. **Order is Priority**: Items higher in lists have higher priority. No separate priority field needed.

### Product Backlog Management

1. **User Story Format**: Every PBI must have a `story` block with `role`, `capability`, and `benefit`.
2. **Ordering**: Product Owner reorders by moving items up/down in the YAML array.
3. **Refinement**: Change status from `draft` -> `refining` -> `ready` as stories mature.

### Definition of Ready (AI-Agentic)

**Ready = AI can complete it without asking humans.**

| Status | Meaning |
|--------|---------|
| `draft` | Initial idea. Needs elaboration. |
| `refining` | Being refined. AI may be able to make it `ready`. |
| `ready` | All information available. AI can execute autonomously. |

**Refinement process**:
1. AI attempts to refine `draft`/`refining` items autonomously (explore codebase, propose acceptance criteria, identify dependencies)
2. If AI can fill in all gaps -> change status to `ready`
3. If story is too big or unclear -> try to split it
4. If unsplittable item still needs human help -> keep as `refining` and document the question

**Prioritization**: Prefer `ready` items. Work on refinement when no `ready` items exist or while waiting for human input.

### Sprint Structure (AI-Agentic)

**1 Sprint = 1 PBI**

Unlike human Scrum where Sprints are time-boxed to amortize event overhead, AI agents have no such constraint. Scrum events are instant for AI, so we maximize iterations by:

- Each Sprint delivers exactly one PBI
- Sprint Planning = select top `ready` item from backlog
- Sprint Review/Retro = run after every PBI completion
- No fixed duration - Sprint ends when PBI is done

**Benefits**: Faster feedback, simpler planning, cleaner increments, easier rollback.

### Sprint Execution (TDD Workflow)

1. **One PBI per Sprint**: Select the top `ready` item. That's the Sprint Backlog.
2. **TDD Subtask Breakdown**: Break the PBI into subtasks. Each subtask produces commits through Red-Green-Refactor:
   - `test`: What behavior to verify (becomes the Red phase test)
   - `implementation`: What to build to make the test pass (Green phase)
   - `type`: `behavioral` (new functionality) or `structural` (refactoring only)
   - `status`: Current TDD phase (`pending` | `red` | `green` | `refactoring` | `completed`)
   - `commits`: Array tracking each commit made for this subtask
3. **TDD Cycle Per Subtask (Commit-Based)**:
   - **Red**: Write a failing test, commit it (`phase: red`), status becomes `red`
   - **Green**: Implement minimum code to pass, commit it (`phase: green`), status becomes `green`
   - **Refactor**: Make structural improvements, commit each one separately (`phase: refactor`), status becomes `refactoring`
   - **Complete**: All refactoring done, status becomes `completed`
4. **Multiple Refactor Commits**: Following Tidy First, make small, frequent structural changes. Each refactor commit should be a single logical improvement (rename, extract method, etc.).
5. **Commit Discipline**: Each commit represents one TDD phase step. Never mix behavioral and structural changes in the same commit.
6. **Full Event Cycle**: After PBI completion, run Review -> Retro -> next Planning.

### Impediment Handling

1. **Log Immediately**: When blocked, add to `impediments.active` right away.
2. **Escalation Path**: Developer -> Scrum Master -> Human.
3. **Resolution**: Move resolved impediments to `impediments.resolved`.

### Definition of Done

1. **All Criteria Must Pass**: Every required DoD criterion must be verified.
2. **Executable Verification**: Run the verification commands, don't just check boxes.
3. **No Partial Done**: An item is either fully Done or still in_progress.

### Status Transitions

`````````
PBI Status (in Product Backlog):
  draft -> refining -> ready

Sprint Status (1 PBI per Sprint):
  in_progress -> done
       |
    blocked

Subtask Status (TDD Cycle with Commits):
  pending ─┬─> red ─────> green ─┬─> refactoring ─┬─> completed
           │   (commit)  (commit) │    (commit)    │
           │                      │       ↓        │
           │                      │   (more refactor commits)
           │                      │       ↓        │
           │                      └───────┴────────┘
           │
           └─> (skip to completed if no test needed, e.g., pure structural)

Each status transition produces a commit:
  pending -> red:        commit(test: ...)
  red -> green:          commit(feat: ... or fix: ...)
  green -> refactoring:  commit(refactor: ...)
  refactoring -> refactoring: commit(refactor: ...) [multiple allowed]
  refactoring -> completed:   (no commit, just status update)
  green -> completed:    (no commit, skip refactor if not needed)

Sprint Cycle:
  Planning -> Execution -> Review -> Retro -> (next Planning)
`````````

### Agent Responsibilities

| Agent | Reads | Writes |
|-------|-------|--------|
| Product Owner | Full dashboard | Product Backlog, Product Goal, Sprint acceptance |
| Scrum Master | Full dashboard | Sprint config, Impediments, Retrospective, Metrics |
| Developer | Sprint Backlog, DoD | Subtask status, Progress, Notes, Impediments |
| Event Agents | Relevant sections | Event-specific outputs |

---

## Quick Status

`````````yaml
sprint:
  number: 2
  pbi: PBI-002
  status: in_progress
  subtasks_completed: 6
  subtasks_total: 8
  impediments: 0
`````````

---

## 1. Product Backlog

### Product Goal

`````````yaml
product_goal:
  statement: "A fast and flexible Language Server Protocol (LSP) server that leverages Tree-sitter for accurate parsing and language-aware features across multiple programming languages."
  success_metrics:
    - metric: "E2E tests pass"
      target: "make test_nvim succeeds"
    - metric: "Unit tests pass"
      target: "make test succeeds"
    - metric: "Code quality"
      target: "make check succeeds (cargo check, clippy, fmt)"
  owner: "@scrum-team-product-owner"
`````````

### Backlog Items

`````````yaml
product_backlog:
  - id: PBI-002
    title: "Unify semantic token handlers to remove LSP layer injection awareness"
    status: ready
    story:
      role: "maintainer of treesitter-ls"
      capability: "use a single semantic tokens handler that works with or without injections"
      benefit: "simpler code with better separation of concerns and reduced conditional complexity"
    acceptance_criteria:
      - criterion: "Unified handler accepts optional coordinator and parser_pool parameters"
        verification: "Code inspection: handle_semantic_tokens_full signature accepts Option<&LanguageCoordinator> and Option<&mut DocumentParserPool>"
      - criterion: "semantic_tokens_full in LSP layer calls only one handler (no if/else branching)"
        verification: "grep -c 'if let Some(inj_query)' src/lsp/lsp_impl.rs returns 0 for semantic_tokens_full method"
      - criterion: "semantic_tokens_range in LSP layer calls only one handler (no if/else branching)"
        verification: "grep -c 'if let Some(inj_query)' src/lsp/lsp_impl.rs returns 0 for semantic_tokens_range method"
      - criterion: "When coordinator/parser_pool are None, function returns same tokens as current non-injection handler"
        verification: "cargo test test_semantic_tokens_with_japanese passes (existing non-injection test)"
      - criterion: "When coordinator/parser_pool are Some, function returns tokens including injected content"
        verification: "cargo test test_injection_semantic_tokens_basic passes"
      - criterion: "All existing semantic token tests pass"
        verification: "cargo test semantic passes"
      - criterion: "Old non-injection handler functions are removed"
        verification: "grep -c 'pub fn handle_semantic_tokens_full[^_]' src/analysis/semantic.rs returns 0"
      - criterion: "Old injection-specific handler functions are removed"
        verification: "grep -c 'handle_semantic_tokens_full_with_injection' src/analysis/semantic.rs returns 0"
    technical_notes: |
      ## Refactoring Strategy

      ### Current State (Design Smell)

      The design smell exists in THREE places in src/lsp/lsp_impl.rs:

      **1. semantic_tokens_full (lines 522-555):**
      ```rust
      let injection_query = self.language.get_injection_query(&language_name);
      if let Some(inj_query) = injection_query {
          handle_semantic_tokens_full_with_injection(...)
      } else {
          handle_semantic_tokens_full(...)
      }
      ```

      **2. semantic_tokens_range (lines 735-770):**
      ```rust
      let injection_query = self.language.get_injection_query(&language_name);
      let result = if let Some(inj_query) = injection_query {
          handle_semantic_tokens_range_with_injection(...)
      } else {
          handle_semantic_tokens_range(...)
      }
      ```

      **3. semantic_tokens_full_delta (line 648-656):**
      Currently ONLY calls non-injection handler - this is a BUG (see PBI-003).

      ### Four Handlers to Unify (in src/analysis/semantic.rs)

      1. `handle_semantic_tokens_full` (lines 157-236) - non-injection
      2. `handle_semantic_tokens_full_with_injection` (lines 430-509) - with injection
      3. `handle_semantic_tokens_range` (lines 526-621) - non-injection
      4. `handle_semantic_tokens_range_with_injection` (lines 646-745) - with injection

      **Note:** The `_injection_query` parameter in `handle_semantic_tokens_full_with_injection`
      (line 436) is unused - the function gets injection query from coordinator internally.
      This makes unification easier since we don't need to pass injection_query separately.

      ### Target State

      Two unified handlers that work with or without injection support:

      ```rust
      // Unified full handler
      pub fn handle_semantic_tokens_full(
          text: &str,
          tree: &Tree,
          query: &Query,
          filetype: Option<&str>,
          capture_mappings: Option<&CaptureMappings>,
          coordinator: Option<&LanguageCoordinator>,  // NEW: None = no injection
          parser_pool: Option<&mut DocumentParserPool>,  // NEW: None = no injection
      ) -> Option<SemanticTokensResult>

      // Unified range handler
      pub fn handle_semantic_tokens_range(
          text: &str,
          tree: &Tree,
          query: &Query,
          range: &Range,
          filetype: Option<&str>,
          capture_mappings: Option<&CaptureMappings>,
          coordinator: Option<&LanguageCoordinator>,  // NEW: None = no injection
          parser_pool: Option<&mut DocumentParserPool>,  // NEW: None = no injection
      ) -> Option<SemanticTokensResult>
      ```

      **LSP layer becomes simple:**
      ```rust
      // semantic_tokens_full - no branching!
      let mut pool = self.parser_pool.lock()...;
      handle_semantic_tokens_full(
          text,
          tree,
          &query,
          Some(&language_name),
          Some(&capture_mappings),
          Some(&self.language),  // Always pass coordinator
          Some(&mut pool),       // Always pass pool
      )
      ```

      ### Implementation Steps

      1. Modify `handle_semantic_tokens_full_with_injection` to accept Option<&LanguageCoordinator>
         and Option<&mut DocumentParserPool>, handling None case gracefully
      2. Rename it to `handle_semantic_tokens_full` (replacing the old function)
      3. Do the same for range handlers
      4. Update lsp_impl.rs to remove conditional branching in all three methods
      5. Remove the old handler functions
      6. Update all tests to use unified functions
      7. Update re-exports in src/analysis/mod.rs

      ### Key Files
      - src/analysis/semantic.rs: Main implementation (4 handlers to unify into 2)
      - src/lsp/lsp_impl.rs: LSP handlers (3 places with branching)
      - src/analysis/mod.rs: Re-exports

      ### Risk Mitigation
      - Keep existing tests passing throughout refactoring
      - Refactor in small steps with tests after each
      - The injection handler already works without injections (returns host-only tokens)

    dependencies: []
    estimated_subtasks: 7
    origin: "Sprint 1 Retrospective AI-005"

  - id: PBI-003
    title: "Add injection support to semantic_tokens_full_delta"
    status: ready
    story:
      role: "user editing a file with embedded languages (e.g., Markdown with Lua)"
      capability: "see syntax highlighting for injected code blocks update correctly when using delta requests"
      benefit: "consistent syntax highlighting experience when the editor uses delta mode for performance"
    acceptance_criteria:
      - criterion: "semantic_tokens_full_delta uses injection-aware handler when coordinator available"
        verification: "Code inspection: handle_semantic_tokens_full_delta calls injection-aware logic"
      - criterion: "Delta requests return tokens for injected content"
        verification: "Manual test: Edit a Markdown file with Lua code block, verify Lua tokens appear in delta response"
      - criterion: "All existing semantic token tests pass"
        verification: "cargo test semantic passes"
      - criterion: "E2E tests pass"
        verification: "make test_nvim passes"
    technical_notes: |
      ## Bug Description

      In src/lsp/lsp_impl.rs, the `semantic_tokens_full_delta` method (lines 595-692) calls
      `handle_semantic_tokens_full_delta` which internally uses the NON-injection handler
      (line 774 in semantic.rs):

      ```rust
      let current_result =
          handle_semantic_tokens_full(text, tree, query, filetype, capture_mappings)?;
      ```

      This means when an editor requests delta tokens (for performance), injected language
      tokens are LOST. This is a regression from the injection feature in Sprint 1.

      ## Solution Options

      **Option A (Quick Fix):** Modify `handle_semantic_tokens_full_delta` to accept
      coordinator/parser_pool and use injection-aware handler.

      **Option B (After PBI-002):** Once handlers are unified, this bug is automatically
      fixed because there's only one handler.

      ## Recommendation

      Complete PBI-002 first, which will fix this bug as a side effect. Then verify
      delta works correctly with injections.

      ## Key Files
      - src/analysis/semantic.rs: handle_semantic_tokens_full_delta (lines 763-790)
      - src/lsp/lsp_impl.rs: semantic_tokens_full_delta method (lines 595-692)

    dependencies:
      - PBI-002  # Fixing PBI-002 will automatically fix this
    estimated_subtasks: 2
    origin: "Backlog Refinement - code review of semantic.rs"
`````````

### Definition of Ready

`````````yaml
definition_of_ready:
  criteria:
    - criterion: "AI can complete this story without human input"
      required: true
      note: "If human input needed, split or keep as refining"
    - criterion: "User story has role, capability, and benefit"
      required: true
    - criterion: "At least 3 acceptance criteria with verification commands"
      required: true
    - criterion: "Dependencies are resolved or not blocking"
      required: true
`````````

---

## 2. Current Sprint

`````````yaml
sprint:
  number: 2
  pbi_id: PBI-002
  story:
    role: "maintainer of treesitter-ls"
    capability: "use a single semantic tokens handler that works with or without injections"
    benefit: "simpler code with better separation of concerns and reduced conditional complexity"
  status: in_progress

  subtasks:
    # Sprint 2: Unify semantic token handlers
    # This is primarily a STRUCTURAL refactoring PBI following Tidy First principles.
    # Strategy: Expand-Contract pattern
    #   1. Expand: Add new unified interface (coordinator/parser_pool as Option)
    #   2. Migrate: Update callers to use unified interface
    #   3. Contract: Remove old interfaces
    #
    # Subtask order ensures tests pass at every step.

    # --- Phase 1: Unify handle_semantic_tokens_full ---

    - test: "handle_semantic_tokens_full_with_injection works when coordinator is None (returns host-only tokens)"
      implementation: "Modify handle_semantic_tokens_full_with_injection to accept Option<&LanguageCoordinator> and Option<&mut DocumentParserPool>, returning host-only tokens when None"
      type: behavioral
      status: completed
      commits:
        - hash: deee8c9
          message: "feat(semantic): make handle_semantic_tokens_full_with_injection accept Option parameters"
          phase: green

    - test: "Existing non-injection semantic token tests still pass after renaming"
      implementation: "Rename handle_semantic_tokens_full_with_injection to handle_semantic_tokens_full (replacing old function)"
      type: structural
      status: completed
      commits:
        - hash: 216cf06
          message: "refactor(semantic): rename handle_semantic_tokens_full_with_injection to handle_semantic_tokens_full"
          phase: refactor

    - test: "LSP semantic_tokens_full calls unified handler without conditional branching"
      implementation: "Update lsp_impl.rs semantic_tokens_full to always call unified handle_semantic_tokens_full with Some(coordinator) and Some(pool)"
      type: structural
      status: completed
      commits:
        - hash: deee8c9
          message: "feat(semantic): make handle_semantic_tokens_full_with_injection accept Option parameters"
          phase: green
          note: "LSP layer updated as part of Subtask 1 to use unified handler"

    # --- Phase 2: Unify handle_semantic_tokens_range ---

    - test: "handle_semantic_tokens_range_with_injection works when coordinator is None (returns host-only tokens)"
      implementation: "Modify handle_semantic_tokens_range_with_injection to accept Option<&LanguageCoordinator> and Option<&mut DocumentParserPool>, returning host-only tokens when None"
      type: behavioral
      status: completed
      commits:
        - hash: 7ff8e7d
          message: "feat(semantic): make handle_semantic_tokens_range_with_injection accept Option parameters"
          phase: green

    - test: "Existing non-injection range semantic token tests still pass after renaming"
      implementation: "Rename handle_semantic_tokens_range_with_injection to handle_semantic_tokens_range (replacing old function)"
      type: structural
      status: completed
      commits:
        - hash: df08489
          message: "refactor(semantic): rename handle_semantic_tokens_range_with_injection to handle_semantic_tokens_range"
          phase: refactor

    - test: "LSP semantic_tokens_range calls unified handler without conditional branching"
      implementation: "Update lsp_impl.rs semantic_tokens_range to always call unified handle_semantic_tokens_range with Some(coordinator) and Some(pool)"
      type: structural
      status: completed
      commits:
        - hash: 7ff8e7d
          message: "feat(semantic): make handle_semantic_tokens_range_with_injection accept Option parameters"
          phase: green
          note: "LSP layer updated as part of Subtask 4 to use unified handler"

    # --- Phase 3: Update delta and cleanup ---

    - test: "handle_semantic_tokens_full_delta uses unified handler internally"
      implementation: "Update handle_semantic_tokens_full_delta to call unified handle_semantic_tokens_full (fixes PBI-003 bug)"
      type: behavioral
      status: pending
      commits: []

    - test: "All acceptance criteria verified: no old handler functions remain"
      implementation: "Remove old non-injection handler function bodies (now dead code), update mod.rs re-exports"
      type: structural
      status: pending
      commits: []

  notes: |
    Sprint 2 started via Sprint Planning.
    Sprint Goal: Unify semantic token handlers to remove LSP layer injection awareness.

    Refactoring Strategy (Expand-Contract):
    - Phase 1: Unify full handlers (subtasks 1-3)
    - Phase 2: Unify range handlers (subtasks 4-6)
    - Phase 3: Fix delta handler and cleanup (subtasks 7-8)

    Key constraint: All tests must pass after each subtask completion.
    This ensures we can safely refactor without breaking existing functionality.

    Note: Subtask 7 will automatically fix the bug identified in PBI-003.
`````````

### Impediment Registry

`````````yaml
impediments:
  active: []
  # Example impediment format:
  # - id: IMP-001
  #   reporter: "@scrum-team-developer"
  #   description: "Redis connection timeout in test environment"
  #   impact: "Blocks rate limiting tests"
  #   severity: high  # low | medium | high | critical
  #   affected_items:
  #     - PBI-003
  #   resolution_attempts:
  #     - attempt: "Increased connection timeout to 30s"
  #       result: "Still failing"
  #   status: investigating  # new | investigating | escalated | resolved
  #   escalated_to: null
  #   resolution: null

  resolved: []
  # Example resolved impediment format:
  # - id: IMP-000
  #   reporter: "@scrum-team-developer"
  #   description: "Missing pytest-asyncio dependency"
  #   impact: "Async tests could not run"
  #   severity: medium
  #   resolution: "Added pytest-asyncio to dev dependencies"
`````````

---

## 3. Definition of Done

`````````yaml
definition_of_done:
  # Run all verification commands from the PBI's acceptance_criteria
  # Plus these baseline checks:
  checks:
    - name: "All unit tests pass"
      run: "make test"
    - name: "Code quality checks pass (check, clippy, fmt)"
      run: "make check"
    - name: "E2E tests pass"
      run: "make test_nvim"
      note: "Runs Neovim integration tests including tests/test_lsp_semantic.lua"
`````````

---

## 4. Completed Sprints

`````````yaml
# Log of completed PBIs (one per sprint)
completed:
  - sprint: 1
    pbi: PBI-001
    story: "As a software engineer using language servers, I want to read syntax highlighted code including injected languages, so that I have improved code readability when viewing files with embedded languages"
    outcome: "Delivered semantic tokens for injected languages with recursive depth support"
    acceptance:
      status: accepted
      criteria_verified:
        - "Semantic tokens include tokens from injected Lua code in Markdown fenced code blocks"
        - "Semantic tokens for injected content have correct UTF-16 positions relative to host document"
        - "Nested injections are supported (e.g., Lua in Markdown in Markdown)"
        - "Indented injections have correct column positions"
        - "All existing semantic token tests continue to pass"
        - "Code quality checks pass"
      dod_verified:
        - "make test: PASSED"
        - "make check: PASSED"
        - "make test_nvim: PASSED"
    subtasks_completed: 8
    unit_tests: 138
    e2e_tests: 20
    impediments: 0
`````````

---

## 5. Retrospective Log

`````````yaml
# After each sprint, record what to improve
retrospectives:
  - sprint: 1
    pbi: PBI-001
    prime_directive_read: true

    what_went_well:
      - item: "TDD Approach Worked"
        detail: "Writing failing tests first led to clean, focused implementations"
      - item: "Existing Pattern Reuse"
        detail: "Following the established selection/range_builder.rs pattern for injection handling accelerated development"
      - item: "Clear Acceptance Criteria"
        detail: "The well-defined verification commands made it easy to know when we were done"
      - item: "Modular Design"
        detail: "The separation of concerns (position adjustment, token merging, recursive collection) made the code testable and maintainable"
      - item: "Zero Impediments"
        detail: "Sprint completed without any blockers"

    what_could_improve:
      - item: "Commit Discipline Violation (Critical)"
        detail: |
          The Sprint had 8 subtasks (ST-001 through ST-008) but resulted in only ONE commit
          for all implementation work (214f139 feat(semantic): add semantic tokens for injected languages).
          This violates TDD principles where:
          - Each GREEN phase should result in a commit
          - Each REFACTOR step should result in a commit
          - With 8 subtasks, there should have been 8-24 commits minimum
        root_cause: |
          The @scrum-team-developer agent processed all subtasks in a single session without
          invoking git commit after each GREEN phase. The agent prompt says to use TDD commands
          but doesn't enforce committing after each cycle.
        impact: "high"

      - item: "Design Smell: LSP Layer Knows About Injections"
        detail: |
          The semantic_tokens_full method in src/lsp/lsp_impl.rs (lines 522-555) has branching logic:
          ```rust
          if injection_query.is_some() {
              handle_semantic_tokens_full_with_injection(...)
          } else {
              handle_semantic_tokens_full(...)
          }
          ```
          This violates separation of concerns:
          - The LSP layer shouldn't need to know whether injections exist
          - There should be ONE unified handler that handles both cases transparently
          - The injection-aware handler should gracefully degrade when no injections are present
        root_cause: |
          Implementation focused on getting the feature working without considering the
          architectural principle that callers shouldn't need to know about implementation details.
        impact: "medium"

      - item: "Technical Notes Quality"
        detail: "The detailed technical notes in PBI-001 were excellent - future PBIs should aim for the same level of detail"

      - item: "Subtask Granularity"
        detail: "Some subtasks could have been combined (e.g., ST-003 and ST-004 were closely related)"

      - item: "Test File Organization"
        detail: "The test file tests/assets/example.md serves multiple test purposes; consider whether dedicated test fixtures would be clearer"

    action_items:
      # Critical Process Improvement - Commit Discipline
      - id: AI-004
        action: "Add commit enforcement to @scrum-team-developer agent prompt"
        detail: |
          Update the developer agent to REQUIRE committing after each subtask completion.
          The agent should:
          1. Mark subtask as in_progress
          2. Write failing test (RED)
          3. Implement minimum code to pass (GREEN)
          4. Run `git add . && git commit -m "test(scope): <subtask description>"` or
             `git add . && git commit -m "feat(scope): <subtask description>"`
          5. Refactor if needed, then commit refactoring separately
          6. Mark subtask as completed
        owner: "@scrum-team-scrum-master"
        status: completed
        resolution: |
          Created .claude/agents/scrum-team-developer.md with explicit TDD phase commit requirements.
          Updated CLAUDE.md with "The Iron Law" commit discipline section.
          Both files now require commits after RED, GREEN, and REFACTOR phases.
        backlog: sprint  # This is a process improvement, goes in Sprint Backlog

      # Design Refactoring - Create new PBI
      - id: AI-005
        action: "Create PBI for unifying semantic token handlers"
        detail: |
          Refactor handle_semantic_tokens_full_with_injection to be the ONLY handler that
          works with or without injections. The function should:
          1. Accept optional injection query parameter
          2. When injection query is None, collect tokens only from the host document
          3. When injection query is Some, collect tokens from host + injections
          4. The LSP layer should always call the unified handler
          This removes the conditional branching in lsp_impl.rs.
        owner: "@scrum-team-product-owner"
        status: completed
        resolution: |
          Created PBI-002 in the Product Backlog with full acceptance criteria,
          technical notes, and verification commands. Status: ready for Sprint 2.
        backlog: product  # This is a technical debt item, goes in Product Backlog

      # Original action items
      - id: AI-001
        action: "Ensure technical notes include implementation pattern and key files to modify"
        owner: "@scrum-team-product-owner"
        status: pending
        backlog: sprint

      - id: AI-002
        action: "Review subtask breakdown before sprint start to combine closely related items"
        owner: "@scrum-team-developer"
        status: pending
        backlog: sprint

      - id: AI-003
        action: "Consider dedicated test fixtures for complex scenarios"
        owner: "@scrum-team-developer"
        status: pending
        backlog: sprint

    insights:
      - insight: "Process adherence requires enforcement, not just documentation"
        analysis: |
          The TDD cycle was documented in CLAUDE.md but not enforced. AI agents, like humans,
          may skip steps under time pressure or when focused on delivering features. The fix
          is to make the commit step part of the explicit workflow that gets checked.

      - insight: "Separation of concerns applies to conditional logic too"
        analysis: |
          Having two separate handlers (with and without injection) forces the caller to
          make a decision it shouldn't need to make. The principle of "make illegal states
          unrepresentable" extends to "make unnecessary decisions unnecessary."

    metrics:
      unit_tests_added: 3
      subtasks_completed: 8
      impediments_encountered: 0
      dod_criteria_met: 3
      commits_expected: 8  # minimum: 1 per subtask
      commits_actual: 1    # actual commits for implementation
      commit_discipline_score: 0.125  # 1/8 = 12.5%
`````````

---

## 6. Agents

`````````yaml
agents:
  product_owner: "@scrum-team-product-owner"
  scrum_master: "@scrum-team-scrum-master"
  developer: "@scrum-team-developer"

events:
  planning: "@scrum-event-sprint-planning"
  review: "@scrum-event-sprint-review"
  retrospective: "@scrum-event-sprint-retrospective"
  refinement: "@scrum-event-backlog-refinement"
`````````
