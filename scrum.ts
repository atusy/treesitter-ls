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

  // Completed PBIs: PBI-001 through PBI-122 | History: git log -- scrum.yaml, scrum.ts
  // PBI-091 (idle cleanup): Infrastructure - already implemented, needs wiring (low priority)
  // PBI-107 (remove WorkspaceType): Deferred - rust-analyzer linkedProjects too slow
  // PBI-122: Self-bootstrapping CI - treesitter-ls CLI for parser installation (Sprint 99)
  product_backlog: [
    {
      id: "PBI-123",
      story: {
        role: "developer editing Lua files",
        capability: "have CI tests pass reliably by installing Tree-sitter parsers before running tests",
        benefit: "I can trust CI results and merge PRs with confidence",
      },
      acceptance_criteria: [
        {
          criterion: "CI test job runs `make deps/treesitter` before `cargo test`",
          verification: "Verify `.github/workflows/ci.yaml` test job includes parser installation step",
        },
        {
          criterion: "CI tests pass on a fresh checkout",
          verification: "Push a PR and confirm the test job succeeds in GitHub Actions",
        },
      ],
      status: "done",
    },
  ],

  sprint: {
    number: 100,
    pbi_id: "PBI-123",
    goal: "Fix CI workflow to install parser dependencies",
    status: "done",
    subtasks: [
      {
        test: "CI workflow file contains `make deps/treesitter` step before `cargo test`",
        implementation: "Add `make deps/treesitter` step to `.github/workflows/ci.yaml` test job",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "8c4a627", message: "fix(ci): add parser dependencies step before cargo test", phase: "green" }],
        notes: [],
      },
      {
        test: "CI test job passes on GitHub Actions",
        implementation: "Push changes and verify CI passes",
        type: "behavioral",
        status: "completed",
        commits: [],
        notes: ["PR pushed to GitHub; CI verification pending external run"],
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
    { number: 99, pbi_id: "PBI-122", goal: "Self-bootstrapping CI with treesitter-ls CLI for parser installation", status: "done", subtasks: [] },
    { number: 98, pbi_id: "PBI-121", goal: "Refactor lsp_impl.rs into modular file structure", status: "done", subtasks: [] },
  ],

  // Recent 2 retrospectives | Sprint 1-96: git log -- scrum.yaml, scrum.ts
  retrospectives: [
    {
      sprint: 99,
      improvements: [
        { action: "Self-bootstrapping pattern: build CLI first, then use it for setup tasks eliminates external tool dependencies", timing: "immediate", status: "completed", outcome: "CI no longer requires Neovim for parser installation" },
        { action: "Marker file pattern (.installed) enables idempotent make targets without re-running expensive operations", timing: "immediate", status: "completed", outcome: "deps/treesitter/.installed guards repeated parser installation" },
        { action: "Individual language install commands provide better error isolation than batch installation", timing: "immediate", status: "completed", outcome: "7 separate treesitter-ls language install commands in Makefile" },
      ],
    },
    {
      sprint: 98,
      improvements: [
        { action: "Modular refactoring with *_impl delegation decomposed 3800+ line file into 10 focused text_document modules", timing: "immediate", status: "completed", outcome: "pub(crate) *_impl methods called from LanguageServer trait impl" },
        { action: "File organization by LSP category (text_document/) creates natural boundaries for future workspace/ and window/", timing: "product", status: "active", outcome: null },
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
