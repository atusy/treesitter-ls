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

  // Completed PBIs: PBI-001 through PBI-103 | History: git log -- scrum.yaml, scrum.ts
  // PBI-091 (idle cleanup): Infrastructure - already implemented, needs wiring (low priority)
  // PBI-098: Language-based routing - already implemented as part of PBI-097 (configurable bridge servers)
  product_backlog: [
    {
      id: "PBI-104",
      story: {
        role: "documentation author with Rust code blocks",
        capability:
          "see rust-analyzer progress notifications during server initialization (not just during requests)",
        benefit:
          "I can see Loading crates and Indexing status as soon as I open a file, not just on first goto_definition",
      },
      acceptance_criteria: [
        {
          criterion:
            "wait_for_indexing captures $/progress notifications instead of discarding them",
          verification:
            "Unit test: wait_for_indexing returns collected $/progress notifications",
        },
        {
          criterion:
            "spawn_in_background accepts a channel/callback to forward progress notifications",
          verification:
            "Unit test: spawn_in_background sends notifications through channel during spawn",
        },
        {
          criterion:
            "eager_spawn_for_injections forwards received progress notifications to the LSP client",
          verification:
            "E2E test: progress_messages captured after opening file with Rust code block",
        },
      ],
      status: "ready",
    },
  ],

  sprint: {
    number: 81,
    pbi_id: "PBI-104",
    goal:
      "Documentation authors see rust-analyzer progress notifications during server initialization (not just during requests) so they know indexing status as soon as they open a file",
    status: "review",
    subtasks: [
      {
        test: "wait_for_indexing_with_notifications returns captured $/progress notifications",
        implementation:
          "Create wait_for_indexing_with_notifications method that reads messages until publishDiagnostics, capturing all $/progress notifications into a Vec<Value>",
        type: "behavioral",
        status: "completed",
        commits: [
          {
            hash: "f270ef3",
            message:
              "feat(init-progress): add wait_for_indexing_with_notifications method",
            phase: "green",
          },
        ],
        notes: [
          "Current wait_for_indexing discards all messages except publishDiagnostics",
          "New method returns Vec<Value> of captured $/progress notifications",
        ],
      },
      {
        test: "did_open_with_notifications returns captured $/progress notifications from indexing",
        implementation:
          "Create did_open_with_notifications method that calls wait_for_indexing_with_notifications and returns the captured notifications",
        type: "behavioral",
        status: "completed",
        commits: [
          {
            hash: "92fd24b",
            message:
              "feat(init-progress): add did_open_with_notifications method",
            phase: "green",
          },
        ],
        notes: [
          "did_open calls wait_for_indexing but discards return value",
          "did_open_with_notifications returns the notifications for forwarding",
        ],
      },
      {
        test: "spawn_with_notifications returns captured $/progress notifications from initialization",
        implementation:
          "Create spawn_with_notifications that returns (LanguageServerConnection, Vec<Value>) tuple with initialization notifications",
        type: "behavioral",
        status: "completed",
        commits: [
          {
            hash: "e8080fe",
            message:
              "feat(init-progress): add spawn_with_notifications method",
            phase: "green",
          },
        ],
        notes: [
          "spawn() calls read_response_for_id which discards notifications",
          "spawn_with_notifications uses read_response_for_id_with_notifications",
        ],
      },
      {
        test: "spawn_in_background_with_notifications accepts tokio channel and sends captured notifications",
        implementation:
          "Add spawn_in_background_with_notifications method that takes mpsc::Sender<Value> and sends notifications through it during spawn",
        type: "behavioral",
        status: "completed",
        commits: [
          {
            hash: "c349850",
            message:
              "feat(init-progress): add spawn_in_background_with_notifications method",
            phase: "green",
          },
        ],
        notes: [
          "spawn_in_background is fire-and-forget with no notification capture",
          "New method takes channel to forward notifications to caller",
        ],
      },
      {
        test: "eager_spawn_for_injections forwards $/progress notifications to LSP client",
        implementation:
          "Modify eager_spawn_for_injections to use spawn_in_background_with_notifications and forward notifications via client.send_notification",
        type: "behavioral",
        status: "completed",
        commits: [
          {
            hash: "709b470",
            message:
              "feat(init-progress): forward $/progress notifications during eager spawn",
            phase: "green",
          },
        ],
        notes: [
          "spawn_in_background_with_notifications now also calls did_open_with_notifications to trigger indexing",
          "E2E test in tests/test_lsp_notification.lua validates infrastructure works",
          "Note: actual progress messages depend on rust-analyzer having work (simple projects may not generate progress)",
        ],
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
      number: 80,
      pbi_id: "PBI-103",
      goal:
        "Documentation authors see actual rust-analyzer progress notifications instead of synthetic 'Waiting' messages so they know exactly what rust-analyzer is doing",
      status: "done",
      subtasks: [],
    },
    {
      number: 79,
      pbi_id: "PBI-102",
      goal:
        "Documentation authors have bridge server connections pre-warmed when opening a document so their first go-to-definition request is fast",
      status: "done",
      subtasks: [],
    },
  ],

  // Recent 2 retrospectives | Sprint 1-77: git log -- scrum.yaml, scrum.ts
  retrospectives: [
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
    {
      sprint: 79,
      improvements: [
        {
          action:
            "Arc<DashMap> pattern enables clean async sharing - consider documenting this pattern for future concurrent features",
          timing: "immediate",
          status: "completed",
          outcome:
            "Pattern documented in CLAUDE.md DashMap Lock Pattern section (Sprint 71). Pattern reused successfully here.",
        },
        {
          action:
            "Add E2E test verifying eager spawn reduces first goto-definition latency in real Markdown file",
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
