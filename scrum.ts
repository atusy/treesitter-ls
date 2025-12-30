// ============================================================
// Dashboard Data (AI edits this section)
// ============================================================

const userStoryRoles = [
  "Rustacean editing Markdown",
  "developer editing Lua files",
] as const satisfies readonly string[]; // Must have at least one role. Avoid generic roles like "user" or "admin". Remove obsolete roles freely.

const scrum: ScrumDashboard = {
  product_goal: {
    statement: "",
    success_metrics: [],
  },

  // Completed PBIs: PBI-001 through PBI-090, PBI-093 | History: git log -- scrum.yaml, scrum.ts
  // PBI-091 (idle cleanup): Infrastructure - already implemented, needs wiring (low priority)
  product_backlog: [],

  sprint: null,

  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_nvim" },
    ],
  },

  // Historical sprints (recent 2) | Sprint 1-69: git log -- scrum.yaml, scrum.ts
  completed: [
    {
      number: 71,
      pbi_id: "PBI-090",
      goal: "Developer can see syntax highlighting for standalone Lua files",
      status: "done",
      subtasks: [
        {
          test:
            "minimal_init.lua configures searchPaths pointing to deps/treesitter",
          implementation:
            "Add initializationOptions with searchPaths to LSP config",
          type: "behavioral",
          status: "completed",
          commits: [
            {
              hash: "69c14b9",
              message:
                "feat(lsp): fix race condition in semantic tokens for dynamically loaded languages",
              phase: "green",
            },
          ],
          notes: [
            "Root cause: Tower-LSP concurrent requests - semanticTokens/full arrived before didOpen completed parsing",
            "Solution: Early document registration with language_id, synchronous parsing fallback, on-demand language loading",
          ],
        },
        {
          test: "E2E: test_lsp_semantic.lua passes for example.lua",
          implementation:
            "Verify semantic tokens returned for standalone Lua files",
          type: "behavioral",
          status: "completed",
          commits: [
            {
              hash: "69c14b9",
              message:
                "feat(lsp): fix race condition in semantic tokens for dynamically loaded languages",
              phase: "green",
            },
          ],
          notes: [
            "2 tests pass: semantic tokens work for standalone .lua files",
          ],
        },
        {
          test: "E2E: test_lsp_select.lua passes for example.lua",
          implementation:
            "Verify selection ranges work for standalone Lua files",
          type: "behavioral",
          status: "completed",
          commits: [
            {
              hash: "69c14b9",
              message:
                "feat(lsp): fix race condition in semantic tokens for dynamically loaded languages",
              phase: "green",
            },
          ],
          notes: [
            "17 tests pass: selection ranges work for standalone .lua files",
          ],
        },
      ],
    },
    {
      number: 70,
      pbi_id: "PBI-089",
      goal:
        "Users see type info when hovering over Rust symbols in Markdown code blocks",
      status: "done",
      subtasks: [],
    },
  ],

  // Recent 2 retrospectives | Sprint 1-69: git log -- scrum.yaml, scrum.ts
  retrospectives: [
    {
      sprint: 71,
      improvements: [
        {
          action:
            "Document Tower-LSP concurrent request processing pattern in CLAUDE.md",
          timing: "immediate",
          status: "completed",
          outcome:
            "Added section on race conditions, solution patterns, and guidelines for new LSP handlers",
        },
        {
          action: "Add unit tests for concurrent LSP request scenarios",
          timing: "product",
          status: "active",
          outcome: null,
        },
      ],
    },
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
          outcome:
            "Created PBI-090 for Lua support; hover test is valid but needs CI setup",
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
