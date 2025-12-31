// ============================================================
// Dashboard Data (AI edits this section)
// ============================================================

const userStoryRoles = [
  "Rustacean editing Markdown",
  "developer editing Lua files",
  "documentation author with Rust code blocks",
] as const satisfies readonly string[]; // Must have at least one role. Avoid generic roles like "user" or "admin". Remove obsolete roles freely.

const scrum: ScrumDashboard = {
  product_goal: {
    statement:
      "Expand LSP bridge to support most language server features indirectly through bridging (ADR-0006, 0007, 0008)",
    success_metrics: [
      {
        metric: "Bridge coverage",
        target:
          "Support completion, signatureHelp, references, rename, codeAction, formatting",
      },
      {
        metric: "Modular architecture",
        target: "Bridge module split into per-feature files for maintainability",
      },
      {
        metric: "E2E test coverage",
        target: "Each bridged feature has E2E test verifying end-to-end flow",
      },
    ],
  },

  // Completed PBIs: PBI-001 through PBI-121 | History: git log -- scrum.yaml, scrum.ts
  // PBI-091 (idle cleanup): Infrastructure - already implemented, needs wiring (low priority)
  // PBI-107 (remove WorkspaceType): Deferred - rust-analyzer linkedProjects too slow
  // PBI-120: Done in e600402 - bridge filter map with enabled flag (docs in docs/README.md)
  product_backlog: [
    {
      id: "PBI-122",
      story: {
        role: "developer editing Lua files",
        capability: "run E2E tests in CI without requiring Neovim for parser installation",
        benefit: "CI workflow is self-contained and does not depend on external tools for core functionality",
      },
      acceptance_criteria: [
        {
          criterion: "deps/treesitter target uses treesitter-ls CLI instead of Neovim",
          verification: "grep -q 'treesitter-ls language install' Makefile && ! grep -q 'nvim.*nvim-treesitter.*install' Makefile",
        },
        {
          criterion: "deps/treesitter no longer depends on deps/nvim/nvim-treesitter",
          verification: "make -n deps/treesitter 2>&1 | grep -v nvim-treesitter",
        },
        {
          criterion: "Self-bootstrapping: cargo build runs before language install commands",
          verification: "make -n deps/treesitter 2>&1 | head -5 | grep -q 'cargo build'",
        },
        {
          criterion: "All required languages are installed (lua, luadoc, rust, markdown, markdown_inline, yaml, r)",
          verification: "make deps/treesitter && ls deps/treesitter/parser/ | grep -E '(lua|luadoc|rust|markdown|markdown_inline|yaml|r)' | wc -l | grep -q 7",
        },
        {
          criterion: "E2E tests pass with new deps/treesitter target",
          verification: "make test_nvim",
        },
      ],
      status: "done",
    },
  ],

  sprint: {
    number: 99,
    pbi_id: "PBI-122",
    goal: "Self-bootstrapping CI with treesitter-ls CLI for parser installation",
    status: "done",
    subtasks: [
      {
        test: "Verify treesitter-ls CLI 'language install' command works for single language (lua)",
        implementation: "Run 'cargo run -- language install lua --data-dir /tmp/test-parsers' and verify parser is installed",
        type: "behavioral",
        status: "completed",
        commits: [],
        notes: ["Manual verification of CLI functionality before Makefile changes", "Verified: lua.dylib and queries installed successfully"],
      },
      {
        test: "Verify treesitter-ls CLI installs all required languages",
        implementation: "Run 'language install' for all 7 languages (lua, luadoc, rust, markdown, markdown_inline, yaml, r)",
        type: "behavioral",
        status: "completed",
        commits: [],
        notes: ["Test each language installation individually to catch any failures", "Verified: all 7 parsers (.dylib) installed successfully"],
      },
      {
        test: "Modify Makefile deps/treesitter target to use treesitter-ls CLI",
        implementation: "Replace nvim-based installation with 'target/debug/treesitter-ls language install' commands",
        type: "behavioral",
        status: "completed",
        commits: [],
        notes: ["Remove deps/nvim/nvim-treesitter dependency; add build-debug dependency", "Makefile updated: deps/treesitter now depends on build-debug and uses CLI"],
      },
      {
        test: "Verify deps/treesitter target works with clean state",
        implementation: "Run 'rm -rf deps/treesitter && make deps/treesitter' and verify all parsers installed",
        type: "behavioral",
        status: "completed",
        commits: [],
        notes: ["Acceptance criteria: ls deps/treesitter/parser/ shows all 7 language parsers", "Verified: all 7 .dylib files present in deps/treesitter/parser/"],
      },
      {
        test: "Verify E2E tests pass with new deps/treesitter target",
        implementation: "Run 'make test_nvim' and ensure all tests pass",
        type: "behavioral",
        status: "completed",
        commits: [],
        notes: ["Final validation that self-bootstrapping works end-to-end", "All 29 E2E tests pass"],
      },
    ],
  },

  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_nvim" },
    ],
  },

  // Historical sprints (recent 2) | Sprint 1-96: git log -- scrum.yaml, scrum.ts
  completed: [
    { number: 98, pbi_id: "PBI-121", goal: "Refactor lsp_impl.rs into modular file structure", status: "done", subtasks: [] },
    { number: 97, pbi_id: "PBI-120", goal: "Bridge filter map with enabled flag", status: "cancelled", subtasks: [] },
  ],

  // Recent 2 retrospectives | Sprint 1-96: git log -- scrum.yaml, scrum.ts
  retrospectives: [
    {
      sprint: 98,
      improvements: [
        { action: "Modular refactoring with *_impl delegation decomposed 3800+ line file into 10 focused text_document modules", timing: "immediate", status: "completed", outcome: "pub(crate) *_impl methods called from LanguageServer trait impl" },
        { action: "File organization by LSP category (text_document/) creates natural boundaries for future workspace/ and window/", timing: "product", status: "active", outcome: null },
      ],
    },
    {
      sprint: 96,
      improvements: [
        { action: "Schema simplification - languageServers at root level", timing: "immediate", status: "completed", outcome: "BridgeSettings wrapper removed; all E2E tests passing" },
      ],
    },
  ],
};

// ============================================================
// Type Definitions (DO NOT MODIFY - request human review for schema changes)
// ============================================================

// PBI lifecycle: draft (idea) -> refining (gathering info) -> ready (can start) -> done
type PBIStatus = "draft" | "refining" | "ready" | "done";

// Sprint lifecycle
type SprintStatus =
  | "planning"
  | "in_progress"
  | "review"
  | "done"
  | "cancelled";

// TDD cycle: pending -> red (test written) -> green (impl done) -> refactoring -> completed
type SubtaskStatus = "pending" | "red" | "green" | "refactoring" | "completed";

// behavioral = changes observable behavior, structural = refactoring only
type SubtaskType = "behavioral" | "structural";

// Commits happen only after tests pass (green/refactoring), never on red
type CommitPhase = "green" | "refactoring";

// When to execute retrospective actions:
//   immediate: Apply within Retrospective (non-production code, single logical change)
//   sprint: Add as subtask to next sprint (process improvements)
//   product: Add as new PBI to Product Backlog (feature additions)
type ImprovementTiming = "immediate" | "sprint" | "product";

type ImprovementStatus = "active" | "completed" | "abandoned";

interface SuccessMetric {
  metric: string;
  target: string;
}

interface ProductGoal {
  statement: string;
  success_metrics: SuccessMetric[];
}

interface AcceptanceCriterion {
  criterion: string;
  verification: string;
}

interface UserStory {
  role: (typeof userStoryRoles)[number];
  capability: string;
  benefit: string;
}

interface PBI {
  id: string;
  story: UserStory;
  acceptance_criteria: AcceptanceCriterion[];
  status: PBIStatus;
}

interface Commit {
  hash: string;
  message: string;
  phase: CommitPhase;
}

interface Subtask {
  test: string;
  implementation: string;
  type: SubtaskType;
  status: SubtaskStatus;
  commits: Commit[];
  notes: string[];
}

interface Sprint {
  number: number;
  pbi_id: string;
  goal: string;
  status: SprintStatus;
  subtasks: Subtask[];
}

interface DoDCheck {
  name: string;
  run: string;
}

interface DefinitionOfDone {
  checks: DoDCheck[];
}

interface Improvement {
  action: string;
  timing: ImprovementTiming;
  status: ImprovementStatus;
  outcome: string | null;
}

interface Retrospective {
  sprint: number;
  improvements: Improvement[];
}

interface ScrumDashboard {
  product_goal: ProductGoal;
  product_backlog: PBI[];
  sprint: Sprint | null;
  definition_of_done: DefinitionOfDone;
  completed: Sprint[];
  retrospectives: Retrospective[];
}

// JSON output (deno run scrum.ts | jq for queries)
console.log(JSON.stringify(scrum, null, 2));
