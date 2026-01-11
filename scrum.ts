// ============================================================
// Dashboard Data (AI edits this section)
// ============================================================

const userStoryRoles = [
  "Lua developer editing markdown",
] as const satisfies readonly string[]; // Must have at least one role. Avoid generic roles like "user" or "admin". Remove obsolete roles freely.

const scrum: ScrumDashboard = {
  product_goal: {
    statement:
      "Implement LSP bridge to support essential language server features indirectly through bridging (ADR-0013, 0014, 0015, 0016, 0017, 0018)",
    success_metrics: [
      {
        metric: "ADR alignment",
        target:
          "Must align with Phase 1 of ADR-0013, 0014, 0015, 0016, 0017, 0018 in @docs/adr",
      },
      {
        metric: "Bridge coverage",
        target:
          "Support completion, signatureHelp, codeAction, definition, hover",
      },
      {
        metric: "Modular architecture",
        target:
          "Bridge module organized with text_document/ subdirectory matching lsp_impl structure",
      },
      {
        metric: "E2E test coverage",
        target: "Each bridged feature has E2E test verifying end-to-end flow",
      },
    ],
  },

  product_backlog: [
    {
      id: "PBI-REQUEST-FAILED-INIT",
      story: {
        role: "Lua developer editing markdown",
        capability: "receive an immediate error response when requesting hover/completion during downstream server initialization",
        benefit: "I get fast feedback instead of the editor appearing frozen",
      },
      acceptance_criteria: [
        { criterion: "Requests during Initializing return REQUEST_FAILED (-32803)", verification: "E2E test with error code check" },
        { criterion: "Error message is 'bridge: downstream server initializing'", verification: "E2E test message verification" },
        { criterion: "ConnectionState enum with Initializing/Ready/Failed", verification: "grep enum ConnectionState src/lsp/bridge/" },
        { criterion: "Requests succeed after Ready state", verification: "E2E test: request after init completes" },
      ],
      status: "done",
      refinement_notes: ["Depends on PBI-INIT-TIMEOUT", "ADR-0015 Operation Gating"],
    },
  ],
  sprint: {
    number: 152,
    pbi_id: "PBI-REQUEST-FAILED-INIT",
    goal: "Return REQUEST_FAILED immediately during initialization instead of blocking",
    status: "done",
    subtasks: [
      {
        test: "ConnectionState starts as Initializing, transitions to Ready after init",
        implementation: "Add ConnectionState enum, store state alongside connection",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "a54b2c05", message: "feat(lsp): add ConnectionState enum for tracking downstream server lifecycle", phase: "green" }],
        notes: ["ADR-0015 defines: Initializing -> Ready -> Failed/Closing -> Closed"],
      },
      {
        test: "Request during init returns error code -32803 (REQUEST_FAILED)",
        implementation: "Check state before forwarding request, return REQUEST_FAILED if not Ready",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "9a2c06d0", message: "feat(lsp): return REQUEST_FAILED immediately during downstream init", phase: "green" }],
        notes: ["Gate at send_hover_request and send_completion_request entry points"],
      },
      {
        test: "Error message is 'bridge: downstream server initializing'",
        implementation: "Return proper JSON-RPC error structure with message",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "cc7fc6e7", message: "test(lsp): verify exact error message for init-during-request", phase: "green" }],
        notes: ["Per ADR-0015: REQUEST_FAILED with this specific message"],
      },
      {
        test: "After init completes, requests work normally",
        implementation: "Verify existing flow still works when state is Ready",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "33293e08", message: "test(lsp): add regression test for requests after init completes", phase: "green" }],
        notes: ["Ensure no regression in happy path"],
      },
    ],
  },
  completed: [
    {
      number: 151,
      pbi_id: "PBI-INIT-TIMEOUT",
      goal: "Add timeout to initialization to prevent infinite hang when downstream server is unresponsive",
      status: "done",
      subtasks: [
        { test: "Timeout fires after duration", implementation: "tokio::time::timeout wrapper", type: "behavioral", status: "completed", commits: [{ hash: "adfbac9b", message: "feat(lsp): add timeout to initialization handshake", phase: "green" }], notes: [] },
        { test: "TimedOut error kind", implementation: "Map Elapsed to io::Error", type: "behavioral", status: "completed", commits: [{ hash: "6e5d27e2", message: "test(lsp): add test for TimedOut error kind", phase: "green" }], notes: [] },
        { test: "Connection not cached", implementation: "Verify existing behavior", type: "behavioral", status: "completed", commits: [{ hash: "d8379259", message: "test(lsp): verify connection not cached after timeout", phase: "green" }], notes: [] },
      ],
    },
  ],
  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_e2e" },
    ],
  },
  retrospectives: [
    {
      sprint: 151,
      improvements: [
        { action: "get_or_create_connection_with_timeout pattern enables testability", timing: "immediate", status: "completed", outcome: "Timeout duration injectable for unit tests" },
      ],
    },
  ],
};

// Type Definitions (DO NOT MODIFY) =============================================
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
  refinement_notes?: string[];
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
