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
  number: 4
  pbi: PBI-005
  status: done
  subtasks_completed: 6
  subtasks_total: 6
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
  # PBI-002 completed in Sprint 2
  # PBI-003 resolved as part of Sprint 2 (PBI-002 fix included delta injection support)

  # ============================================================================
  # EPIC: Automatic Parser and Query Installation
  # ============================================================================
  # Goal: Users can install Tree-sitter parsers and queries with a single command,
  #       leveraging nvim-treesitter's metadata without requiring Neovim.
  #
  # Splitting Strategy: By workflow step (Happy Path first, then edge cases)
  #   PBI-004: CLI infrastructure + basic install command (foundation)
  #   PBI-005: Query downloading from nvim-treesitter (quick value)
  #   PBI-006: Parser metadata parsing and compilation (core feature)
  #   PBI-007: Parser dependency resolution (edge case)
  #   PBI-008: Auto-install on file open (enhancement - optional)
  # ============================================================================

  - id: PBI-004
    status: ready
    story:
      role: "user of treesitter-ls"
      capability: "run treesitter-ls with CLI subcommands"
      benefit: "I can manage parsers and queries using the same binary I use for the language server"
    acceptance_criteria:
      - criterion: "Running `treesitter-ls --help` shows available subcommands including `install`"
        verification: |
          cargo build --release
          ./target/release/treesitter-ls --help | grep -q "install"
      - criterion: "Running `treesitter-ls install --help` shows install command usage"
        verification: |
          ./target/release/treesitter-ls install --help | grep -q "LANGUAGE"
      - criterion: "Running `treesitter-ls` with no arguments starts the LSP server (backward compatible)"
        verification: |
          # The server should start and wait for LSP input (will timeout without stdin)
          timeout 1 ./target/release/treesitter-ls 2>&1 || true
      - criterion: "Running `treesitter-ls install lua` without prerequisites prints helpful error message"
        verification: |
          ./target/release/treesitter-ls install lua 2>&1 | grep -qi "error\|not found\|missing"
    story_points: 3
    dependencies: []
    technical_notes: |
      ## Implementation Strategy

      1. Add `clap` crate for CLI argument parsing (de facto standard in Rust)
      2. Modify src/bin/main.rs to:
         - Parse CLI arguments before starting LSP
         - If no subcommand, start LSP server (current behavior)
         - If subcommand, execute it and exit

      ## Key Files to Modify
      - src/bin/main.rs - Add CLI parsing
      - Cargo.toml - Add clap dependency

      ## CLI Structure
      ```
      treesitter-ls                    # Start LSP server
      treesitter-ls install <LANG>     # Install parser and queries
      treesitter-ls install --list     # List available languages
      treesitter-ls --version          # Show version
      ```

      ## Example main.rs Structure
      ```rust
      use clap::{Parser, Subcommand};

      #[derive(Parser)]
      #[command(name = "treesitter-ls")]
      struct Cli {
          #[command(subcommand)]
          command: Option<Commands>,
      }

      #[derive(Subcommand)]
      enum Commands {
          Install { language: String },
      }

      #[tokio::main]
      async fn main() {
          let cli = Cli::parse();
          match cli.command {
              Some(Commands::Install { language }) => {
                  // TODO: Implement in PBI-005/006
                  eprintln!("Install command not yet implemented for: {}", language);
                  std::process::exit(1);
              }
              None => {
                  // Current LSP server logic
              }
          }
      }
      ```

  - id: PBI-005
    status: ready
    story:
      role: "user of treesitter-ls"
      capability: "download Tree-sitter query files (highlights.scm, locals.scm) for a language"
      benefit: "I get syntax highlighting and go-to-definition without manually finding and copying query files"
    acceptance_criteria:
      - criterion: "Running `treesitter-ls install-queries lua` downloads Lua queries to the data directory"
        verification: |
          rm -rf ~/.local/share/treesitter-ls/queries/lua
          ./target/release/treesitter-ls install-queries lua
          test -f ~/.local/share/treesitter-ls/queries/lua/highlights.scm
      - criterion: "Downloaded queries are from nvim-treesitter repository"
        verification: |
          head -5 ~/.local/share/treesitter-ls/queries/lua/highlights.scm | grep -qi "lua\|tree-sitter"
      - criterion: "Running with `--data-dir` flag uses custom directory"
        verification: |
          rm -rf /tmp/treesitter-test/queries
          ./target/release/treesitter-ls install-queries lua --data-dir /tmp/treesitter-test
          test -f /tmp/treesitter-test/queries/lua/highlights.scm
      - criterion: "Attempting to download queries for unsupported language shows helpful error"
        verification: |
          ./target/release/treesitter-ls install-queries nonexistent_lang 2>&1 | grep -qi "not supported\|not found"
      - criterion: "Existing queries can be overwritten with `--force` flag"
        verification: |
          ./target/release/treesitter-ls install-queries lua --force
          test -f ~/.local/share/treesitter-ls/queries/lua/highlights.scm
    story_points: 5
    dependencies:
      - PBI-004  # Requires CLI infrastructure
    technical_notes: |
      ## Implementation Strategy

      1. Add `reqwest` crate for HTTP requests (async HTTP client)
      2. Create src/install/mod.rs module for installation logic
      3. Create src/install/queries.rs for query downloading

      ## Key Files to Create/Modify
      - src/install/mod.rs - New module for installation
      - src/install/queries.rs - Query download logic
      - src/bin/main.rs - Add install-queries subcommand
      - Cargo.toml - Add reqwest, dirs dependencies

      ## Query Sources (nvim-treesitter GitHub raw URLs)
      ```
      https://raw.githubusercontent.com/nvim-treesitter/nvim-treesitter/master/queries/{lang}/highlights.scm
      https://raw.githubusercontent.com/nvim-treesitter/nvim-treesitter/master/queries/{lang}/locals.scm
      https://raw.githubusercontent.com/nvim-treesitter/nvim-treesitter/master/queries/{lang}/injections.scm
      ```

      ## Default Data Directory
      - Linux: ~/.local/share/treesitter-ls/
      - macOS: ~/Library/Application Support/treesitter-ls/
      - Windows: %APPDATA%/treesitter-ls/

      Use `dirs` crate for platform-specific paths.

      ## File Structure After Install
      ```
      ~/.local/share/treesitter-ls/
        queries/
          lua/
            highlights.scm
            locals.scm
            injections.scm (if exists)
          python/
            highlights.scm
            ...
      ```

  - id: PBI-006
    status: ready
    story:
      role: "user of treesitter-ls"
      capability: "compile and install a Tree-sitter parser for a language"
      benefit: "I get a working parser without manually cloning repos and running build commands"
    acceptance_criteria:
      - criterion: "Running `treesitter-ls install-parser lua` downloads and compiles the Lua parser"
        verification: |
          rm -rf ~/.local/share/treesitter-ls/parsers/lua.*
          ./target/release/treesitter-ls install-parser lua
          test -f ~/.local/share/treesitter-ls/parsers/lua.so || test -f ~/.local/share/treesitter-ls/parsers/lua.dylib
      - criterion: "Parser metadata is read from nvim-treesitter's parsers.lua or lockfile.json"
        verification: |
          # Verify the installed parser matches nvim-treesitter's pinned version
          # (implementation will log the commit hash used)
          ./target/release/treesitter-ls install-parser lua --verbose 2>&1 | grep -i "revision\|commit"
      - criterion: "Parser compilation requires tree-sitter CLI and a C compiler"
        verification: |
          # On a system without tree-sitter CLI, should show clear error
          PATH=/nonexistent:$PATH ./target/release/treesitter-ls install-parser lua 2>&1 | grep -qi "tree-sitter.*not found\|compiler"
      - criterion: "Running with `--data-dir` flag uses custom directory"
        verification: |
          ./target/release/treesitter-ls install-parser lua --data-dir /tmp/treesitter-test
          test -f /tmp/treesitter-test/parsers/lua.so || test -f /tmp/treesitter-test/parsers/lua.dylib
    story_points: 8
    dependencies:
      - PBI-004  # Requires CLI infrastructure
    decisions:
      - question: "Should we use tree-sitter CLI or compile directly with cc crate?"
        decision: "Use tree-sitter CLI (`tree-sitter build`)"
        rationale: |
          Simpler implementation, fewer dependencies to maintain, leverages tree-sitter's
          battle-tested build system. Can add cc crate option later if needed.
      - question: "How should we handle parsers that need C++ compiler (like tree-sitter-cpp)?"
        decision: "Document as known limitation in first iteration"
        rationale: |
          Most popular parsers (lua, rust, python, go, etc.) use C. C++ parsers are edge cases.
          Document that tree-sitter CLI handles this automatically if available.
    technical_notes: |
      ## Implementation Strategy

      1. Parse nvim-treesitter metadata to get parser info
      2. Clone parser repository at specific revision
      3. Run `tree-sitter build` to compile
      4. Copy .so/.dylib to data directory

      ## Key Files to Create/Modify
      - src/install/parser.rs - Parser installation logic
      - src/install/metadata.rs - nvim-treesitter metadata parsing
      - src/bin/main.rs - Add install-parser subcommand
      - Cargo.toml - Add git2 dependency (or use git CLI)

      ## nvim-treesitter Metadata Structure (parsers.lua)
      ```lua
      -- From nvim-treesitter/lua/nvim-treesitter/parsers.lua
      list.lua = {
        install_info = {
          url = "https://github.com/tree-sitter-grammars/tree-sitter-lua",
          revision = "v0.2.0",
          branch = "master",  -- optional
        },
        maintainers = { "@..." },
        tier = 1,
      }
      ```

      ## Alternative: lockfile.json (easier to parse)
      ```json
      {
        "lua": {
          "revision": "abc123..."
        }
      }
      ```

      ## Installation Flow
      1. Read metadata for language
      2. Create temp directory
      3. Clone repo at revision: `git clone --depth 1 --branch <revision> <url>`
      4. Navigate to parser location (some are in subdirectories)
      5. Run `tree-sitter build`
      6. Copy output to data directory

  - id: PBI-007
    status: draft
    story:
      role: "user of treesitter-ls"
      capability: "automatically install parser dependencies when installing a parser"
      benefit: "I can install C++ parser without manually installing C parser first"
    acceptance_criteria:
      - criterion: "Installing tree-sitter-cpp automatically installs tree-sitter-c first"
        verification: |
          rm -rf ~/.local/share/treesitter-ls/parsers/c.* ~/.local/share/treesitter-ls/parsers/cpp.*
          ./target/release/treesitter-ls install-parser cpp
          test -f ~/.local/share/treesitter-ls/parsers/c.so || test -f ~/.local/share/treesitter-ls/parsers/c.dylib
          test -f ~/.local/share/treesitter-ls/parsers/cpp.so || test -f ~/.local/share/treesitter-ls/parsers/cpp.dylib
      - criterion: "Dependency installation shows progress message"
        verification: |
          ./target/release/treesitter-ls install-parser cpp 2>&1 | grep -qi "installing.*c.*dependency"
      - criterion: "Already installed dependencies are skipped"
        verification: |
          ./target/release/treesitter-ls install-parser cpp 2>&1 | grep -qi "already installed\|skipping"
    story_points: 3
    dependencies:
      - PBI-006  # Requires parser installation to work first
    technical_notes: |
      ## Implementation Strategy

      1. Parse `requires` field from nvim-treesitter metadata
      2. Build dependency graph
      3. Install dependencies in topological order

      ## Known Dependencies (from nvim-treesitter)
      - cpp requires c
      - typescript requires javascript (for TSX)
      - tsx requires typescript, javascript

  - id: PBI-008
    status: draft
    story:
      role: "user of treesitter-ls"
      capability: "have treesitter-ls automatically install missing parsers when I open a file"
      benefit: "I get syntax highlighting for any language without running install commands manually"
    acceptance_criteria:
      - criterion: "Opening a .lua file when no Lua parser is installed prompts for installation"
        verification: |
          # This would be tested via LSP integration test
          # The server should send a notification/prompt to the client
      - criterion: "Auto-install can be disabled in settings"
        verification: |
          # Verify setting is respected
      - criterion: "Failed auto-install shows clear error message via LSP notification"
        verification: |
          # The server should report the error to the client
    story_points: 5
    dependencies:
      - PBI-006  # Requires parser installation
      - PBI-005  # Requires query installation
    technical_notes: |
      ## Implementation Strategy

      This is an enhancement that integrates installation into the LSP flow.
      Consider deferring until core installation is proven stable.

      1. When document opened, check if parser exists
      2. If not, check settings for auto_install
      3. If enabled, trigger async installation
      4. Send LSP notification with progress/result

      ## Settings Addition
      ```json
      {
        "treesitter": {
          "autoInstall": true,
          "autoInstallQueries": true
        }
      }
      ```
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
  number: 3
  pbi_id: PBI-004
  story:
    role: "user of treesitter-ls"
    capability: "run treesitter-ls with CLI subcommands"
    benefit: "I can manage parsers and queries using the same binary I use for the language server"
  status: in_progress

  subtasks:
    # Sprint 3: CLI Infrastructure (PBI-004)
    # Strategy: Incremental CLI addition with backward compatibility
    #   1. Add clap dependency and basic CLI structure with --help
    #   2. Add install subcommand with placeholder implementation
    #   3. Ensure backward compatibility (no args = LSP server)
    #   4. Add --version flag
    #
    # Each subtask follows TDD: write test, implement, refactor

    # --- Phase 1: Basic CLI Structure ---

    - test: "Running `treesitter-ls --help` shows help message with program description"
      implementation: "Add clap dependency, create CLI struct with Parser derive, update main.rs to parse args"
      type: behavioral
      status: green
      commits: []
      files_modified:
        - Cargo.toml (added clap dependency)
        - src/bin/main.rs (added CLI parsing with clap)
        - tests/test_cli.rs (new integration tests)

    - test: "Running `treesitter-ls install --help` shows install subcommand usage with LANGUAGE argument"
      implementation: "Add Install subcommand with language argument to CLI struct"
      type: behavioral
      status: green
      commits: []

    # --- Phase 2: Subcommand Implementation ---

    - test: "Running `treesitter-ls install lua` prints placeholder error message (not yet implemented)"
      implementation: "Handle Install command in main, print informative placeholder message and exit with code 1"
      type: behavioral
      status: green
      commits: []

    # --- Phase 3: Backward Compatibility ---

    - test: "Running `treesitter-ls` with no arguments starts the LSP server (backward compatible)"
      implementation: "When no subcommand is provided, execute current LSP server startup code"
      type: behavioral
      status: green
      commits: []

  notes: |
    Sprint 3 started via Sprint Planning.
    Sprint Goal: Enable CLI subcommands while maintaining backward compatibility for LSP mode.

    Implementation Strategy:
    - Phase 1: Add clap and basic CLI structure (subtasks 1-2)
    - Phase 2: Implement install placeholder (subtask 3)
    - Phase 3: Ensure backward compatibility (subtask 4)

    Key constraint: All tests must pass after each subtask completion.
    Backward compatibility is critical - existing users should not be affected.

    IMPLEMENTATION COMPLETE - PENDING VERIFICATION:
    Files created/modified:
    - Cargo.toml: Added clap = { version = "4.5", features = ["derive"] }
    - src/bin/main.rs: Added CLI parsing with Commands enum and backward-compatible LSP startup
    - tests/test_cli.rs: Added 5 integration tests for CLI functionality

    To verify and complete Sprint:
    1. Run: cargo test --test test_cli
    2. Run: make test
    3. Run: make check
    4. Run: make test_nvim
    5. If all pass, commit with: git add . && git commit -m "feat(cli): add CLI infrastructure with clap"
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
  - sprint: 2
    pbi: PBI-002
    story: "As a maintainer of treesitter-ls, I want to use a single semantic tokens handler that works with or without injections, so that I have simpler code with better separation of concerns and reduced conditional complexity"
    outcome: "Unified semantic token handlers; removed LSP layer injection branching; fixed PBI-003 delta bug as side effect"
    acceptance:
      status: accepted
      criteria_verified:
        - "Unified handler accepts optional coordinator and parser_pool parameters"
        - "semantic_tokens_full in LSP layer calls only one handler (no if/else branching)"
        - "semantic_tokens_range in LSP layer calls only one handler (no if/else branching)"
        - "When coordinator/parser_pool are None, function returns same tokens as current non-injection handler"
        - "When coordinator/parser_pool are Some, function returns tokens including injected content"
        - "All existing semantic token tests pass"
        - "Old non-injection handler functions are removed"
        - "Old injection-specific handler functions are removed"
      dod_verified:
        - "make test: PASSED (141 unit tests)"
        - "make check: PASSED"
        - "make test_nvim: PASSED"
      bonus:
        - "PBI-003 (delta injection bug) fixed as side effect of unification"
    subtasks_completed: 8
    commits_actual: 5
    unit_tests: 141
    impediments: 0

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
    commits_actual: 1
    unit_tests: 138
    e2e_tests: 20
    impediments: 0
`````````

---

## 5. Retrospective Log

`````````yaml
# After each sprint, record what to improve
retrospectives:
  - sprint: 2
    pbi: PBI-002
    prime_directive_read: true

    what_went_well:
      - item: "Significant Commit Discipline Improvement"
        detail: |
          Sprint 2 achieved 5 commits for 8 subtasks (62.5% ratio) compared to Sprint 1's
          1 commit for 8 subtasks (12.5% ratio). This represents a 5x improvement in commit
          granularity, directly addressing the critical action item AI-004 from Sprint 1.
      - item: "Expand-Contract Refactoring Pattern"
        detail: |
          Used the Expand-Contract pattern effectively:
          1. Expanded API to accept Option parameters
          2. Updated callers to use new unified interface
          3. Contracted by removing old duplicate handlers
          This ensured tests passed at every step.
      - item: "Bug Fix as Side Effect"
        detail: |
          PBI-003 (delta injection bug) was automatically fixed as a consequence of
          unifying the handlers. This validates the refactoring approach - when you
          have only one code path, bugs in alternate paths disappear.
      - item: "Clear Phase Structure"
        detail: |
          The 3-phase structure (full handlers, range handlers, delta+cleanup) made
          progress visible and kept the work organized.
      - item: "Zero Impediments"
        detail: "Sprint completed without any blockers, continuing the streak from Sprint 1."

    what_could_improve:
      - item: "Commit Granularity Still Below Ideal"
        detail: |
          While 5 commits for 8 subtasks is a major improvement over Sprint 1 (1 commit
          for 8 subtasks), the TDD ideal would be closer to 1-2 commits per subtask,
          which would mean 8-16 commits. Some subtasks were combined into single commits.
        root_cause: |
          Some subtasks (like 1 and 3, or 4 and 6) shared commits because the LSP layer
          changes were done together with the handler changes.
        impact: "low"

      - item: "Subtask Overlap"
        detail: |
          Subtasks 1 and 3 shared a commit, as did subtasks 4 and 6. This suggests the
          subtask breakdown could be refined - either combine related subtasks or ensure
          each subtask truly represents an independent commit.
        root_cause: |
          The subtask breakdown separated "handler change" from "LSP caller change" but
          in practice these were done atomically.
        impact: "low"

    action_items:
      - id: AI-006
        action: "Consider combining handler + caller updates into single subtasks"
        detail: |
          When refactoring a function signature, the handler change and all caller updates
          should be in the same subtask since they must be committed together to maintain
          a green build.
        owner: "@scrum-team-developer"
        status: pending
        backlog: sprint

      - id: AI-007
        action: "Mark PBI-003 as resolved/closed in Product Backlog"
        detail: |
          PBI-003 (delta injection bug) was fixed as a side effect of PBI-002.
          The Product Owner should formally close it or mark it as superseded.
        owner: "@scrum-team-product-owner"
        status: pending
        backlog: sprint

    insights:
      - insight: "Process improvements compound across sprints"
        analysis: |
          The commit discipline enforcement added after Sprint 1 (AI-004) paid off with
          5x improvement in Sprint 2. This validates the retrospective process - action
          items that address root causes lead to measurable improvements.

      - insight: "Structural refactoring eliminates bug categories"
        analysis: |
          By unifying handlers, we didn't just fix PBI-003 - we made that entire class
          of bugs (inconsistent behavior between injection and non-injection paths)
          impossible. This is "making illegal states unrepresentable" in action.

      - insight: "Expand-Contract is ideal for API unification"
        analysis: |
          The pattern of first adding optional parameters to the "richer" implementation,
          then migrating callers, then removing the "simpler" implementation works well
          for this type of refactoring. All tests pass at every step.

    metrics:
      unit_tests_added: 3
      subtasks_completed: 8
      impediments_encountered: 0
      dod_criteria_met: 3
      commits_expected: 8
      commits_actual: 5
      commit_discipline_score: 0.625
      improvement_from_sprint_1: "5x (12.5% -> 62.5%)"

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
