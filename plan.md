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

### Sprint Execution

1. **One PBI per Sprint**: Select the top `ready` item. That's the Sprint Backlog.
2. **Subtask Breakdown**: Break the PBI into subtasks at Sprint start.
3. **Update on Completion**: Mark subtasks `completed` immediately when done.
4. **Full Event Cycle**: After PBI completion, run Review -> Retro -> next Planning.

### Impediment Handling

1. **Log Immediately**: When blocked, add to `impediments.active` right away.
2. **Escalation Path**: Developer -> Scrum Master -> Human.
3. **Resolution**: Move resolved impediments to `impediments.resolved`.

### Definition of Done

1. **All Criteria Must Pass**: Every required DoD criterion must be verified.
2. **Executable Verification**: Run the verification commands, don't just check boxes.
3. **No Partial Done**: An item is either fully Done or still in_progress.

### Status Transitions

```
PBI Status (in Product Backlog):
  draft -> refining -> ready

Sprint Status (1 PBI per Sprint):
  in_progress -> done
       |
    blocked

Sprint Cycle:
  Planning -> Execution -> Review -> Retro -> (next Planning)
```

### Agent Responsibilities

| Agent | Reads | Writes |
|-------|-------|--------|
| Product Owner | Full dashboard | Product Backlog, Product Goal, Sprint acceptance |
| Scrum Master | Full dashboard | Sprint config, Impediments, Retrospective, Metrics |
| Developer | Sprint Backlog, DoD | Subtask status, Progress, Notes, Impediments |
| Event Agents | Relevant sections | Event-specific outputs |

---

## Quick Status

```yaml
sprint:
  number: 1
  pbi: PBI-001
  status: accepted
  subtasks_completed: 8
  subtasks_total: 8
  impediments: 0
```

---

## 1. Product Backlog

### Product Goal

```yaml
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
```

### Backlog Items

```yaml
product_backlog: []
  # PBI-001 has been accepted and moved to Completed Sprints
```

### Definition of Ready

```yaml
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
```

---

## 2. Current Sprint

```yaml
sprint:
  number: 1
  pbi_id: PBI-001
  story:
    role: "software engineer using language servers"
    capability: "read syntax highlighted code including injected languages"
    benefit: "improved code readability when viewing files with embedded languages (e.g., Lua in Markdown)"
  status: accepted

  subtasks:
    - id: ST-001
      description: "Add unit test for basic injection semantic tokens (Lua in Markdown line 7)"
      status: completed
      acceptance_criterion: "Semantic tokens for injected content have correct UTF-16 positions relative to host document"

    - id: ST-002
      description: "Implement handle_semantic_tokens_full_with_injection function in src/analysis/semantic.rs"
      status: completed
      acceptance_criterion: "Semantic tokens include tokens from injected Lua code in Markdown fenced code blocks"

    - id: ST-003
      description: "Add helper to adjust token positions for injection offset"
      status: completed
      acceptance_criterion: "Semantic tokens for injected content have correct UTF-16 positions relative to host document"

    - id: ST-004
      description: "Add helper to merge token lists while maintaining sorted order"
      status: completed
      acceptance_criterion: "Semantic tokens include tokens from injected Lua code in Markdown fenced code blocks"

    - id: ST-005
      description: "Modify lsp_impl.rs to use injection-aware handler for both full and range requests"
      status: completed
      acceptance_criterion: "Semantic tokens include tokens from injected Lua code in Markdown fenced code blocks"

    - id: ST-006
      description: "Add unit test for nested injections (Lua in Markdown in Markdown)"
      status: completed
      acceptance_criterion: "Nested injections are supported (e.g., Lua in Markdown in Markdown)"

    - id: ST-007
      description: "Add unit test for indented injections (Lua in list item)"
      status: completed
      acceptance_criterion: "Indented injections have correct column positions"

    - id: ST-008
      description: "Update tests/test_lsp_semantic.lua line 64 to expect keyword token"
      status: completed
      acceptance_criterion: "Semantic tokens include tokens from injected Lua code in Markdown fenced code blocks"

  notes: |
    Sprint 1 started via Sprint Planning.
    Sprint Goal: Deliver semantic tokens for injected languages.
    Following TDD approach: write failing test first, then implement minimum code to pass.
```

### Impediment Registry

```yaml
impediments:
  active: []
  resolved: []
```

---

## 3. Definition of Done

```yaml
definition_of_done:
  checks:
    - check: "All unit tests pass"
      command: "make test"
      required: true
    - check: "Code quality checks pass (check, clippy, fmt)"
      command: "make check"
      required: true
    - check: "E2E tests pass"
      command: "make test_nvim"
      required: true
      note: "Runs Neovim integration tests including tests/test_lsp_semantic.lua"
```

---

## 4. Completed Sprints

```yaml
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
```

---

## 5. Retrospective Log

```yaml
retrospectives:
  - sprint: 1
    pbi: PBI-001

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
      - item: "Technical Notes Quality"
        detail: "The detailed technical notes in PBI-001 were excellent - future PBIs should aim for the same level of detail"
      - item: "Subtask Granularity"
        detail: "Some subtasks could have been combined (e.g., ST-003 and ST-004 were closely related)"
      - item: "Test File Organization"
        detail: "The test file tests/assets/example.md serves multiple test purposes; consider whether dedicated test fixtures would be clearer"

    action_items:
      - id: AI-001
        action: "Ensure technical notes include implementation pattern and key files to modify"
        owner: "@scrum-team-product-owner"
        status: pending
      - id: AI-002
        action: "Review subtask breakdown before sprint start to combine closely related items"
        owner: "@scrum-team-developer"
        status: pending
      - id: AI-003
        action: "Consider dedicated test fixtures for complex scenarios"
        owner: "@scrum-team-developer"
        status: pending

    metrics:
      unit_tests_added: 3
      subtasks_completed: 8
      impediments_encountered: 0
      dod_criteria_met: 3
```

---

## 6. Agents

```yaml
agents:
  product_owner: "@scrum-team-product-owner"
  scrum_master: "@scrum-team-scrum-master"
  developer: "@scrum-team-developer"

events:
  planning: "@scrum-event-sprint-planning"
  review: "@scrum-event-sprint-review"
  retrospective: "@scrum-event-sprint-retrospective"
  refinement: "@scrum-event-backlog-refinement"
```
