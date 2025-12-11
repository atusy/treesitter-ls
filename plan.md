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
  number: 1
  pbi: PBI-001
  status: accepted
  subtasks_completed: 8
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
      - criterion: "handle_semantic_tokens_full accepts optional injection_query, coordinator, and parser_pool parameters"
        verification: "Code inspection: function signature accepts Option types for injection-related params"
      - criterion: "LSP layer calls only one handler (no branching based on injection query existence)"
        verification: "grep -n 'handle_semantic_tokens_full' src/lsp/lsp_impl.rs shows single call pattern"
      - criterion: "When injection_query is None, function returns same tokens as current non-injection handler"
        verification: "cargo test test_semantic_tokens_with_japanese passes (existing non-injection test)"
      - criterion: "When injection_query is Some, function returns tokens including injected content"
        verification: "cargo test test_injection_semantic_tokens_basic passes"
      - criterion: "All existing semantic token tests pass"
        verification: "cargo test semantic passes"
      - criterion: "handle_semantic_tokens_full (non-injection) function is removed or deprecated"
        verification: "grep -c 'handle_semantic_tokens_full[^_]' src/analysis/semantic.rs returns 0 or shows #[deprecated]"
    technical_notes: |
      ## Refactoring Strategy

      ### Current State (Design Smell)
      src/lsp/lsp_impl.rs lines 522-555:
      ```rust
      let injection_query = self.language.get_injection_query(&language_name);
      if let Some(inj_query) = injection_query {
          handle_semantic_tokens_full_with_injection(...)
      } else {
          handle_semantic_tokens_full(...)
      }
      ```

      ### Target State
      The LSP layer should simply call:
      ```rust
      let injection_query = self.language.get_injection_query(&language_name);
      handle_semantic_tokens(
          text,
          tree,
          &query,
          Some(&language_name),
          Some(&capture_mappings),
          injection_query.as_ref(),  // Option<&Query>
          Some(&self.language),       // Option<&LanguageCoordinator>
          Some(&mut pool),            // Option<&mut DocumentParserPool>
      )
      ```

      ### Implementation Steps
      1. Modify handle_semantic_tokens_full_with_injection to handle None injection_query gracefully
      2. Rename it to handle_semantic_tokens_full (replacing the old function)
      3. Update lsp_impl.rs to remove the conditional branching
      4. Update all tests to use the unified function
      5. Remove the old handle_semantic_tokens_full function

      ### Key Files
      - src/analysis/semantic.rs: Main implementation
      - src/lsp/lsp_impl.rs: LSP handler (lines 484-556, 600-680 for range)
      - src/analysis/mod.rs: Re-exports

      ### Alternative Considered
      Instead of optional params, could use a builder pattern or a SemanticTokensRequest struct.
      However, optional params are simpler and match the existing pattern in the codebase.
    dependencies: []
    estimated_subtasks: 5
    origin: "Sprint 1 Retrospective AI-005"
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
  number: 1
  pbi_id: PBI-001
  story:
    role: "software engineer using language servers"
    capability: "read syntax highlighted code including injected languages"
    benefit: "improved code readability when viewing files with embedded languages (e.g., Lua in Markdown)"
  status: accepted

  subtasks:
    # TDD Subtask Format - Each subtask tracks commits through Red-Green-Refactor:
    #
    # - test: "User model has email and hashed_password fields"
    #   implementation: "Create User SQLAlchemy model with fields"
    #   type: behavioral  # behavioral | structural
    #   status: completed  # pending | red | green | refactoring | completed
    #   commits:
    #     - phase: red
    #       message: "test: User model has email and hashed_password fields"
    #     - phase: green
    #       message: "feat: Create User SQLAlchemy model"
    #     - phase: refactor
    #       message: "refactor: Extract field definitions to constants"
    #     - phase: refactor
    #       message: "refactor: Add docstring to User model"
    #
    # - test: "hash_password returns bcrypt hash"
    #   implementation: "Implement hash_password utility function"
    #   type: behavioral
    #   status: green  # Test passing, no refactor needed yet
    #   commits:
    #     - phase: red
    #       message: "test: hash_password returns bcrypt hash"
    #     - phase: green
    #       message: "feat: Implement hash_password utility"
    #
    # - test: "verify_password returns True for matching passwords"
    #   implementation: "Implement verify_password utility function"
    #   type: behavioral
    #   status: red  # Failing test committed, implementation pending
    #   commits:
    #     - phase: red
    #       message: "test: verify_password returns True for matching"
    #
    # - test: "Extract password validation to separate module"
    #   implementation: "Move validation logic to validators.py"
    #   type: structural  # Refactoring - no new behavior, no red phase
    #   status: pending
    #   commits: []
    #
    # Status meanings:
    #   pending    -> Not started, no commits yet
    #   red        -> Failing test committed, ready for implementation
    #   green      -> Passing implementation committed, ready for refactoring
    #   refactoring -> One or more refactor commits done, more may come
    #   completed  -> All commits done, subtask finished
    #
    # Commit tracking:
    #   - Each TDD phase produces exactly one commit (except refactoring which may have many)
    #   - phase: red | green | refactor
    #   - message: The actual commit message used
    #   - Multiple refactor commits are encouraged (Tidy First = small structural changes)

    - test: "Semantic tokens for injected content have correct UTF-16 positions relative to host document"
      implementation: "Add unit test for basic injection semantic tokens (Lua in Markdown line 7)"
      type: behavioral
      status: completed
      commits: []

    - test: "Semantic tokens include tokens from injected Lua code in Markdown fenced code blocks"
      implementation: "Implement handle_semantic_tokens_full_with_injection function in src/analysis/semantic.rs"
      type: behavioral
      status: completed
      commits: []

    - test: "Semantic tokens for injected content have correct UTF-16 positions relative to host document"
      implementation: "Add helper to adjust token positions for injection offset"
      type: behavioral
      status: completed
      commits: []

    - test: "Semantic tokens include tokens from injected Lua code in Markdown fenced code blocks"
      implementation: "Add helper to merge token lists while maintaining sorted order"
      type: behavioral
      status: completed
      commits: []

    - test: "Semantic tokens include tokens from injected Lua code in Markdown fenced code blocks"
      implementation: "Modify lsp_impl.rs to use injection-aware handler for both full and range requests"
      type: behavioral
      status: completed
      commits: []

    - test: "Nested injections are supported (e.g., Lua in Markdown in Markdown)"
      implementation: "Add unit test for nested injections (Lua in Markdown in Markdown)"
      type: behavioral
      status: completed
      commits: []

    - test: "Indented injections have correct column positions"
      implementation: "Add unit test for indented injections (Lua in list item)"
      type: behavioral
      status: completed
      commits: []

    - test: "Semantic tokens include tokens from injected Lua code in Markdown fenced code blocks"
      implementation: "Update tests/test_lsp_semantic.lua line 64 to expect keyword token"
      type: behavioral
      status: completed
      commits: []

  notes: |
    Sprint 1 started via Sprint Planning.
    Sprint Goal: Deliver semantic tokens for injected languages.
    Following TDD approach: write failing test first, then implement minimum code to pass.
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
