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

  product_backlog: [],
  sprint: null,
  completed: [
    {
      number: 153,
      pbi_id: "PBI-WIRE-FAILED-STATE",
      goal: "Return REQUEST_FAILED when downstream server has failed initialization",
      status: "done",
      subtasks: [
        { test: "Failed state returns error for hover", implementation: "Change if-let to match in send_hover_request", type: "behavioral", status: "completed", commits: [{ hash: "4f0674c5", message: "feat(lsp): check Failed state in send_hover_request", phase: "green" }], notes: [] },
        { test: "Failed state returns error for completion", implementation: "Change if-let to match in send_completion_request", type: "behavioral", status: "completed", commits: [{ hash: "8d3afda6", message: "feat(lsp): check Failed state in send_completion_request", phase: "green" }], notes: [] },
        { test: "N/A (structural)", implementation: "Update comment to reflect both states", type: "structural", status: "completed", commits: [{ hash: "5b680b65", message: "docs(lsp): update doc comments for Failed state check", phase: "refactoring" }], notes: [] },
      ],
    },
    {
      number: 152,
      pbi_id: "PBI-REQUEST-FAILED-INIT",
      goal: "Return REQUEST_FAILED immediately during initialization instead of blocking",
      status: "done",
      subtasks: [
        { test: "ConnectionState transitions", implementation: "Add enum + state tracking", type: "behavioral", status: "completed", commits: [{ hash: "a54b2c05", message: "feat(lsp): add ConnectionState enum", phase: "green" }], notes: [] },
        { test: "REQUEST_FAILED during init", implementation: "Gate on Ready state", type: "behavioral", status: "completed", commits: [{ hash: "9a2c06d0", message: "feat(lsp): return REQUEST_FAILED immediately", phase: "green" }], notes: [] },
        { test: "Exact error message", implementation: "bridge: downstream server initializing", type: "behavioral", status: "completed", commits: [{ hash: "cc7fc6e7", message: "test(lsp): verify exact error message", phase: "green" }], notes: [] },
        { test: "Requests work after Ready", implementation: "Regression test", type: "behavioral", status: "completed", commits: [{ hash: "33293e08", message: "test(lsp): regression test for ready state", phase: "green" }], notes: [] },
      ],
    },
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
    {
      sprint: 152,
      improvements: [
        { action: "ConnectionState enum provides foundation for ADR-0015 state machine", timing: "immediate", status: "completed", outcome: "State tracking enables non-blocking request gating" },
        { action: "Separate state map from connection map enables checking state before blocking", timing: "immediate", status: "completed", outcome: "connection_states HashMap decouples state check from connection acquisition" },
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
