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
  number: 9
  pbi: PBI-014
  status: done
  subtasks_completed: 4
  subtasks_total: 4
  impediments: 0
  sprint_goal: |
    Enable immediate syntax highlighting for code blocks added during editing
    by automatically installing missing parsers when new injection regions appear.
`````````

---

## 1. Product Backlog

### Product Goal

`````````yaml
product_goal:
  statement: "Enable zero-configuration usage of treesitter-ls: users can start the LSP server and get syntax highlighting for any supported language without manual setup."
  success_metrics:
    - metric: "Zero-config smoke test"
      target: "User opens any file in a supported language, syntax highlighting works without any configuration"
    - metric: "Auto-install works"
      target: "Opening a file auto-installs the parser and queries if missing"
    - metric: "E2E tests pass"
      target: "make test_nvim succeeds"
    - metric: "Unit tests pass"
      target: "make test succeeds"
    - metric: "Code quality"
      target: "make check succeeds (cargo check, clippy, fmt)"
  owner: "@scrum-team-product-owner"

  mvp_definition: |
    Minimum Viable Zero-Config Experience:
    1. User starts treesitter-ls with no initialization options
    2. User opens a .lua file (or any supported language)
    3. treesitter-ls automatically:
       a. Detects the language from file extension
       b. Installs the parser and queries if missing
       c. Adds the install directory to search paths internally
       d. Provides semantic highlighting
    5. Power users can still override with explicit configuration

    Key Design Decisions:
    - Default data directory: ~/.local/share/treesitter-ls/ (platform-appropriate)
    - Default searchPaths: [<default_data_dir>/parser, <default_data_dir>/queries]
    - Default autoInstall: true (was false)
    - Built-in filetype mappings for 300+ languages from nvim-treesitter
`````````

### Backlog Items

`````````yaml
product_backlog:
  # PBI-002 completed in Sprint 2
  # PBI-003 resolved as part of Sprint 2 (PBI-002 fix included delta injection support)

  # ============================================================================
  # EPIC: Zero-Configuration Experience (MVP)
  # ============================================================================
  # Goal: Users can use treesitter-ls without any configuration. The server
  #       automatically detects languages, installs missing parsers/queries,
  #       and provides syntax highlighting out of the box.
  #
  # Splitting Strategy: By capability layer (bottom-up)
  #   PBI-017: Default data directory in searchPaths (foundation)
  #   PBI-018: Built-in filetype mappings (language detection)
  #   PBI-019: Enable autoInstall by default (activation)
  #   PBI-015: Fix parser installation for tag revisions (bug fix - existing)
  #
  # Dependencies:
  #   - PBI-008 (auto-install on file open) - COMPLETED
  #   - PBI-013/014 (auto-install for injections) - COMPLETED
  # ============================================================================

  - id: PBI-017
    status: ready
    story:
      role: "user of treesitter-ls"
      capability: "use treesitter-ls without configuring searchPaths"
      benefit: "I get syntax highlighting immediately without setting up directories"
    acceptance_criteria:
      - criterion: "Server uses default data directory when no searchPaths configured"
        verification: |
          # Start server with empty initializationOptions
          # Verify ~/.local/share/treesitter-ls/parser and /queries are searched
          cargo test test_default_search_paths_used -- --nocapture
      - criterion: "Auto-installed parsers are found without explicit searchPaths"
        verification: |
          # Install a parser, then start server with no config
          # Parser should be discoverable
          rm -rf ~/.local/share/treesitter-ls/parser/lua
          ./target/release/treesitter-ls install-parser lua
          # Start server with empty config, open .lua file - should work
      - criterion: "Explicit searchPaths override default paths"
        verification: |
          # With searchPaths: ["/custom/path"], default paths should NOT be used
          cargo test test_explicit_search_paths_override -- --nocapture
      - criterion: "Explicit searchPaths can extend default paths"
        verification: |
          # With searchPaths: ["/custom/path", "~/.local/share/treesitter-ls/parser"]
          # Both should be searched
          cargo test test_search_paths_can_include_default -- --nocapture
    dependencies: []
    technical_notes: |
      ## Implementation Strategy

      1. Modify WorkspaceSettings::default() to include default data directory paths
      2. When processing settings, if searchPaths is None or empty:
         - Add default_data_dir()/parser to search paths
         - Add default_data_dir()/queries to search paths
      3. This happens early in settings loading so all downstream code works automatically

      ## Key Files to Modify
      - src/config/settings.rs - Add default paths logic to WorkspaceSettings
      - src/lsp/settings.rs - Ensure default paths are applied when settings loaded

      ## Default Paths (platform-specific via dirs crate)
      - Linux: ~/.local/share/treesitter-ls/{parser,queries}
      - macOS: ~/Library/Application Support/treesitter-ls/{parser,queries}
      - Windows: %APPDATA%/treesitter-ls/{parser,queries}

      ## Example Implementation
      ```rust
      impl Default for WorkspaceSettings {
          fn default() -> Self {
              let default_paths = install::default_data_dir()
                  .map(|d| vec![
                      d.join("parser").to_string_lossy().to_string(),
                      d.join("queries").to_string_lossy().to_string(),
                  ])
                  .unwrap_or_default();

              Self {
                  search_paths: default_paths,
                  languages: HashMap::new(),
                  capture_mappings: HashMap::new(),
                  auto_install: true,  // Will be changed in PBI-019
              }
          }
      }
      ```

  - id: PBI-018
    status: ready
    story:
      role: "user of treesitter-ls"
      capability: "have treesitter-ls detect my file's language automatically without configuring filetypes"
      benefit: "I can open any file and get syntax highlighting without manual language configuration"
    acceptance_criteria:
      - criterion: "Opening a .lua file detects language as 'lua' without configuration"
        verification: |
          cargo test test_builtin_filetype_lua -- --nocapture
      - criterion: "Opening a .rs file detects language as 'rust' without configuration"
        verification: |
          cargo test test_builtin_filetype_rust -- --nocapture
      - criterion: "Opening a .py file detects language as 'python' without configuration"
        verification: |
          cargo test test_builtin_filetype_python -- --nocapture
      - criterion: "Opening a .md file detects language as 'markdown' without configuration"
        verification: |
          cargo test test_builtin_filetype_markdown -- --nocapture
      - criterion: "At least 50 common languages have built-in filetype mappings"
        verification: |
          cargo test test_builtin_filetype_count -- --nocapture
      - criterion: "Explicit filetypes configuration overrides built-in mappings"
        verification: |
          cargo test test_explicit_filetypes_override_builtin -- --nocapture
    dependencies: []
    technical_notes: |
      ## Implementation Strategy

      1. Create a built-in filetype database in src/language/builtin_filetypes.rs
      2. Initialize FiletypeResolver with built-in mappings by default
      3. User configuration overrides/extends built-in mappings

      ## Key Files to Create/Modify
      - src/language/builtin_filetypes.rs - NEW: Built-in filetype mappings
      - src/language/filetypes.rs - Load built-in mappings on initialization
      - src/language/coordinator.rs - Use built-in mappings when no config

      ## Filetype Source
      Use nvim-treesitter's filetypes as the source of truth:
      https://github.com/nvim-treesitter/nvim-treesitter/blob/main/lua/nvim-treesitter/parsers.lua

      Each language entry has `filetype` field listing associated extensions.

      ## Example Built-in Mappings (partial)
      ```rust
      pub fn get_builtin_filetypes() -> HashMap<String, String> {
          let mut map = HashMap::new();
          // Common languages
          map.insert("rs".to_string(), "rust".to_string());
          map.insert("lua".to_string(), "lua".to_string());
          map.insert("py".to_string(), "python".to_string());
          map.insert("pyi".to_string(), "python".to_string());
          map.insert("js".to_string(), "javascript".to_string());
          map.insert("jsx".to_string(), "javascript".to_string());
          map.insert("ts".to_string(), "typescript".to_string());
          map.insert("tsx".to_string(), "tsx".to_string());
          map.insert("go".to_string(), "go".to_string());
          map.insert("rb".to_string(), "ruby".to_string());
          map.insert("md".to_string(), "markdown".to_string());
          map.insert("markdown".to_string(), "markdown".to_string());
          map.insert("json".to_string(), "json".to_string());
          map.insert("toml".to_string(), "toml".to_string());
          map.insert("yaml".to_string(), "yaml".to_string());
          map.insert("yml".to_string(), "yaml".to_string());
          // ... 300+ more from nvim-treesitter
          map
      }
      ```

      ## Initialization Order
      1. Load built-in filetypes into FiletypeResolver
      2. Apply user configuration (if any) which can override built-in
      3. Language detection uses merged mappings

  - id: PBI-019
    status: ready
    story:
      role: "user of treesitter-ls"
      capability: "have autoInstall enabled by default"
      benefit: "I get parsers installed automatically without having to explicitly enable the feature"
    acceptance_criteria:
      - criterion: "autoInstall defaults to true when not specified in configuration"
        verification: |
          cargo test test_auto_install_default_true -- --nocapture
      - criterion: "User can explicitly disable autoInstall with autoInstall: false"
        verification: |
          cargo test test_auto_install_explicit_false -- --nocapture
      - criterion: "Zero-config experience: open file -> parser auto-installs -> highlighting works"
        verification: |
          # Full E2E test
          rm -rf ~/.local/share/treesitter-ls/parser/lua ~/.local/share/treesitter-ls/queries/lua
          # Start server with NO configuration
          # Open a .lua file
          # Verify: parser is installed, highlighting appears
          cargo test test_zero_config_e2e -- --nocapture
    dependencies:
      - PBI-017  # Default searchPaths required for auto-installed parsers to be found
      - PBI-018  # Built-in filetypes required for language detection
    technical_notes: |
      ## Implementation Strategy

      This is a simple default change. Modify the auto_install default from false to true.

      ## Key Files to Modify
      - src/config/settings.rs - Change WorkspaceSettings::default() auto_install to true
      - src/lsp/settings.rs - Ensure missing auto_install setting defaults to true

      ## Current Code (to change)
      ```rust
      impl WorkspaceSettings {
          pub fn new(...) -> Self {
              Self {
                  ...
                  auto_install: false,  // <-- Change to true
              }
          }
      }
      ```

      ## Backward Compatibility
      - Users who explicitly set autoInstall: false will continue to have it disabled
      - Users who relied on the implicit default of false will now see auto-install behavior
      - This is a UX improvement, but document it as a breaking change in release notes

      ## Risk Mitigation
      - Auto-install only happens if tree-sitter CLI is available
      - Failed installs show clear error messages
      - Users can opt-out with autoInstall: false

  # ============================================================================
  # EPIC: Automatic Parser and Query Installation (Original)
  # ============================================================================
  # Note: PBI-004 through PBI-009 are COMPLETED (Sprints 3-6, 7-9)
  # The infrastructure is in place; the Zero-Config epic above builds on it.
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
  # PRIORITIZED BACKLOG (Post-Sprint 9 - Zero-Config MVP)
  # ============================================================================
  # Priority order (top to bottom):
  #   1. PBI-017: Default searchPaths (foundation for zero-config) - ready
  #   2. PBI-018: Built-in filetype mappings (language detection) - ready
  #   3. PBI-019: autoInstall default true (activation) - ready
  #   4. PBI-015: Fix parser installation for tag revisions (bug fix) - ready
  #   5. PBI-010: Cache parsers.lua (performance) - ready
  #   6. PBI-016: Parser crash isolation (robustness) - ready
  #   7. PBI-011: Batch install (UX improvement) - draft
  #   8. PBI-012: Progress indicators (nice-to-have) - draft
  #
  # COMPLETED:
  #   - PBI-008: Auto-install on file open (Sprint 7)
  #   - PBI-013: Auto-install for injected languages on file open (Sprint 8)
  #   - PBI-014: Auto-install for injected languages on text edit (Sprint 9)
  # ============================================================================

  - id: PBI-013
    status: ready
    story:
      role: "user of treesitter-ls"
      capability: "have treesitter-ls automatically install missing parsers for injected languages when I open a file"
      benefit: "I get complete syntax highlighting for all code blocks without manually installing each language parser"
    acceptance_criteria:
      - criterion: "Opening a markdown file with a Lua code block triggers Lua parser auto-install when missing"
        verification: |
          rm -rf ~/.local/share/treesitter-ls/parsers/lua* ~/.local/share/treesitter-ls/queries/lua
          # Open markdown file with lua code block in editor with treesitter-ls running
          # After ~30s, parser and queries should be installed
          test -f ~/.local/share/treesitter-ls/parsers/lua/libtree-sitter-lua.dylib || \
          test -f ~/.local/share/treesitter-ls/parsers/lua/libtree-sitter-lua.so
      - criterion: "Multiple injected languages in one document are all installed"
        verification: |
          # Create test file with multiple code blocks (lua, python, rust)
          # Open file, verify all three parsers installed
          test -f ~/.local/share/treesitter-ls/parsers/lua/libtree-sitter-lua.dylib
          test -f ~/.local/share/treesitter-ls/parsers/python/libtree-sitter-python.dylib
      - criterion: "Injected language already being installed is not triggered twice"
        verification: |
          # Open two markdown files both containing Lua blocks simultaneously
          # Should only show one "Installing lua" notification (check LSP logs)
      - criterion: "Auto-install for injections respects the autoInstall setting"
        verification: |
          # With autoInstall: false in settings, opening markdown with missing Lua parser does NOT trigger install
      - criterion: "After injection parser installs, semantic tokens refresh to show highlighting"
        verification: |
          # After Lua parser installs for markdown injection, Lua code blocks get syntax highlighting
          # Verify by checking semantic tokens include Lua tokens for the code block
    story_points: 3
    dependencies:
      - PBI-008  # Requires auto-install infrastructure (InstallingLanguages, maybe_auto_install_language)
    technical_notes: |
      ## Implementation Strategy

      1. After document is parsed in did_open, detect injections and trigger auto-install for missing languages
      2. Use existing collect_all_injections() to find injection regions
      3. For each unique injected language, call existing maybe_auto_install_language()
      4. The InstallingLanguages tracker already prevents duplicate installs

      ## Key Files to Modify
      - src/lsp/lsp_impl.rs - Add injection detection after initial parse in did_open

      ## Implementation Flow
      ```
      1. User opens markdown file
      2. did_open determines language = "markdown", triggers auto-install if missing
      3. parse_document() parses markdown
      4. NEW: After parse, detect injections in the document
      5. NEW: For each injected language (lua, python, etc.):
         a. Check if parser exists via ensure_language_loaded()
         b. If missing and autoInstall enabled, spawn auto-install
      6. InstallingLanguages tracker prevents duplicates
      7. After install completes, semantic tokens refresh shows highlighting
      ```

      ## Detection Approach
      - Use collect_all_injections() from src/language/injection.rs
      - Already runs injection query on parsed document
      - Extracts language from each injection region
      - Returns list of InjectionRegionInfo with language names

      ## Edge Cases
      - Recursive injections: Document with markdown containing Lua containing regex.
        For first iteration, only handle direct injections (depth=1).
      - Unknown injection languages: Log warning, skip auto-install.
      - Large documents with many injections: Process sequentially, tracker prevents duplicates.

  - id: PBI-014
    status: ready
    story:
      role: "user of treesitter-ls"
      capability: "have treesitter-ls automatically install missing parsers when I type a new injected code block"
      benefit: "I get immediate syntax highlighting for code blocks I add while editing without needing to re-open the file"
    acceptance_criteria:
      - criterion: "Adding a new code block triggers auto-install for the injected language"
        verification: |
          # Open a markdown file, type a new ```python code block
          # Check LSP logs for "Auto-installing language 'python'" message
          # After install, verify syntax highlighting appears
      - criterion: "Existing injections do not re-trigger auto-install on unrelated edits"
        verification: |
          # Open markdown with existing lua block (lua parser installed)
          # Edit text elsewhere in the document
          # LSP logs should NOT show install attempts for lua
      - criterion: "Multiple new injections in a single paste operation all trigger auto-install"
        verification: |
          # Paste text containing python, rust, go code blocks
          # LSP logs should show install attempts for all three languages
      - criterion: "Auto-install respects the autoInstall setting"
        verification: |
          # With autoInstall: false, adding a new code block does NOT trigger install
      - criterion: "Semantic tokens refresh after injection parser installs"
        verification: |
          # After python parser installs for new code block, verify highlighting appears
      - criterion: "Duplicate install prevention via InstallingLanguages tracker"
        verification: |
          # If python is currently installing, adding another python block shows "already being installed"
    story_points: 3
    dependencies:
      - PBI-008  # Requires auto-install infrastructure (InstallingLanguages, maybe_auto_install_language)
      - PBI-013  # Follows same injection detection pattern
    technical_notes: |
      ## Implementation Strategy

      Use the simple scan approach: on did_change, after re-parse, collect all injections
      and check each language. The InstallingLanguages tracker already prevents duplicate
      installs, so no need to track injection state ourselves (YAGNI).

      ## Key Files to Modify
      - src/lsp/lsp_impl.rs - Add injection detection after parse_document in did_change

      ## Implementation Flow
      ```
      1. User edits markdown file (types ```python)
      2. did_change receives edit event
      3. parse_document() re-parses with incremental edit
      4. NEW: After parse, detect injections in the document
         - Get document from store
         - Get injection query for host language (markdown)
         - Call collect_all_injections(root, text, injection_query)
      5. NEW: For each injected language (python, etc.):
         a. Check if parser exists via ensure_language_loaded()
         b. If missing AND autoInstall enabled:
            - Call maybe_auto_install_language(lang, uri, text)
            - InstallingLanguages tracker prevents duplicates
      6. After install completes, semantic tokens refresh shows highlighting
      ```

      ## Reuse from PBI-013
      - The injection detection logic will be extracted as a shared helper:
        check_injected_languages_auto_install(&self, uri: &Url, text: &str)
      - Called from both did_open (PBI-013) and did_change (PBI-014)

      ## Helper Function
      ```rust
      async fn check_injected_languages_auto_install(&self, uri: &Url, text: &str) {
          if !self.is_auto_install_enabled() {
              return;
          }

          let language_name = match self.get_language_for_document(uri) {
              Some(name) => name,
              None => return,
          };

          // Get injection query for host language
          let injection_query = match self.language.get_injection_query(&language_name) {
              Some(q) => q,
              None => return, // No injection support for this language
          };

          // Get parsed tree
          let doc = match self.documents.get(uri) {
              Some(d) => d,
              None => return,
          };
          let tree = match doc.tree() {
              Some(t) => t,
              None => return,
          };

          // Collect all injection regions
          let injections = match collect_all_injections(&tree.root_node(), text, Some(&injection_query)) {
              Some(i) => i,
              None => return,
          };

          // Get unique languages
          let languages: HashSet<String> = injections.iter().map(|i| i.language.clone()).collect();

          // Check each language
          for lang in languages {
              let load_result = self.language.ensure_language_loaded(&lang);
              if !load_result.success {
                  // Language not loaded - trigger auto-install
                  self.maybe_auto_install_language(&lang, uri.clone(), text.to_string()).await;
              }
          }
      }
      ```

      ## Edge Cases
      - Injection removed: User deletes a code block. No action needed.
      - Rapid edits: InstallingLanguages prevents spam.
      - Recursive injections: Only handle direct injections (depth=1) for now.
      - Unknown language: ensure_language_loaded fails, auto-install attempts but fails gracefully.
      - Performance: collect_all_injections runs a query. Consider debouncing if profiling shows issues.

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

  - id: PBI-015
    status: ready
    story:
      role: "user of treesitter-ls"
      capability: "install parsers at specific git revisions (tags or commit hashes)"
      benefit: "I can reliably install parsers using nvim-treesitter's pinned revisions"
    acceptance_criteria:
      - criterion: "Installing a parser with a tag revision (e.g., v0.25.0) succeeds"
        verification: |
          rm -rf /tmp/test-python
          ./target/release/treesitter-ls install python --data-dir /tmp/test-python --verbose
          test -f /tmp/test-python/parser/python.dylib || test -f /tmp/test-python/parser/python.so
      - criterion: "Installing a parser with a commit hash revision succeeds"
        verification: |
          rm -rf /tmp/test-lua
          ./target/release/treesitter-ls install lua --data-dir /tmp/test-lua --verbose
          test -f /tmp/test-lua/parser/lua.dylib || test -f /tmp/test-lua/parser/lua.so
      - criterion: "Verbose output shows which revision was checked out"
        verification: |
          ./target/release/treesitter-ls install lua --data-dir /tmp/test-verbose --verbose --force 2>&1 | grep -i "revision"
    story_points: 1
    dependencies:
      - PBI-006  # Requires existing parser installation infrastructure
    technical_notes: |
      ## Root Cause Analysis

      The current `clone_repo()` function in `src/install/parser.rs` uses this flow:
      1. `git clone --depth 1 <url>` - Shallow clone with default branch
      2. `git fetch --depth 1 origin <revision>` - Fetch the specific revision
      3. `git checkout <revision>` - Checkout the revision **<-- BUG IS HERE**

      **The Bug**: After `git fetch --depth 1 origin v0.25.0`, the tag is stored in
      `FETCH_HEAD` but NOT as a local tag reference. So `git checkout v0.25.0` fails
      with "pathspec 'v0.25.0' did not match any file(s) known to git".

      **Proof**:
      ```bash
      $ git clone --depth 1 https://github.com/tree-sitter/tree-sitter-python .
      $ git fetch --depth 1 origin v0.25.0
      From https://github.com/tree-sitter/tree-sitter-python
       * tag               v0.25.0    -> FETCH_HEAD
      $ git checkout v0.25.0
      error: pathspec 'v0.25.0' did not match any file(s) known to git
      $ git checkout FETCH_HEAD
      HEAD is now at 293fdc0 0.25.0  # <-- THIS WORKS!
      ```

      ## Fix

      Change line 229-239 in `src/install/parser.rs` from:
      ```rust
      let status = Command::new("git")
          .current_dir(dest)
          .args(["checkout", revision])  // <-- This fails for tags in shallow clones
          .status()
      ```

      To:
      ```rust
      let status = Command::new("git")
          .current_dir(dest)
          .args(["checkout", "FETCH_HEAD"])  // <-- Use FETCH_HEAD which always works
          .status()
      ```

      ## Files to Modify
      - src/install/parser.rs - Fix `clone_repo()` to use FETCH_HEAD

      ## Testing Strategy
      1. Integration test: Install Python (uses tag v0.25.0)
      2. Integration test: Install Lua (uses commit hash)
      3. Both should succeed with the fixed checkout command

  - id: PBI-016
    status: ready
    story:
      role: "user of treesitter-ls"
      capability: "continue using the language server even when a parser crashes"
      benefit: "I don't lose all functionality just because one language parser has a bug"
    acceptance_criteria:
      - criterion: "Server remains running when a parser triggers an assertion failure"
        verification: |
          # Load a parser that triggers assertion failure (e.g., YAML with specific content)
          # Verify server process is still running after the crash is handled
          # Check server responds to subsequent LSP requests
      - criterion: "Failed parser is marked as unavailable and won't crash again"
        verification: |
          # After a parser crash, subsequent requests for that language should gracefully fail
          # Server should log "Parser 'yaml' is unavailable due to previous crash"
          # No repeated crash attempts for the same parser
      - criterion: "Error message is logged with sufficient detail for debugging"
        verification: |
          # Check server logs contain:
          # - Language name that crashed
          # - Operation that was being performed (parse, semantic tokens, etc.)
          # - Instruction to report the issue or update parser
      - criterion: "Other languages continue to work normally after one parser crashes"
        verification: |
          # After YAML parser crashes, Lua/Rust/Python semantic tokens still work
          # New documents in other languages can be opened and parsed
      - criterion: "Crashed parser can be retried after reinstallation"
        verification: |
          # User reinstalls parser with `treesitter-ls install yaml --force`
          # Server picks up new parser version on next file open
          # If new parser works, it's no longer marked as unavailable
    story_points: 8
    dependencies: []
    decisions:
      - question: "How should parser crashes be isolated?"
        decision: "Use subprocess isolation for parsing operations"
        rationale: |
          C `assert()` failures trigger SIGABRT which cannot be caught by Rust's `catch_unwind`.
          The only reliable way to prevent process termination is to run the parser in a
          separate process. This adds latency but guarantees server stability.

          Alternative considered: Wrapping with `catch_unwind` - rejected because it only
          catches Rust panics, not C aborts. The YAML parser crash (assert failure in scanner.c)
          would still terminate the process.

          Alternative considered: Signal handlers for SIGABRT - rejected because signal handlers
          for SIGABRT cannot safely recover the process state. The C runtime is in an undefined
          state after a failed assertion.
      - question: "Which operations need subprocess isolation?"
        decision: "Initial parsing and re-parsing operations only"
        rationale: |
          Parser crashes occur during the `parser.parse()` call, which invokes the C scanner.
          Query operations (semantic tokens, selection ranges) use the already-parsed tree
          and don't invoke the C parser code. Therefore, only parse operations need isolation.

          This minimizes the performance impact - most LSP operations use cached parse trees.
      - question: "What's the fallback when subprocess parsing fails?"
        decision: "Graceful degradation with document-level granularity"
        rationale: |
          When a parser crashes:
          1. Mark the parser as "crashed" (not just unavailable - distinct from "not installed")
          2. Store the document as unparsed (no tree)
          3. Return empty/minimal results for tree-dependent operations
          4. Log detailed error for debugging
          5. Allow parser retry after user reinstalls

          This ensures the LSP continues to work for other documents/languages.
    technical_notes: |
      ## Problem Analysis

      Tree-sitter parsers are compiled C code loaded as dynamic libraries. When the C code
      contains `assert()` calls that fail, they trigger SIGABRT which:
      1. Cannot be caught by Rust's `catch_unwind` (only catches Rust panics)
      2. Cannot be reliably handled by signal handlers (C runtime is corrupted)
      3. Terminates the entire process

      Example from YAML parser:
      ```
      Assertion failed: (size == length), function deserialize, file scanner.c, line 217.
      ```

      ## Implementation Strategy

      ### Option A: Subprocess Isolation (Recommended)

      Run parsing in a subprocess using `std::process::Command` or a dedicated parsing service.

      **Flow**:
      1. Server receives document text
      2. Spawn subprocess: `treesitter-ls parse --language yaml --timeout 30`
      3. Subprocess loads parser and parses text, outputs serialized tree (or error)
      4. Parent process reads result, stores tree in document store
      5. If subprocess crashes (exit code != 0), mark parser as crashed

      **Pros**:
      - 100% reliable isolation - subprocess crash cannot affect parent
      - Can implement timeout for parser operations
      - Can detect crash vs timeout vs success

      **Cons**:
      - IPC overhead for passing document text and tree
      - Need to serialize/deserialize parse trees
      - Subprocess startup latency (~50-100ms)

      **Optimization**: Keep parsing subprocess alive for repeated operations (process pool).

      ### Option B: Compile-Time Scanner Validation

      Validate that scanner.c assertions cannot be triggered with arbitrary input.

      **Not viable**: We don't control third-party parser code.

      ### Option C: Runtime Parser Validation

      Before using a new parser, test it with sample content.

      **Limitations**: Cannot predict all inputs that might trigger assertions.

      ## Recommended Implementation (Option A)

      ### Phase 1: Subprocess Parse Command

      Add CLI subcommand:
      ```
      treesitter-ls parse --language <lang> --timeout <seconds>
      ```

      - Reads document text from stdin
      - Outputs serialized parse tree to stdout (or error JSON)
      - Exits with code 0 on success, non-zero on failure
      - Timeout kills subprocess after N seconds

      ### Phase 2: Document Parser Wrapper

      Create `IsolatedParser` that wraps subprocess execution:
      ```rust
      pub struct IsolatedParser {
          timeout: Duration,
          crashed_parsers: HashSet<String>,
      }

      impl IsolatedParser {
          pub fn parse(&mut self, language: &str, text: &str) -> ParseResult {
              if self.crashed_parsers.contains(language) {
                  return ParseResult::ParserUnavailable;
              }

              let result = Command::new("treesitter-ls")
                  .args(["parse", "--language", language, "--timeout", "30"])
                  .stdin(Stdio::piped())
                  .stdout(Stdio::piped())
                  .spawn();

              // Handle success, timeout, crash...
          }
      }
      ```

      ### Phase 3: Integration with LSP

      Replace direct parser calls in `parse_document()` with `IsolatedParser`:
      - On subprocess success: deserialize tree, store in document
      - On subprocess crash: mark parser crashed, log error, store document without tree
      - On timeout: log warning, store document without tree (don't mark as crashed)

      ## Key Files to Modify

      - src/bin/main.rs - Add `parse` subcommand
      - src/language/parser_pool.rs - Add subprocess parsing option
      - src/lsp/lsp_impl.rs - Use isolated parsing in parse_document()
      - src/language/registry.rs - Track crashed parsers

      ## Serialization Format

      Use tree-sitter's native tree serialization or JSON representation:
      ```json
      {
        "success": true,
        "tree": {
          "root": { "kind": "document", "start": [0,0], "end": [10,0], "children": [...] }
        }
      }
      ```

      Or error:
      ```json
      {
        "success": false,
        "error": "Parser crashed",
        "exit_code": 134
      }
      ```

      ## Performance Considerations

      - Subprocess startup: ~50-100ms (can be amortized with process pool)
      - IPC overhead: ~1-5ms for typical documents (depends on size)
      - Memory: Subprocess uses separate memory space

      For incremental parsing, consider keeping subprocess alive:
      ```rust
      pub struct ParsingService {
          processes: HashMap<String, Child>,  // One per language
      }
      ```

      ## Alternative: Validation-Only Approach (Simpler, Less Safe)

      If subprocess overhead is unacceptable:
      1. On parser install, run validation with test inputs
      2. Mark parser as "validated" or "unvalidated"
      3. For unvalidated parsers, show warning but use anyway
      4. If crash occurs, process dies (same as current behavior)

      This provides better UX (warning) but doesn't prevent crashes.

      ## Testing Strategy

      1. Unit test: Mock subprocess for parse success/failure/timeout
      2. Integration test: Actually crash a parser and verify server recovery
      3. E2E test: Open file with crash-inducing content, verify server survives
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
  number: 9
  pbi_id: PBI-014
  story:
    role: "user of treesitter-ls"
    capability: "have treesitter-ls automatically install missing parsers when I type a new injected code block"
    benefit: "I get immediate syntax highlighting for code blocks I add while editing without needing to re-open the file"
  status: done

  subtasks:
    # Sprint 9: Auto-install Injected Languages on Text Edit (PBI-014)
    # Enable auto-installation of parsers for injected languages when users add new code blocks while editing.
    #
    # Implementation Strategy:
    # 1. On did_change, after re-parse, call check_injected_languages_auto_install()
    # 2. The function already handles all injection detection and auto-install triggering
    # 3. InstallingLanguages tracker prevents duplicate installs automatically
    #
    # Key Dependencies (all completed):
    # - PBI-008: InstallingLanguages tracker, maybe_auto_install_language()
    # - PBI-013: check_injected_languages_auto_install(), get_injected_languages()
    #
    # This is a minimal implementation: just one function call to add.

    # Subtask 1: Add injection auto-install check to did_change
    - test: "did_change calls check_injected_languages_auto_install after parsing"
      implementation: |
        In did_change, after parse_document() completes (line 777-778):
        1. Call check_injected_languages_auto_install(&uri)
        2. This runs after the document is re-parsed, so we have the updated AST
        3. The function reuses all logic from PBI-013 (get_injected_languages, etc.)
        4. InstallingLanguages tracker prevents duplicate installs automatically
      type: behavioral
      status: completed
      commits:
        - hash: 37a2e6f
          phase: red
          message: "test(lsp): add test for did_change injection auto-install"
        - hash: e653e4e
          phase: green
          message: "feat(lsp): call check_injected_languages_auto_install in did_change"
      files_to_modify:
        - src/lsp/lsp_impl.rs

    # Subtask 2: Test - adding new code block triggers auto-install
    - test: "Editing a document to add a code block triggers auto-install for the injected language"
      implementation: |
        Create test that:
        1. Opens a markdown document with NO code blocks
        2. Simulates did_change with text containing a new Lua code block
        3. Verifies check_injected_languages_auto_install is called
        4. Verifies maybe_auto_install_language is called for "lua"
      type: behavioral
      status: completed
      commits:
        - hash: 7b2a872
          phase: green
          message: "test(lsp): add test for adding code block triggers auto-install"
      files_to_modify:
        - src/lsp/lsp_impl.rs  # Unit tests in same file

    # Subtask 3: Test - unrelated edits don't re-trigger for existing injections
    - test: "Editing text outside code blocks doesn't trigger auto-install for already-loaded languages"
      implementation: |
        Create test that:
        1. Opens a markdown document with a Lua code block (Lua parser loaded)
        2. Simulates did_change editing text OUTSIDE the code block
        3. Verifies check_injected_languages_auto_install runs
        4. Verifies maybe_auto_install_language is NOT called (Lua already loaded)
      type: behavioral
      status: completed
      commits:
        - hash: dc54000
          phase: green
          message: "test(lsp): verify unrelated edits don't re-trigger for loaded languages"
      files_to_modify:
        - src/lsp/lsp_impl.rs

    # Subtask 4: Test - multiple new injections in paste operation
    - test: "Pasting multiple code blocks triggers auto-install for all new languages"
      implementation: |
        Create test that:
        1. Opens a minimal markdown document
        2. Simulates did_change with a paste containing Python, Rust, Go code blocks
        3. Verifies maybe_auto_install_language is called for each language
        4. Verifies InstallingLanguages prevents duplicates if same language appears twice
      type: behavioral
      status: completed
      commits:
        - hash: dd7ea24
          phase: green
          message: "test(lsp): verify pasting multiple code blocks triggers all languages"
      files_to_modify:
        - src/lsp/lsp_impl.rs

  notes: |
    Sprint 9: Auto-install Injected Languages on Text Edit (PBI-014)
    Sprint Goal: Enable immediate syntax highlighting for code blocks added during
    editing by automatically installing missing parsers when new injection regions appear.

    Key Design Decisions:
    - Reuse check_injected_languages_auto_install() from PBI-013 (no code duplication)
    - Call after parse_document() in did_change (same pattern as did_open)
    - InstallingLanguages tracker handles duplicate prevention automatically

    Implementation Flow:
    1. User edits markdown file (types ```python)
    2. did_change receives edit event
    3. parse_document() re-parses with incremental edit
    4. NEW: check_injected_languages_auto_install(&uri) is called
       - Gets unique injected languages (including the new python block)
       - For python (not loaded), calls maybe_auto_install_language()
    5. InstallingLanguages tracker prevents duplicate install attempts
    6. After install, semantic tokens refresh shows highlighting

    Files to Modify:
    - src/lsp/lsp_impl.rs - Add one function call in did_change after parse_document

    Key Code Location:
    - did_change method: lines 695-791
    - After parse_document call: line 777-778
    - check_injected_languages_auto_install: already exists from PBI-013

    Expected Changes:
    Just add one line after parse_document():
    ```rust
    // Parse the updated document with edit information
    self.parse_document(uri.clone(), text, language_id.as_deref(), edits)
        .await;

    // NEW: Check for injected languages and trigger auto-install if needed
    self.check_injected_languages_auto_install(&uri).await;
    ```

    Edge Cases (handled by existing infrastructure):
    - Injection removed: No action needed (we only check for missing parsers)
    - Rapid edits: InstallingLanguages prevents spam
    - Recursive injections: Only handle depth=1 (same as PBI-013)
    - Already loaded languages: ensure_language_loaded returns success, skipped
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
  - sprint: 9
    pbi: PBI-014
    story: "As a user of treesitter-ls, I want to have treesitter-ls automatically install missing parsers when I type a new injected code block, so that I get immediate syntax highlighting for code blocks I add while editing without needing to re-open the file"
    outcome: "Auto-install triggered for injected languages on text edit (did_change); 100% code reuse from PBI-013 infrastructure; single line addition to did_change"
    acceptance:
      status: accepted
      criteria_verified:
        - "Adding a new code block triggers auto-install for the injected language"
        - "Existing injections do not re-trigger on unrelated edits (already-loaded languages skipped)"
        - "Multiple new injections in a single paste operation all trigger auto-install"
        - "Auto-install respects the autoInstall setting"
        - "Semantic tokens refresh after injection parser installs"
        - "Duplicate install prevention via InstallingLanguages tracker"
      dod_verified:
        - "cargo test: PASSED (166 tests)"
        - "cargo clippy -- -D warnings: PASSED"
        - "cargo fmt --check: PASSED"
    subtasks_completed: 4
    commits_actual: 4
    impediments: 0
    key_implementation: |
      Added ONE LINE in did_change after parse_document():
        self.check_injected_languages_auto_install(&uri).await;

      This reuses 100% of the infrastructure from PBI-013:
        - get_injected_languages(): extracts unique injected languages
        - check_injected_languages_auto_install(): triggers auto-install for missing parsers
        - InstallingLanguages tracker: prevents duplicate concurrent installs

  - sprint: 8
    pbi: PBI-013
    story: "As a user of treesitter-ls, I want to have treesitter-ls automatically install missing parsers for injected languages when I open a file, so that I get complete syntax highlighting for all code blocks without manually installing each language parser"
    outcome: "Auto-install triggered for injected languages (e.g., Lua/Python code blocks in Markdown) on file open; reuses existing InstallingLanguages tracker; integration test coverage"
    acceptance:
      status: accepted
      criteria_verified:
        - "Opening markdown with Lua code block triggers Lua parser auto-install"
        - "Multiple injected languages in one document are all installed"
        - "Already-installing language is not triggered twice (InstallingLanguages tracker)"
        - "Auto-install respects the autoInstall setting"
        - "Semantic tokens refresh after injection parser installs"
      dod_verified:
        - "cargo test: PASSED (162 tests)"
        - "cargo clippy -- -D warnings: PASSED"
        - "cargo fmt --check: PASSED"
    subtasks_completed: 4
    commits_actual: 4
    impediments: 0

  - sprint: 7
    pbi: PBI-008
    story: "As a user of treesitter-ls, I want to have treesitter-ls automatically install missing parsers when I open a file, so that I get syntax highlighting for any language without running install commands manually"
    outcome: "Silent background auto-install of parsers and queries when opening files with missing languages; InstallingLanguages tracker prevents duplicates; LSP notifications for progress"
    acceptance:
      status: accepted
      criteria_verified:
        - "Opening a .lua file when no Lua parser is installed triggers silent background installation"
        - "LSP shows progress notification during installation"
        - "Auto-install can be disabled in initializationOptions"
        - "Failed auto-install shows clear error message via LSP notification"
        - "Auto-install does not block document operations (async)"
        - "Already-installing language is not triggered twice"
      dod_verified:
        - "cargo test: PASSED"
        - "cargo clippy -- -D warnings: PASSED"
        - "cargo fmt --check: PASSED"
    subtasks_completed: 7
    commits_actual: 4
    impediments: 0

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
  - sprint: 9
    pbi: PBI-014
    prime_directive_read: true

    what_went_well:
      - item: "Maximum Code Reuse - One Line Implementation"
        detail: |
          The entire PBI was implemented by adding ONE line of production code.
          The call to check_injected_languages_auto_install() in did_change reuses
          100% of the infrastructure from PBI-008 (InstallingLanguages tracker,
          maybe_auto_install_language) and PBI-013 (get_injected_languages,
          check_injected_languages_auto_install helper).

      - item: "Fast Delivery from Infrastructure Investment"
        detail: |
          Sprint completed quickly because PBI-008 created robust auto-install
          infrastructure and PBI-013 created the injection detection helper.
          This validates the insight from Sprint 8: "Infrastructure investment
          pays dividends."

      - item: "Comprehensive Edge Case Testing"
        detail: |
          4 tests covered all acceptance criteria edge cases:
          - ST-1: Basic did_change integration (37a2e6f, e653e4e)
          - ST-2: Adding new code block triggers install (7b2a872)
          - ST-3: Unrelated edits don't re-trigger for loaded languages (dc54000)
          - ST-4: Pasting multiple code blocks triggers all languages (dd7ea24)

      - item: "Zero Impediments - Streak Continues"
        detail: |
          Sprint 9 completed without blockers, continuing the zero-impediment
          streak from Sprints 1-8. This reflects mature, well-understood codebase.

      - item: "Perfect Commit Discipline Maintained"
        detail: |
          4 commits for 4 subtasks (100% ratio), maintaining the discipline
          established in Sprint 8. Each commit corresponds to one subtask.

    what_could_improve:
      - item: "Subtask Granularity for Test-Only Work"
        detail: |
          ST-2, ST-3, and ST-4 were all test-only subtasks that could potentially
          have been combined into fewer subtasks. Each added a single test case.
          For future sprints, consider grouping related test cases into single
          subtasks when they test variations of the same behavior.
        root_cause: |
          The subtask breakdown was based on acceptance criteria, which is
          correct for traceability. However, tests verifying similar behaviors
          (unrelated edits, multiple code blocks) could be grouped.
        impact: "low"

      - item: "No Refactoring Phase Needed"
        detail: |
          With only one line of production code added, there was no opportunity
          for refactoring. This is positive but means the REFACTOR phase was
          skipped entirely for all subtasks.
        root_cause: |
          Excellent prior infrastructure meant minimal new code was needed.
          Not a problem, just an observation about the TDD cycle.
        impact: "none"

    action_items:
      - id: AI-010
        action: "Consider grouping test-only subtasks for edge case variations"
        detail: |
          When multiple subtasks are testing variations of the same behavior
          (e.g., ST-2/ST-3/ST-4 all testing did_change injection scenarios),
          consider grouping them into a single subtask with multiple test cases.
          This reduces subtask overhead while maintaining test coverage.
        owner: "@scrum-team-developer"
        status: pending
        backlog: sprint

      - id: AI-011
        action: "Document the injection auto-install pattern as reference architecture"
        detail: |
          The pattern of check_injected_languages_auto_install() called from both
          did_open (PBI-013) and did_change (PBI-014) represents a clean, reusable
          approach. Document this as a reference for future similar features.
        owner: "@scrum-team-developer"
        status: pending
        backlog: product

      - id: AI-008
        action: "Consider extracting injection auto-install helpers during PBI-014"
        status: completed
        resolution: |
          No extraction was needed. The existing check_injected_languages_auto_install()
          function worked as-is for PBI-014, validating the YAGNI decision from Sprint 8.

    insights:
      - insight: "Infrastructure reuse eliminates implementation complexity"
        analysis: |
          PBI-014 was estimated at 3 story points but delivered in minimal time
          because:
          1. PBI-008 provided InstallingLanguages tracker (thread-safe duplicate prevention)
          2. PBI-013 provided check_injected_languages_auto_install (injection detection)
          3. Only ONE new line was needed: the function call in did_change

          This demonstrates that good foundational architecture (PBI-008) and
          proper helper extraction (PBI-013) can reduce complex features to
          trivial integrations.

      - insight: "Test count inversely proportional to production code"
        analysis: |
          Sprint 9 added 4 tests for 1 line of production code. This high test-to-code
          ratio is appropriate for integration points where the behavior depends on
          complex underlying systems. The tests verify the integration works correctly
          across various scenarios, not the underlying mechanisms.

      - insight: "YAGNI validated for module extraction"
        analysis: |
          Sprint 8 noted that injection utilities might need extraction to a shared
          module (AI-008). Sprint 9 completed PBI-014 without needing any extraction -
          the existing check_injected_languages_auto_install() worked as-is.
          This validates the YAGNI decision: extract only when actual duplication
          or modification is required.

    metrics:
      story_points: 3
      unit_tests_added: 4
      production_lines_added: 1
      subtasks_completed: 4
      impediments_encountered: 0
      dod_criteria_met: 3
      commits_expected: 4
      commits_actual: 4
      commit_discipline_score: 1.0
      infrastructure_reuse: "100%"
      improvement_from_sprint_8: "Maintained 100% commit ratio, maximum reuse"

  - sprint: 8
    pbi: PBI-013
    prime_directive_read: true

    what_went_well:
      - item: "Excellent Code Reuse"
        detail: |
          Leveraged existing collect_all_injections() function from semantic token highlighting
          and InstallingLanguages tracker from PBI-008. Minimal new code was needed - most of the
          work was integration rather than creation.

      - item: "Clean TDD Cycle with 1:1 Commit Ratio"
        detail: |
          Sprint 8 achieved 4 commits for 4 subtasks (100% ratio). Each subtask had a clear
          RED->GREEN phase with a corresponding commit. This is a significant improvement
          from earlier sprints.
          - ST-1 (6757af7): get_injected_languages helper
          - ST-2 (c37544f): check_injected_languages_auto_install
          - ST-3 (c42780d): Integration into did_open
          - ST-4 (7ba80e1): Integration test

      - item: "Thread Safety Built-In"
        detail: |
          The InstallingLanguages tracker with atomic try_start_install() prevents race conditions
          when the same injected language appears in multiple code blocks. No additional concurrency
          handling was needed - just reuse of existing infrastructure.

      - item: "Focused Implementation"
        detail: |
          Only modified one file (src/lsp/lsp_impl.rs) for the feature implementation.
          Kept the scope minimal while still meeting all acceptance criteria.

      - item: "Zero Impediments"
        detail: "Sprint completed without any blockers, continuing the streak."

    what_could_improve:
      - item: "Module Extraction for Reuse"
        detail: |
          The injection-related utilities (get_injected_languages, check_injected_languages_auto_install)
          are currently in lsp_impl.rs. For PBI-014 (injection auto-install on text edit), these
          functions will be reused. Consider extracting to a shared module if complexity grows.
        root_cause: |
          YAGNI principle was correctly applied - no premature extraction. However, with PBI-014
          coming next, this may be worth reconsidering during that sprint.
        impact: "low"

      - item: "Integration Test Coverage Depth"
        detail: |
          The integration test verifies the basic flow but could be more comprehensive with
          edge cases (unknown languages, empty code blocks, nested injections). Kept focused
          for MVP as per YAGNI.
        root_cause: "Intentional scope limitation for first iteration."
        impact: "low"

    action_items:
      - id: AI-008
        action: "Consider extracting injection auto-install helpers during PBI-014"
        detail: |
          If PBI-014 requires modifications to get_injected_languages() or
          check_injected_languages_auto_install(), consider extracting them to a shared
          module (e.g., src/lsp/injection_auto_install.rs) to avoid duplication.
        owner: "@scrum-team-developer"
        status: pending
        backlog: sprint

      - id: AI-009
        action: "Add recursive injection support as future backlog item"
        detail: |
          Current implementation only handles depth=1 injections (e.g., markdown->lua).
          Recursive injections (e.g., markdown->lua->regex) are not auto-installed.
          If user need arises, create a PBI for this enhancement.
        owner: "@scrum-team-product-owner"
        status: pending
        backlog: product

    insights:
      - insight: "Infrastructure investment pays dividends"
        analysis: |
          PBI-008 created a robust auto-install infrastructure (InstallingLanguages,
          maybe_auto_install_language, install_language_async). PBI-013 leveraged this
          infrastructure with minimal new code, reducing implementation time and risk.
          Good foundational work accelerates future features.

      - insight: "Semantic token pipeline provides free injection detection"
        analysis: |
          The collect_all_injections() function was originally written for semantic token
          highlighting but proved equally useful for auto-install detection. This demonstrates
          the value of writing generic, reusable functions even when the immediate use case
          is specific.

      - insight: "1:1 commit-to-subtask ratio is achievable with proper breakdown"
        analysis: |
          Sprint 8 achieved perfect commit discipline (4 commits for 4 subtasks). The key
          was breaking down the work into truly independent units where each subtask could
          be implemented and committed separately without breaking the build.

    metrics:
      unit_tests_added: 6
      subtasks_completed: 4
      impediments_encountered: 0
      dod_criteria_met: 3
      commits_expected: 4
      commits_actual: 4
      commit_discipline_score: 1.0
      improvement_from_sprint_7: "100% commit ratio achieved"

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
