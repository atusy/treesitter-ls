// ============================================================
// Dashboard Data (AI edits this section)
// ============================================================

const userStoryRoles = [
  "Rustacean editing Markdown",
  "developer editing Lua files",
] as const satisfies readonly string[]; // Must have at least one role. Avoid generic roles like "user" or "admin". Remove obsolete roles freely.

const scrum: ScrumDashboard = {
  product_goal: {
    statement:
      "Extend LSP bridge with additional request types and fix E2E test gaps",
    success_metrics: [
      {
        metric: "ADR-0006 Phase 2 partial",
        target: "Crash recovery implemented (SATISFIED via get_or_spawn)",
      },
      {
        metric: "ADR-0008 Strategy 2 expansion",
        target: "signatureHelp delegation working in injection regions",
      },
      {
        metric: "E2E test reliability",
        target: "Standalone Lua files return semantic tokens and selection ranges",
      },
    ],
  },

  // Completed PBIs: PBI-001 through PBI-093 | History: git log -- scrum.yaml, scrum.ts
  // PBI-091 (idle cleanup): Infrastructure - already implemented, needs wiring (low priority)
  // PBI-093 (crash recovery): Already implemented in ServerPool.get_or_spawn() - DONE
  product_backlog: [
    {
      id: "PBI-090",
      story: {
        role: "developer editing Lua files",
        capability: "see syntax highlighting for standalone .lua files",
        benefit: "code is readable with proper keyword/string/comment colors",
      },
      acceptance_criteria: [
        {
          criterion: "Semantic tokens returned for .lua files",
          verification: "make test_nvim_file FILE=tests/test_lsp_semantic.lua",
        },
        {
          criterion: "Selection ranges work for .lua files",
          verification: "make test_nvim_file FILE=tests/test_lsp_select.lua",
        },
      ],
      status: "ready",
    },
    {
      id: "PBI-092",
      story: {
        role: "Rustacean editing Markdown",
        capability: "see function signatures when typing arguments in Rust code blocks",
        benefit: "know parameter types without leaving the editor",
      },
      acceptance_criteria: [
        {
          criterion: "signatureHelp delegated to rust-analyzer",
          verification: "cargo test --test test_signature_help (new E2E test)",
        },
      ],
      status: "ready",
    },
  ],

  sprint: {
    number: 71,
    pbi_id: "PBI-090",
    goal: "Developer can see syntax highlighting for standalone Lua files",
    status: "planning",
    subtasks: [
      {
        test: "minimal_init.lua configures searchPaths pointing to deps/treesitter",
        implementation: "Add initializationOptions with searchPaths to LSP config",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [
          "Root cause: LSP lacks searchPaths → ensure_language_loaded fails → no queries",
        ],
      },
      {
        test: "E2E: test_lsp_semantic.lua passes for example.lua",
        implementation: "Verify semantic tokens returned for standalone Lua files",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [],
      },
      {
        test: "E2E: test_lsp_select.lua passes for example.lua",
        implementation: "Verify selection ranges work for standalone Lua files",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [],
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

  // Historical sprints (recent 3) | Sprint 1-67: git log -- scrum.yaml, scrum.ts
  completed: [
    {
      number: 70,
      pbi_id: "PBI-089",
      goal:
        "Users see type info when hovering over Rust symbols in Markdown code blocks",
      status: "done",
      subtasks: [],
    },
    {
      number: 69,
      pbi_id: "PBI-087",
      goal: "ServerPool for connection reuse (<200ms latency)",
      status: "done",
      subtasks: [],
    },
    {
      number: 68,
      pbi_id: "PBI-086",
      goal: "Go-to-definition in Markdown Rust code blocks",
      status: "done",
      subtasks: [],
    },
  ],

  // Recent 3 retrospectives | Sprint 1-67: git log -- scrum.yaml, scrum.ts
  retrospectives: [
    {
      sprint: 70,
      improvements: [
        {
          action: "Pre-existing E2E failures need documentation",
          timing: "sprint",
          status: "completed",
          outcome:
            "Root cause: standalone .lua files have no parser/queries (PBI-090); hover test needs rust-analyzer in CI",
        },
        {
          action:
            "Fix 4 failing E2E tests: Lua standalone (select x2, semantic x1), hover test env",
          timing: "product",
          status: "completed",
          outcome: "Created PBI-090 for Lua support; hover test is valid but needs CI setup",
        },
      ],
    },
    {
      sprint: 69,
      improvements: [
        {
          action: "cleanup_idle() needs timer wiring",
          timing: "product",
          status: "completed",
          outcome: "Created PBI-091 for idle cleanup",
        },
        {
          action: "ServerPool not yet in lsp_impl.rs",
          timing: "sprint",
          status: "completed",
          outcome: "ServerPool integrated in Sprint 70",
        },
      ],
    },
    {
      sprint: 68,
      improvements: [
        {
          action: "PoC sync subprocess needs production refactor",
          timing: "product",
          status: "completed",
          outcome: "ServerPool with connection reuse implemented in Sprint 69",
        },
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
