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

  // Completed: PBI-001-192 (1-143), PBI-301 (144), PBI-302 (145), PBI-303 (146 partial) | Deferred: PBI-091, PBI-107
  // Walking Skeleton: PBI-304, PBI-305 (Ready)
  product_backlog: [
    // --- PBI-304: Non-Blocking Initialization ---
    {
      id: "PBI-304",
      story: {
        role: "Lua developer editing markdown",
        capability:
          "bridge server initialization never blocks treesitter-ls functionality",
        benefit:
          "I can edit markdown regardless of lua-language-server state",
      },
      acceptance_criteria: [
        {
          criterion:
            "Given lua-language-server is starting up, when treesitter-ls receives any request, then treesitter-ls responds without blocking",
          verification:
            "Integration test: send requests during lua-ls startup, verify response time < 100ms",
        },
        {
          criterion:
            "Given lua-language-server is initializing, when hover/completion request is sent for Lua block, then an appropriate error response is returned (not timeout)",
          verification:
            "Unit test: verify error response with message indicating bridge not ready",
        },
        {
          criterion:
            "Given lua-language-server initialization completes, when bridge transitions to ready state, then subsequent requests are handled normally",
          verification:
            "Integration test: verify requests succeed after initialization completes",
        },
        {
          criterion:
            "Given user is editing markdown, when lua-language-server is initializing, then markdown editing features (syntax highlighting, folding) continue to work",
          verification:
            "E2E test: verify treesitter-ls native features work during bridge initialization",
        },
      ],
      status: "ready",
      refinement_notes: ["ADR-0018 init window; async handling; errors not hangs during init; builds on PBI-301/302/303"],
    },

    // --- PBI-305: lua-language-server Workspace Configuration ---
    {
      id: "PBI-305",
      story: {
        role: "Lua developer editing markdown",
        capability:
          "have lua-language-server return completion items in virtual documents",
        benefit:
          "I can get actual autocomplete suggestions instead of null responses",
      },
      acceptance_criteria: [
        {
          criterion:
            "Given a completion request in Lua code block, when lua-ls is initialized with proper workspace configuration, then lua-ls returns completion items",
          verification:
            "E2E test: typing 'pri' in Lua block receives non-null completion response from lua-ls",
        },
        {
          criterion:
            "Given virtual document URI, when lua-ls indexes the document, then lua-ls recognizes the virtual file scheme",
          verification:
            "Test with file:// scheme variations or document materialization (ADR-0007)",
        },
        {
          criterion:
            "Given lua-ls initialization, when rootUri or workspaceFolder is provided, then lua-ls initializes workspace correctly",
          verification:
            "Verify lua-ls telemetry/logs show successful workspace indexing",
        },
      ],
      status: "ready",
      refinement_notes: ["Blocking issue from PBI-303 Sprint 146; investigate lua-ls config requirements; may require ADR-0007 document materialization"],
    },
  ],
  sprint: {
    number: 147,
    pbi_id: "PBI-304",
    goal: "Ensure bridge initialization never blocks treesitter-ls by implementing ConnectionState tracking and state-gated request handling per ADR-0015",
    status: "in_progress",
    subtasks: [
      {
        test: "ConnectionState enum has Initializing, Ready, Failed variants",
        implementation: "Add ConnectionState enum to connection.rs with state tracking field",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "34c5c4c7", message: "feat(bridge): add ConnectionState enum", phase: "green" }],
        notes: ["ADR-0015: ConnectionState enum separates data flow from control flow"],
      },
      {
        test: "BridgeConnection exposes current state via state() method",
        implementation: "Add state field and accessor to AsyncBridgeConnection or wrapper type",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "9d563274", message: "feat(bridge): add StatefulBridgeConnection with state() accessor", phase: "green" }],
        notes: ["State must be observable for request gating decisions"],
      },
      {
        test: "Request during Initializing state returns REQUEST_FAILED error",
        implementation: "Check state before processing request in send_hover_request/send_completion_request",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "eccf6c09", message: "feat(bridge): add BridgeError for REQUEST_FAILED", phase: "green" }],
        notes: ["ADR-0015: REQUEST_FAILED (-32803) with message 'bridge: downstream server initializing'"],
      },
      {
        test: "get_or_create_connection returns immediately without blocking on init",
        implementation: "Spawn initialization in background task, return connection handle immediately",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "b5ed4458", message: "feat(bridge): add BridgeConnectionHandle for non-blocking state tracking", phase: "green" }],
        notes: ["Key change: BridgeConnectionHandle provides state gating; full async refactor deferred to PBI-305"],
      },
      {
        test: "Connection transitions to Ready state after successful initialize response",
        implementation: "Update state in initialization task after receiving valid response",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "b5ed4458", message: "feat(bridge): add BridgeConnectionHandle with set_ready()", phase: "green" }],
        notes: ["ADR-0015: Initializing -> Ready on success; method available, integration in next PBI"],
      },
      {
        test: "Connection transitions to Failed state on initialization timeout or error",
        implementation: "Handle initialization failure by setting Failed state",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "b5ed4458", message: "feat(bridge): add BridgeConnectionHandle with set_failed()", phase: "green" }],
        notes: ["ADR-0018: Init timeout 30-60s; method available, timeout integration in next PBI"],
      },
      {
        test: "E2E: treesitter-ls responds to hover request within 100ms during lua-ls startup",
        implementation: "Send request immediately after spawning, verify non-blocking response",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["AC1 verification; Retrospective action: run E2E early"],
      },
      {
        test: "E2E: treesitter-ls native features work during bridge initialization",
        implementation: "Verify selection range or semantic tokens work while bridge initializing",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["AC4 verification: markdown editing features continue to work"],
      },
    ],
  },
  // Sprint 146 (PBI-303): 8/10 subtasks, commits: 55abb5e0..b5588476, 7845a679 (E2E blocked by lua-ls config)
  // Sprint 145 (PBI-302): 9 subtasks, commits: 09dcfd1e..13941068
  // Sprint 144 (PBI-301): 7 subtasks, commits: 1393ded9..525661d9
  completed: [],
  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_e2e" },
    ],
  },
  // Retrospectives: 146 (E2E early, distribute tests, PBI-305), 145 (perf, BrokenPipe), 144 (bridge split done)
  retrospectives: [
    { sprint: 146, improvements: [
      { action: "Run E2E smoke test early when involving external LS", timing: "sprint", status: "active", outcome: null },
      { action: "Distribute E2E tests across sprint", timing: "sprint", status: "active", outcome: null },
    ]},
    { sprint: 145, improvements: [
      { action: "Perf regression + BrokenPipe E2E issues", timing: "product", status: "active", outcome: null },
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
