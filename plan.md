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
  number: 7
  pbi: PBI-008
  status: in_progress
  subtasks_completed: 0
  subtasks_total: 7
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
      https://raw.githubusercontent.com/nvim-treesitter/nvim-treesitter/main/runtime/queries/{lang}/highlights.scm
      https://raw.githubusercontent.com/nvim-treesitter/nvim-treesitter/main/runtime/queries/{lang}/locals.scm
      https://raw.githubusercontent.com/nvim-treesitter/nvim-treesitter/main/runtime/queries/{lang}/injections.scm
      ```
      NOTE: Uses `main` branch (not `master`) and `runtime/queries/` path.

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

      ## nvim-treesitter Metadata Structure (parsers.lua - main branch format)
      ```lua
      -- From nvim-treesitter/lua/nvim-treesitter/parsers.lua (main branch)
      -- NOTE: Uses `main` branch, not `master`
      return {
        lua = {
          install_info = {
            url = 'https://github.com/tree-sitter-grammars/tree-sitter-lua',
            revision = 'abc123...',
            location = 'optional/subdir',  -- optional, for monorepos
          },
          maintainers = { '@...' },
          tier = 1,
        },
        -- 323+ languages supported
      }
      ```
      NOTE: The implementation dynamically parses parsers.lua to support all languages.

      ## Installation Flow
      1. Fetch parsers.lua from nvim-treesitter main branch
      2. Parse Lua table to extract url, revision, location
      3. Clone repo at revision: `git clone --depth 1 --branch <revision> <url>`
      4. Navigate to parser location (some are in subdirectories)
      5. Run `tree-sitter build`
      6. Copy output to data directory

  # PBI-007: CLOSED - Not Needed
  # Tree-sitter parsers are self-contained. Grammar inheritance (e.g., cpp inherits from c)
  # is resolved at code generation time, not at runtime. Each .dylib works independently.
  # Verified: cpp.dylib builds and works without c.dylib installed.

  - id: PBI-008
    status: ready
    story:
      role: "user of treesitter-ls"
      capability: "have treesitter-ls automatically install missing parsers when I open a file"
      benefit: "I get syntax highlighting for any language without running install commands manually"
    acceptance_criteria:
      - criterion: "Opening a .lua file when no Lua parser is installed triggers silent background installation"
        verification: |
          # Remove existing Lua parser/queries
          rm -rf ~/.local/share/treesitter-ls/parser/lua.* ~/.local/share/treesitter-ls/queries/lua
          # Open a .lua file in editor with treesitter-ls running
          # After ~30s, parser and queries should be installed
          test -f ~/.local/share/treesitter-ls/parser/lua.dylib || test -f ~/.local/share/treesitter-ls/parser/lua.so
          test -f ~/.local/share/treesitter-ls/queries/lua/highlights.scm
      - criterion: "LSP shows progress notification during installation"
        verification: |
          # Server sends window/showMessage or $/progress notifications
          # Check LSP log for "Installing parser for..." messages
      - criterion: "Auto-install can be disabled in initializationOptions"
        verification: |
          # With autoInstall: false, opening file with missing parser does NOT trigger install
      - criterion: "Failed auto-install shows clear error message via LSP notification"
        verification: |
          # On a system without tree-sitter CLI, should show error notification
      - criterion: "Auto-install does not block document operations (async)"
        verification: |
          # Document should be immediately usable (no syntax highlighting initially)
          # Syntax highlighting appears after installation completes
      - criterion: "Already-installing language is not triggered twice"
        verification: |
          # Opening multiple .lua files during Lua install doesn't spawn multiple installs
    story_points: 5
    dependencies:
      - PBI-006  # Requires parser installation
      - PBI-005  # Requires query installation
    decisions:
      - question: "How should auto-install be triggered?"
        decision: "Full silent install in background"
        rationale: |
          User preference: Seamless experience without prompts. Installation happens
          automatically when opening a file. Progress/errors shown via LSP notifications.
    technical_notes: |
      ## Implementation Strategy

      Full silent background installation when opening files with missing parsers.

      ### Flow
      1. On textDocument/didOpen, check if parser exists for the language
      2. If parser missing and autoInstall enabled:
         a. Add language to "installing" set (prevent duplicate installs)
         b. Spawn async task to install parser + queries
         c. Send window/showMessage with "Installing {lang}..." (info level)
         d. On completion: send success/error notification, remove from set
         e. Trigger re-parse of open documents for that language
      3. If parser exists, proceed normally

      ### Key Files to Modify
      - src/lsp/lsp_impl.rs - Add auto-install check in did_open
      - src/lsp/settings.rs - Add autoInstall setting
      - src/install/mod.rs - Expose async install function for LSP use

      ### Settings Addition
      ```json
      {
        "initializationOptions": {
          "autoInstall": true
        }
      }
      ```

      ### Concurrency Handling
      - Use HashSet<String> to track languages currently being installed
      - Prevent duplicate install attempts for same language
      - Use tokio::spawn for background installation

      ### Error Handling
      - Missing tree-sitter CLI: Show error notification with install instructions
      - Network failure: Show retry suggestion
      - Compilation failure: Show error with log path

  - id: PBI-009
    status: ready
    story:
      role: "user of treesitter-ls"
      capability: "install both parser and queries with a single command"
      benefit: "I can set up a language with one command instead of running two separate commands"
    acceptance_criteria:
      - criterion: "Running `treesitter-ls install lua` installs both parser and queries"
        verification: |
          rm -rf /tmp/treesitter-full-test
          ./target/release/treesitter-ls install lua --data-dir /tmp/treesitter-full-test
          test -f /tmp/treesitter-full-test/parsers/lua.dylib || test -f /tmp/treesitter-full-test/parsers/lua.so
          test -f /tmp/treesitter-full-test/queries/lua/highlights.scm
      - criterion: "Install command shows progress for both parser and queries"
        verification: |
          ./target/release/treesitter-ls install lua --data-dir /tmp/treesitter-full-test --force 2>&1 | grep -qi "parser\|queries"
      - criterion: "If parser install fails, queries are still attempted (and vice versa)"
        verification: |
          # Test with a language that has queries but we can't compile (requires specific setup)
          # For now, verify the code handles partial failures gracefully
      - criterion: "Running with --queries-only skips parser installation"
        verification: |
          rm -rf /tmp/treesitter-queries-only
          ./target/release/treesitter-ls install lua --data-dir /tmp/treesitter-queries-only --queries-only
          test ! -f /tmp/treesitter-queries-only/parsers/lua.dylib
          test -f /tmp/treesitter-queries-only/queries/lua/highlights.scm
      - criterion: "Running with --parser-only skips query installation"
        verification: |
          rm -rf /tmp/treesitter-parser-only
          ./target/release/treesitter-ls install lua --data-dir /tmp/treesitter-parser-only --parser-only
          test -f /tmp/treesitter-parser-only/parsers/lua.dylib || test -f /tmp/treesitter-parser-only/parsers/lua.so
          test ! -d /tmp/treesitter-parser-only/queries/lua
    story_points: 2
    dependencies:
      - PBI-005  # Requires query installation
      - PBI-006  # Requires parser installation
    technical_notes: |
      ## Implementation Strategy

      This unifies the existing `install` stub command with the real implementations.

      1. Update the `Install` command in main.rs to call both:
         - `parser::install_parser()` - compile and install parser
         - `queries::install_queries()` - download query files
      2. Add `--queries-only` and `--parser-only` flags
      3. Handle partial failures gracefully (continue if one fails)
      4. Show combined progress output

      ## Key Files to Modify
      - src/bin/main.rs - Update Install command handler

      ## Example Usage
      ```
      treesitter-ls install lua                    # Install parser + queries
      treesitter-ls install lua --queries-only     # Only download queries
      treesitter-ls install lua --parser-only      # Only compile parser
      treesitter-ls install lua --force            # Overwrite existing
      ```

  # ============================================================================
  # PRIORITIZED BACKLOG (Post-Sprint 6 Refinement)
  # ============================================================================
  # Priority order (top to bottom):
  #   1. PBI-008: Auto-install on file open (USER PRIORITY - ready)
  #   2. PBI-010: Cache parsers.lua (performance foundation - ready)
  #   3. PBI-011: Batch install (UX improvement - draft)
  #   4. PBI-012: Progress indicators (nice-to-have - draft)
  # ============================================================================

  - id: PBI-010
    status: ready
    story:
      role: "user of treesitter-ls"
      capability: "have parsers.lua cached locally to avoid repeated HTTP requests"
      benefit: "I can run multiple install commands quickly without waiting for network requests"
    acceptance_criteria:
      - criterion: "Second install command in same session reuses cached parsers.lua"
        verification: |
          # Time two consecutive installs - second should be faster
          time ./target/release/treesitter-ls install lua --data-dir /tmp/test1 --force
          time ./target/release/treesitter-ls install rust --data-dir /tmp/test2 --force
      - criterion: "Cache expires after 1 hour (configurable)"
        verification: |
          # Verify cache TTL behavior through code review/unit tests
      - criterion: "Cache can be bypassed with --no-cache flag"
        verification: |
          ./target/release/treesitter-ls install lua --no-cache 2>&1 | grep -i "fetch"
      - criterion: "Cache is stored in data directory"
        verification: |
          ./target/release/treesitter-ls install lua --data-dir /tmp/cache-test
          test -f /tmp/cache-test/cache/parsers.lua
    story_points: 3
    dependencies: []
    technical_notes: |
      ## Implementation Strategy

      1. Create src/install/cache.rs module
      2. Store parsers.lua in {data_dir}/cache/parsers.lua with timestamp
      3. Check cache age before fetching from network
      4. Default TTL: 1 hour (3600 seconds)

      ## Cache Structure
      ```
      ~/.local/share/treesitter-ls/
        cache/
          parsers.lua           # Cached content
          parsers.lua.meta      # Timestamp and TTL info
      ```

      ## Key Files to Create/Modify
      - src/install/cache.rs - New cache module
      - src/install/metadata.rs - Use cache before network fetch
      - src/bin/main.rs - Add --no-cache flag

  - id: PBI-011
    status: draft
    story:
      role: "user of treesitter-ls"
      capability: "install multiple languages with a single command"
      benefit: "I can set up my development environment quickly without running separate commands"
    acceptance_criteria:
      - criterion: "Running `treesitter-ls install lua rust python` installs all three languages"
        verification: |
          rm -rf /tmp/batch-test
          ./target/release/treesitter-ls install lua rust python --data-dir /tmp/batch-test
          test -f /tmp/batch-test/parsers/lua.dylib
          test -f /tmp/batch-test/parsers/rust.dylib
          test -f /tmp/batch-test/parsers/python.dylib
      - criterion: "Progress shows which language is being installed (1/3, 2/3, 3/3)"
        verification: |
          ./target/release/treesitter-ls install lua rust --force 2>&1 | grep -E "\[1/2\]|\[2/2\]"
      - criterion: "If one language fails, others are still attempted"
        verification: |
          # Install with one invalid language
          ./target/release/treesitter-ls install lua invalid_lang rust --force 2>&1 | grep -i "lua.*success"
      - criterion: "Summary shows success/failure count at the end"
        verification: |
          ./target/release/treesitter-ls install lua rust --force 2>&1 | grep -i "2.*success"
    story_points: 3
    dependencies:
      - PBI-010  # Cache makes batch install much faster
    technical_notes: |
      ## Implementation Strategy

      1. Change `language: String` to `languages: Vec<String>` in CLI
      2. Loop through languages, calling install for each
      3. Track success/failure for each language
      4. Print summary at end

      ## Key Files to Modify
      - src/bin/main.rs - Update Install command to accept multiple languages

      ## UX Considerations
      - Show progress: "[1/3] Installing lua..."
      - Continue on failure (don't stop at first error)
      - Summary: "Installed 2/3 languages (1 failed)"

  - id: PBI-012
    status: draft
    story:
      role: "user of treesitter-ls"
      capability: "see progress indicators during parser compilation"
      benefit: "I know the install is working and approximately how long it will take"
    acceptance_criteria:
      - criterion: "Parser compilation shows phases (cloning, building, installing)"
        verification: |
          ./target/release/treesitter-ls install lua --force 2>&1 | grep -i "cloning\|building\|installing"
      - criterion: "Long operations show spinner or elapsed time"
        verification: |
          # Verify through manual testing - compilation shows activity
      - criterion: "Quiet mode (--quiet) suppresses progress output"
        verification: |
          ./target/release/treesitter-ls install lua --quiet --force 2>&1 | wc -l
    story_points: 2
    dependencies: []
    technical_notes: |
      ## Implementation Strategy

      1. Add indicatif crate for progress bars/spinners
      2. Update parser.rs to emit progress events
      3. Show phases: Fetching metadata -> Cloning repo -> Building parser -> Installing

      ## Key Files to Modify
      - Cargo.toml - Add indicatif dependency
      - src/install/parser.rs - Add progress callbacks
      - src/bin/main.rs - Display progress indicators

      ## Progress Phases
      1. "Fetching parser metadata..." (fast, network)
      2. "Cloning tree-sitter-{lang}..." (medium, network)
      3. "Building parser..." (slow, CPU intensive)
      4. "Installing parser..." (fast, file copy)
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
  number: 7
  pbi_id: PBI-008
  story:
    role: "user of treesitter-ls"
    capability: "have treesitter-ls automatically install missing parsers when I open a file"
    benefit: "I get syntax highlighting for any language without running install commands manually"
  status: in_progress

  subtasks:
    # Sprint 7: Auto-install on File Open (PBI-008)
    # Full silent background installation when opening files with missing parsers.
    #
    # Implementation Strategy:
    # 1. Add autoInstall setting to configuration
    # 2. Track languages currently being installed (prevent duplicates)
    # 3. Check parser existence on did_open
    # 4. Spawn async install task for missing parsers
    # 5. Send LSP notifications for progress/errors
    # 6. Re-parse open documents after install completes

    # Subtask 1: Add autoInstall setting to configuration
    - test: "TreeSitterSettings parses autoInstall field from JSON/TOML configuration"
      implementation: "Add auto_install: Option<bool> field to TreeSitterSettings with serde rename to autoInstall"
      type: behavioral
      status: pending
      commits: []
      files_to_modify:
        - src/config/settings.rs

    # Subtask 2: Propagate autoInstall setting through WorkspaceSettings
    - test: "WorkspaceSettings exposes auto_install field that defaults to false"
      implementation: "Add auto_install: bool field to WorkspaceSettings, update From impl for TreeSitterSettings"
      type: behavioral
      status: pending
      commits: []
      files_to_modify:
        - src/config/settings.rs

    # Subtask 3: Add installing languages tracker to TreeSitterLs
    - test: "TreeSitterLs can track which languages are currently being installed"
      implementation: "Add installing_languages: Mutex<HashSet<String>> field to TreeSitterLs"
      type: behavioral
      status: pending
      commits: []
      files_to_modify:
        - src/lsp/lsp_impl.rs

    # Subtask 4: Expose async install function for LSP use
    - test: "install module exposes async install_language_async function"
      implementation: "Add async wrapper around parser::install_parser and queries::install_queries"
      type: behavioral
      status: pending
      commits: []
      files_to_modify:
        - src/install/mod.rs

    # Subtask 5: Check parser existence on did_open
    - test: "did_open checks if parser exists for the language and triggers install if missing"
      implementation: "In did_open, check language coordinator for parser, spawn install if autoInstall enabled and parser missing"
      type: behavioral
      status: pending
      commits: []
      files_to_modify:
        - src/lsp/lsp_impl.rs

    # Subtask 6: Send LSP notifications for install progress
    - test: "Auto-install sends window/showMessage notifications for progress and errors"
      implementation: "Use client.show_message() to notify user of install start, completion, and errors"
      type: behavioral
      status: pending
      commits: []
      files_to_modify:
        - src/lsp/lsp_impl.rs

    # Subtask 7: Re-parse documents and refresh tokens after install
    - test: "After successful auto-install, open documents are re-parsed and semantic tokens refreshed"
      implementation: "After install completes, re-load language, re-parse affected documents, call semantic_tokens_refresh"
      type: behavioral
      status: pending
      commits: []
      files_to_modify:
        - src/lsp/lsp_impl.rs

  notes: |
    Sprint 7: Auto-install on File Open (PBI-008)
    Sprint Goal: Enable seamless syntax highlighting for any language by automatically
    installing missing parsers when files are opened.

    Key Design Decisions:
    - Full silent background install (no prompts, async)
    - HashSet<String> tracks languages currently being installed (prevent duplicates)
    - Uses tokio::spawn for background installation
    - LSP notifications via window/showMessage for progress/errors
    - autoInstall defaults to false for backward compatibility

    Implementation Flow:
    1. User opens a file (e.g., .lua)
    2. did_open determines language from path/language_id
    3. Check if parser exists for language
    4. If missing AND autoInstall enabled:
       a. Check if language already being installed (skip if so)
       b. Add language to installing set
       c. Send "Installing {lang}..." notification
       d. Spawn async task to install parser + queries
       e. On completion: remove from set, send success/error notification
       f. Trigger re-parse of open documents for that language
       g. Request semantic tokens refresh
    5. If parser exists, proceed normally

    Files to Modify:
    - src/config/settings.rs - Add autoInstall setting
    - src/lsp/lsp_impl.rs - Add auto-install logic in did_open
    - src/install/mod.rs - Expose async install function
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
  - sprint: 6
    pbi: PBI-009
    story: "As a user of treesitter-ls, I want to install both parser and queries with a single command, so that I can set up a language with one command instead of running two separate commands"
    outcome: "Combined install command; --queries-only and --parser-only flags; partial failure handling"
    acceptance:
      status: accepted
      criteria_verified:
        - "treesitter-ls install lua installs both parser and queries"
        - "Install command shows progress for both parser and queries"
        - "Running with --queries-only skips parser installation"
        - "Running with --parser-only skips query installation"
        - "Partial failures handled gracefully (one can fail, other continues)"
      dod_verified:
        - "cargo test: PASSED (all tests)"
        - "cargo clippy -- -D warnings: PASSED"
        - "cargo fmt --check: PASSED"
        - "cargo build --release: PASSED"
    subtasks_completed: 1
    commits_actual: 1
    impediments: 0

  - sprint: 5
    pbi: PBI-006
    story: "As a user of treesitter-ls, I want to compile and install a Tree-sitter parser for a language, so that I get a working parser without manually cloning repos and running build commands"
    outcome: "Parser compilation via tree-sitter CLI; nvim-treesitter metadata for repository URLs and revisions; monorepo support"
    acceptance:
      status: accepted
      criteria_verified:
        - "treesitter-ls install-parser lua downloads and compiles Lua parser"
        - "Parser metadata read from nvim-treesitter lockfile.json"
        - "Parser compilation requires tree-sitter CLI and C compiler"
        - "--data-dir flag uses custom directory"
      dod_verified:
        - "cargo test: PASSED"
        - "cargo clippy -- -D warnings: PASSED"
        - "cargo fmt --check: PASSED"
    subtasks_completed: 4
    commits_actual: 2
    impediments: 0

  - sprint: 4
    pbi: PBI-005
    story: "As a user of treesitter-ls, I want to download Tree-sitter query files for a language, so that I get syntax highlighting and go-to-definition without manually finding and copying query files"
    outcome: "Query downloading from nvim-treesitter raw GitHub URLs; default data directory support; force overwrite"
    acceptance:
      status: accepted
      criteria_verified:
        - "treesitter-ls install-queries lua downloads Lua queries"
        - "Queries downloaded from nvim-treesitter repository"
        - "--data-dir flag uses custom directory"
        - "Unsupported language shows helpful error"
        - "--force flag overwrites existing queries"
      dod_verified:
        - "cargo test: PASSED"
        - "cargo clippy -- -D warnings: PASSED"
        - "cargo fmt --check: PASSED"
    subtasks_completed: 3
    commits_actual: 2
    impediments: 0

  - sprint: 3
    pbi: PBI-004
    story: "As a user of treesitter-ls, I want to run treesitter-ls with CLI subcommands, so that I can manage parsers and queries using the same binary I use for the language server"
    outcome: "CLI infrastructure with clap; install subcommand; backward compatible LSP server mode"
    acceptance:
      status: accepted
      criteria_verified:
        - "treesitter-ls --help shows available subcommands"
        - "treesitter-ls install --help shows install command usage"
        - "treesitter-ls with no args starts LSP server (backward compatible)"
        - "treesitter-ls install lua prints placeholder error"
      dod_verified:
        - "cargo test: PASSED"
        - "cargo clippy -- -D warnings: PASSED"
        - "cargo fmt --check: PASSED"
    subtasks_completed: 4
    commits_actual: 2
    impediments: 0

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
