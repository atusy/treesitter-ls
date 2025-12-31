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
      "Improve LSP bridge go-to-definition to be production ready (ADR-0006, 0007, 0008)",
    success_metrics: [
      {
        metric: "Connection pooling implemented",
        target: "Server connections reused across requests",
      },
      {
        metric: "Configuration system complete",
        target: "User can configure bridge servers via initializationOptions",
      },
      {
        metric: "Robustness features",
        target: "Ready detection, timeout handling, crash recovery",
      },
    ],
  },

  // Completed PBIs: PBI-001 through PBI-104 | History: git log -- scrum.yaml, scrum.ts
  // PBI-091 (idle cleanup): Infrastructure - already implemented, needs wiring (low priority)
  // PBI-098: Language-based routing - already implemented as part of PBI-097 (configurable bridge servers)
  product_backlog: [
    {
      id: "PBI-105",
      story: {
        role: "developer editing Lua files",
        capability:
          "have redirection.rs be language-agnostic with no hardcoded rust-analyzer or Cargo defaults",
        benefit:
          "bridge servers are fully configurable via initializationOptions, no special-casing for any language",
      },
      acceptance_criteria: [
        {
          criterion:
            "spawn_rust_analyzer() method is removed from LanguageServerConnection",
          verification:
            "Grep for spawn_rust_analyzer shows no matches in src/",
        },
        {
          criterion:
            "get_bridge_config_for_language() has no hardcoded rust-analyzer fallback",
          verification:
            "Grep for 'rust-analyzer' in lsp_impl.rs shows no matches except comments/logs",
        },
        {
          criterion:
            "WorkspaceType defaults to Generic (not Cargo) when not specified",
          verification:
            "Unit test: None workspace_type defaults to Generic",
        },
        {
          criterion:
            "scripts/minimal_init.lua configures rust-analyzer bridge server via initializationOptions",
          verification:
            "E2E tests pass with explicit bridge config in minimal_init.lua",
        },
      ],
      status: "done",
    },
  ],

  sprint: null,

  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_nvim" },
    ],
  },

  // Historical sprints (recent 2) | Sprint 1-77: git log -- scrum.yaml, scrum.ts
  completed: [
    {
      number: 82,
      pbi_id: "PBI-105",
      goal:
        "Make redirection.rs language-agnostic by removing hardcoded rust-analyzer and Cargo defaults",
      status: "done",
      subtasks: [],
    },
    {
      number: 81,
      pbi_id: "PBI-104",
      goal:
        "Documentation authors see rust-analyzer progress notifications during server initialization",
      status: "done",
      subtasks: [],
    },
  ],

  // Recent 2 retrospectives | Sprint 1-77: git log -- scrum.yaml, scrum.ts
  retrospectives: [
    {
      sprint: 82,
      improvements: [
        {
          action:
            "Serde rename attributes need verification in integration tests - camelCase mismatch caught by E2E test, not unit test",
          timing: "immediate",
          status: "completed",
          outcome:
            "E2E tests provide essential coverage for JSON serialization boundaries; unit tests don't catch schema mismatches with external clients",
        },
        {
          action:
            "Avoid unit tests with blocking BufReader::read_line() on external processes - use E2E tests for external process integration",
          timing: "immediate",
          status: "completed",
          outcome:
            "Removed 8 blocking tests from redirection.rs; E2E tests in test_lsp_definition.lua and test_lsp_notification.lua provide reliable coverage",
        },
      ],
    },
    {
      sprint: 81,
      improvements: [
        {
          action:
            "Tokio channel for notification forwarding works well - spawn_in_background_with_notifications pattern enables async notification delivery without blocking",
          timing: "immediate",
          status: "completed",
          outcome:
            "Pattern implemented: spawn_in_background_with_notifications accepts Sender<Value> and forwards $/progress to LSP client during eager_spawn_for_injections",
        },
        {
          action:
            "Layered notification capture (wait_for_indexing -> did_open -> spawn -> spawn_in_background) provides clean separation - each layer captures and forwards appropriately",
          timing: "immediate",
          status: "completed",
          outcome:
            "Clean API: spawn returns (conn, notifications), did_open returns Vec<Value>, background spawn uses channel",
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
