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
  status: done
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
product_backlog:
  - id: PBI-001
    story:
      role: "software engineer using language servers"
      capability: "read syntax highlighted code including injected languages"
      benefit: "improved code readability when viewing files with embedded languages (e.g., Lua in Markdown)"
    acceptance_criteria:
      - criterion: "Semantic tokens include tokens from injected Lua code in Markdown fenced code blocks"
        verification: |
          Update tests/test_lsp_semantic.lua line 64:
          Change `{ 6, 1, {} }` to `{ 6, 1, { { type = "keyword" } } }`
          (line 6 col 1 in example.md is `local` keyword in ```lua block)
          Run: make test_nvim
      - criterion: "Semantic tokens for injected content have correct UTF-16 positions relative to host document"
        verification: |
          Add unit test in src/analysis/semantic.rs:
          Test that tokens in injected Lua (line 7: "local xyz = 12345")
          have delta_line accounting for the fenced code block position.
          Run: cargo test semantic
      - criterion: "Nested injections are supported (e.g., Lua in Markdown in Markdown)"
        verification: |
          Add test case for example.md lines 12-16 (nested markdown with lua block).
          Run: cargo test semantic
      - criterion: "Indented injections have correct column positions"
        verification: |
          Add test case for example.md lines 22-24 (indented lua block in list item).
          The `local` keyword should have column 4 (indented by 4 spaces).
          Run: cargo test semantic
      - criterion: "All existing semantic token tests continue to pass"
        verification: "cargo test semantic && cargo test test_lsp_semantic"
      - criterion: "Code quality checks pass"
        verification: "make check"
    dependencies: []
    status: ready
    technical_notes: |
      ## Implementation Pattern

      Follow the pattern established in `src/analysis/selection/range_builder.rs`:

      1. Create new function `handle_semantic_tokens_full_with_injection` in `src/analysis/semantic.rs`
      2. Use `InjectionContext` and `DocumentContext` from `src/analysis/selection/context.rs`
      3. Use `injection::detect_injection_with_content` to find injection regions
      4. For each injection region:
         - Parse injected content using the coordinator and parser pool
         - Get highlight query for injected language
         - Generate tokens for injected content
         - Adjust token positions to host document coordinates
         - Merge with host document tokens

      ## Key Files to Modify

      1. `src/analysis/semantic.rs`:
         - Add `handle_semantic_tokens_full_with_injection` function
         - Add helper to merge token lists while maintaining sorted order
         - Add helper to adjust token positions for injection offset

      2. `src/lsp/lsp_impl.rs`:
         - Modify `semantic_tokens_full` to use injection-aware handler
         - Pass coordinator and parser_pool to the handler

      3. `tests/test_lsp_semantic.lua`:
         - Update line 64 to expect `{ type = "keyword" }` instead of `{}`

      ## Position Calculation

      For injected content at host document offset `content_start_byte`:
      - Token positions from injected parse tree are relative to content start
      - Add `content_start_byte` to each token's byte offset
      - Use PositionMapper to convert to UTF-16 LSP positions
      - Handle offset directives from injection queries (see `parse_offset_directive_for_pattern`)

      ## Test File: tests/assets/example.md

      ```markdown
      ---
      title: "awesome"
      array: ["xxxx"]
      ---

      ```lua
      local xyz = 12345     <- Line 7 (0-indexed: 6), col 0: `local` is keyword
      ```

      # nested injection

      `````markdown
      ```lua
      local injection = true
      ```
      `````

      # indented injection

      * item

          ```lua
          local indent = true   <- `local` at col 4
          ```
      ```

      ## Recursion Depth

      Use MAX_INJECTION_DEPTH (10) from context.rs to prevent stack overflow.

      ## Token Merging Strategy

      1. Collect host tokens (excluding injection regions)
      2. Collect injected tokens (with position adjustment)
      3. Merge by position (line, then column)
      4. Convert to relative delta format for LSP response
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
  status: in_progress

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
completed: []
```

---

## 5. Retrospective Log

```yaml
retrospectives: []
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
