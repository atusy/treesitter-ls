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
      id: "PBI-REQUEST-ID-SERVICE-WRAPPER",
      story: {
        role: "Lua developer editing markdown",
        capability: "have upstream request IDs flow through to downstream servers unchanged",
        benefit: "I get consistent request ID tracing across client, bridge, and server as specified in ADR-0016",
      },
      acceptance_criteria: [
        {
          criterion: "Tower Service wrapper captures request IDs from req.id()",
          verification: "Unit test verifies RequestIdCapture middleware extracts ID from tower-lsp Request",
        },
        {
          criterion: "Captured IDs flow to bridge layer via task-local storage or request context",
          verification: "Unit test verifies ID is accessible in send_hover_request and send_completion_request",
        },
        {
          criterion: "Downstream servers receive the upstream request ID unchanged",
          verification: "Integration test sends hover request with ID=42, verifies downstream receives ID=42",
        },
        {
          criterion: "Internal next_request_id counter is removed from LanguageServerPool",
          verification: "Code review confirms counter removal and all request IDs come from upstream",
        },
      ],
      status: "ready",
      refinement_notes: [
        "Sprint 156 concluded tower-lsp doesn't expose request IDs, but Service wrapper CAN access them",
        "Implementation approach: Create RequestIdCapture<S> wrapper implementing tower Service trait",
        "In Service::call(), extract req.id() before delegating to inner service",
        "Thread ID to bridge via tokio::task_local! or explicit RequestContext parameter",
        "Key code pattern: impl<S> Service<Request> for RequestIdCapture<S> { fn call(&mut self, req: Request) { if let Some(id) = req.id() { ... } } }",
        "This supersedes Sprint 156 finding - actual implementation path now validated",
      ],
    },
  ],
  sprint: null,
  completed: [
    { number: 156, pbi_id: "PBI-REQUEST-ID-PASSTHROUGH", goal: "Validate ADR-0016 request ID semantics (research sprint)", status: "done", subtasks: [] },
    { number: 155, pbi_id: "PBI-RETRY-FAILED-CONNECTION", goal: "Enable automatic retry when downstream server connection has failed", status: "done", subtasks: [] },
    { number: 154, pbi_id: "PBI-STATE-PER-CONNECTION", goal: "Move ConnectionState to per-connection ownership fixing race condition", status: "done", subtasks: [] },
    { number: 153, pbi_id: "PBI-WIRE-FAILED-STATE", goal: "Return REQUEST_FAILED when downstream server has failed initialization", status: "done", subtasks: [] },
    { number: 152, pbi_id: "PBI-REQUEST-FAILED-INIT", goal: "Return REQUEST_FAILED immediately during initialization instead of blocking", status: "done", subtasks: [] },
    { number: 151, pbi_id: "PBI-INIT-TIMEOUT", goal: "Add timeout to initialization to prevent infinite hang", status: "done", subtasks: [] },
  ],
  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_e2e" },
    ],
  },
  retrospectives: [
    { sprint: 156, improvements: [
      { action: "Investigate framework constraints before planning", timing: "immediate", status: "completed", outcome: "tower-lsp LanguageServer trait doesn't expose IDs, but Service wrapper can" },
      { action: "Distinguish ADR intent vs literal interpretation", timing: "immediate", status: "completed", outcome: "ADR-0016 intent achievable via Service wrapper pattern" },
      { action: "Research sprints are valid outcomes", timing: "immediate", status: "completed", outcome: "Research led to Service wrapper discovery - PBI-REQUEST-ID-SERVICE-WRAPPER created" },
    ]},
    { sprint: 155, improvements: [
      { action: "Box::pin for recursive async calls", timing: "immediate", status: "completed", outcome: "Recursive retry compiles" },
    ]},
    { sprint: 154, improvements: [
      { action: "Per-connection state via ConnectionHandle", timing: "immediate", status: "completed", outcome: "Race conditions fixed" },
    ]},
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
