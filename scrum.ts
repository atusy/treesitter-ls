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
      status: "ready",
    },
  ],

  sprint: {
    number: 82,
    pbi_id: "PBI-105",
    goal:
      "Make redirection.rs language-agnostic by removing hardcoded rust-analyzer and Cargo defaults so bridge servers are fully configurable via initializationOptions",
    status: "in_progress",
    subtasks: [
      {
        test: "E2E tests still pass after adding explicit rust-analyzer bridge config to minimal_init.lua",
        implementation:
          "Add bridge.servers configuration with rust-analyzer command and languages: ['rust'] to minimal_init.lua initializationOptions",
        type: "structural",
        status: "completed",
        commits: [],
        notes: [
          "Structural change - makes E2E tests work with explicit config before removing fallback",
          "Must add workspace_type: 'Cargo' to maintain current behavior for rust",
        ],
      },
      {
        test: "get_bridge_config_for_language returns None for rust when no bridge config exists (grep shows no rust-analyzer hardcoding)",
        implementation:
          "Remove lines 454-463 in lsp_impl.rs that provide hardcoded rust-analyzer fallback",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [
          "After removing fallback, bridge config is purely user-driven",
          "E2E tests must already have explicit config from subtask 1",
        ],
      },
      {
        test: "setup_workspace_with_option(None) creates Generic workspace (virtual.ext), not Cargo (Cargo.toml + src/main.rs)",
        implementation:
          "Change line 118 in redirection.rs from WorkspaceType::Cargo to WorkspaceType::Generic",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [
          "Update existing test setup_cargo_workspace_none_defaults_to_cargo to expect Generic",
          "May need to update test name to setup_workspace_with_option_none_defaults_to_generic",
        ],
      },
      {
        test: "spawn_rust_analyzer() is removed - grep shows no matches in src/",
        implementation:
          "Delete spawn_rust_analyzer() method (lines 191-257) and update all tests using it to use spawn() with explicit BridgeServerConfig",
        type: "structural",
        status: "pending",
        commits: [],
        notes: [
          "Tests to update: language_server_connection_is_alive_*, language_server_pool_respawns_dead_connection",
          "Each test needs explicit config with command: rust-analyzer, workspace_type: Cargo",
        ],
      },
      {
        test: "All unit tests pass and cargo clippy shows no warnings",
        implementation:
          "Run make test and make check to verify all changes work together",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["Final verification subtask"],
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

  // Historical sprints (recent 2) | Sprint 1-77: git log -- scrum.yaml, scrum.ts
  completed: [
    {
      number: 81,
      pbi_id: "PBI-104",
      goal:
        "Documentation authors see rust-analyzer progress notifications during server initialization (not just during requests) so they know indexing status as soon as they open a file",
      status: "done",
      subtasks: [],
    },
    {
      number: 80,
      pbi_id: "PBI-103",
      goal:
        "Documentation authors see actual rust-analyzer progress notifications instead of synthetic 'Waiting' messages so they know exactly what rust-analyzer is doing",
      status: "done",
      subtasks: [],
    },
  ],

  // Recent 2 retrospectives | Sprint 1-77: git log -- scrum.yaml, scrum.ts
  retrospectives: [
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
    {
      sprint: 80,
      improvements: [
        {
          action:
            "ResponseWithNotifications pattern cleanly separates concerns - reuse for other notification types (e.g., diagnostics)",
          timing: "product",
          status: "active",
          outcome: null,
        },
        {
          action:
            "Notification forwarding duplicated in goto_definition and hover - consider extracting helper method",
          timing: "product",
          status: "active",
          outcome: null,
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
