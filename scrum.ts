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
      id: "PBI-REQUEST-ID-PASSTHROUGH",
      story: {
        role: "Lua developer editing markdown",
        capability: "have my LSP requests use consistent IDs across client, bridge, and downstream server",
        benefit: "I get simpler debugging and state management as documented in ADR-0016",
      },
      acceptance_criteria: [
        {
          criterion: "next_request_id counter removed from LanguageServerPool",
          verification: "grep for 'next_request_id' in pool.rs returns no matches",
        },
        {
          criterion: "send_hover_request accepts upstream request ID as parameter",
          verification: "Function signature includes request_id: i64 parameter; grep shows no self.next_request_id() call in hover.rs",
        },
        {
          criterion: "send_completion_request accepts upstream request ID as parameter",
          verification: "Function signature includes request_id: i64 parameter; grep shows no self.next_request_id() call in completion.rs",
        },
        {
          criterion: "Upstream request ID flows through to downstream server unchanged",
          verification: "Integration test verifies request ID=42 from client appears in downstream request as ID=42",
        },
        {
          criterion: "ADR-0016 Phase 1 Request ID Semantics diagram matches implementation",
          verification: "Code review confirms: Client (ID=42) -> bridge -> downstream (ID=42) with no transformation",
        },
      ],
      status: "ready",
      refinement_notes: [
        "ADR-0016 lines 77-91 explicitly require upstream IDs: 'Use upstream request IDs directly for downstream servers'",
        "Current violation: pool.rs:102-113 has next_request_id counter generating new IDs",
        "Current violation: hover.rs:66 calls self.next_request_id() instead of using upstream ID",
        "Current violation: completion.rs:83 calls self.next_request_id() instead of using upstream ID",
        "Fix scope: Remove counter, thread upstream ID through send_hover_request/send_completion_request",
        "Callers (lsp_impl.rs) already have access to upstream request ID from tower-lsp handler",
      ],
    },
  ],
  sprint: {
    number: 156,
    pbi_id: "PBI-REQUEST-ID-PASSTHROUGH",
    goal: "Pass upstream request IDs to downstream servers per ADR-0016",
    status: "in_progress",
    subtasks: [
      {
        test: "Integration test: request ID=42 from upstream appears as ID=42 in downstream request",
        implementation: "Add test verifying request ID passthrough from caller to downstream server",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [
          "Test should verify the exact ID flows through unchanged",
          "May need mock server to capture the request ID received",
        ],
      },
      {
        test: "N/A (structural refactor)",
        implementation: "Add request_id: i64 parameter to send_hover_request signature",
        type: "structural",
        status: "pending",
        commits: [],
        notes: [
          "Update hover.rs function signature",
          "Update all callers to pass a placeholder ID temporarily",
          "Existing tests will need request_id parameter added",
        ],
      },
      {
        test: "N/A (structural refactor)",
        implementation: "Add request_id: i64 parameter to send_completion_request signature",
        type: "structural",
        status: "pending",
        commits: [],
        notes: [
          "Update completion.rs function signature",
          "Update all callers to pass a placeholder ID temporarily",
          "Existing tests will need request_id parameter added",
        ],
      },
      {
        test: "send_hover_request uses provided request_id instead of self.next_request_id()",
        implementation: "Replace self.next_request_id() call with request_id parameter in hover.rs",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [
          "Pass request_id to build_bridge_hover_request",
          "Verify response matching uses same ID",
        ],
      },
      {
        test: "send_completion_request uses provided request_id instead of self.next_request_id()",
        implementation: "Replace self.next_request_id() call with request_id parameter in completion.rs",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [
          "Pass request_id to build_bridge_completion_request",
          "Verify response matching uses same ID",
        ],
      },
      {
        test: "N/A (structural refactor)",
        implementation: "Remove next_request_id field and method from LanguageServerPool",
        type: "structural",
        status: "pending",
        commits: [],
        notes: [
          "Delete next_request_id: AtomicI64 field from pool.rs",
          "Delete pub(super) fn next_request_id() method",
          "Verify no remaining callers of next_request_id()",
          "Note: initialize handshake still needs an ID - consider using fixed ID=0 for init",
        ],
      },
      {
        test: "lsp_impl callers pass upstream request ID to bridge methods",
        implementation: "Update hover_impl and completion_impl to pass request ID from tower-lsp",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [
          "Investigate how to access request ID from tower-lsp handlers",
          "If tower-lsp doesn't expose IDs, document alternative approach",
          "May need to use tower-lsp custom handler or middleware",
        ],
      },
    ],
  },
  completed: [
    {
      number: 155,
      pbi_id: "PBI-RETRY-FAILED-CONNECTION",
      goal: "Enable automatic retry when downstream server connection has failed",
      status: "done",
      subtasks: [
        {
          test: "Failed connection retry removes cached entry and spawns new server",
          implementation: "Remove failed connection from cache, recursively call get_or_create_connection_with_timeout",
          type: "behavioral",
          status: "completed",
          commits: [{ hash: "cf8a69c7", message: "feat(lsp): auto-retry failed downstream server connections", phase: "green" }],
          notes: [
            "Modify ConnectionState::Failed branch in get_or_create_connection_with_timeout",
            "Pattern: connections.remove(language); drop(connections); return self.get_or_create_connection_with_timeout(...).await",
          ],
        },
        {
          test: "Recovery works after initialization timeout",
          implementation: "Verify timeout followed by successful connection on retry",
          type: "behavioral",
          status: "completed",
          commits: [{ hash: "fd740e96", message: "test(lsp): add integration test for recovery after initialization timeout", phase: "green" }],
          notes: [
            "Integration test: first request times out, second request succeeds with working server",
            "Requires swapping server config between calls to simulate recovery",
          ],
        },
      ],
    },
    {
      number: 154,
      pbi_id: "PBI-STATE-PER-CONNECTION",
      goal: "Move ConnectionState to per-connection ownership fixing race condition",
      status: "done",
      subtasks: [
        { test: "N/A (structural refactor)", implementation: "Create ConnectionHandle wrapper struct with state and connection fields", type: "structural", status: "completed", commits: [{ hash: "ddf6e08d", message: "refactor(lsp): move ConnectionState to per-connection via ConnectionHandle", phase: "refactoring" }], notes: ["Single structural commit as this is pure refactoring with no behavior change"] },
      ],
    },
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
    { sprint: 155, improvements: [
      { action: "Box::pin for recursive async calls prevents infinite future size", timing: "immediate", status: "completed", outcome: "Recursive retry compiles" },
      { action: "Integration tests with timeout swapping validate recovery", timing: "immediate", status: "completed", outcome: "Timeout->success flow tested" },
    ]},
    { sprint: 154, improvements: [
      { action: "Per-connection state via ConnectionHandle prevents race conditions", timing: "immediate", status: "completed", outcome: "State ownership explicit" },
      { action: "std::sync::RwLock for sync checks, tokio::sync::Mutex for async I/O", timing: "immediate", status: "completed", outcome: "Fast state checks" },
    ]},
    { sprint: 153, improvements: [{ action: "Review state machines for completeness", timing: "immediate", status: "completed", outcome: "Failed state wired" }]},
    { sprint: 152, improvements: [{ action: "ConnectionState enum foundation for ADR-0015", timing: "immediate", status: "completed", outcome: "Non-blocking gating" }]},
    { sprint: 151, improvements: [{ action: "Timeout injection pattern enables testability", timing: "immediate", status: "completed", outcome: "Configurable timeout" }]},
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
