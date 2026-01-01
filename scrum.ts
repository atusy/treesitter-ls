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
      "Expand LSP bridge to support most language server features indirectly through bridging (ADR-0006, 0007, 0008)",
    success_metrics: [
      {
        metric: "Bridge coverage",
        target:
          "Support completion, signatureHelp, references, rename, codeAction, formatting, typeDefinition, implementation, documentHighlight, declaration, inlayHint, callHierarchy, typeHierarchy, documentLink, foldingRange",
      },
      {
        metric: "Modular architecture",
        target: "Bridge module organized with text_document/ subdirectory matching lsp_impl structure",
      },
      {
        metric: "E2E test coverage",
        target: "Each bridged feature has E2E test verifying end-to-end flow",
      },
    ],
  },

  // Completed PBIs: PBI-001 through PBI-133 | History: git log -- scrum.yaml, scrum.ts
  // PBI-091 (idle cleanup): Deferred - infrastructure already implemented, needs wiring (low priority)
  // PBI-107 (remove WorkspaceType): Deferred - rust-analyzer linkedProjects too slow
  product_backlog: [
    {
      id: "PBI-132",
      story: {
        role: "developer editing Lua files",
        capability: "have treesitter-ls remain responsive when bridged language servers are slow or unresponsive",
        benefit: "the LSP does not hang indefinitely when rust-analyzer or other bridged servers fail to respond",
      },
      acceptance_criteria: [
        { criterion: "Bridge I/O operations have configurable timeout", verification: "read_response_for_id_with_notifications accepts timeout parameter; defaults to 30 seconds" },
        { criterion: "Timeout triggers graceful error handling", verification: "When timeout expires, function returns None response with empty notifications; no infinite loop" },
        { criterion: "All bridge request methods use timeout", verification: "22+ *_with_notifications methods pass DEFAULT_TIMEOUT to read function" },
        { criterion: "Unit test verifies timeout behavior", verification: "Test read_response_for_id_with_notifications returns None after timeout" },
      ],
      status: "done",
    },
    {
      id: "PBI-133",
      story: {
        role: "developer editing Lua files",
        capability: "have DashMap operations in DocumentStore verified safe and documented",
        benefit: "the LSP does not freeze when concurrent document updates occur and future changes maintain safety",
      },
      acceptance_criteria: [
        { criterion: "DashMap lock safety verified with concurrent test", verification: "Unit test spawns multiple threads calling update_document and get concurrently; no deadlock within 5 seconds" },
        { criterion: "DocumentStore methods have lock safety comments", verification: "Each method using DashMap has comment explaining why lock pattern is safe (e.g., 'Ref consumed by and_then before insert')" },
        { criterion: "CLAUDE.md DashMap pattern documented", verification: "CLAUDE.md 'DashMap Lock Pattern' section exists with safe/unsafe examples (already present)" },
      ],
      status: "done",
    },
    {
      id: "PBI-134",
      story: {
        role: "developer editing Lua files",
        capability: "have bridge connection use async request/response queue pattern",
        benefit: "concurrent LSP requests share one connection without blocking each other",
      },
      acceptance_criteria: [
        { criterion: "Single connection handles multiple concurrent requests", verification: "Background reader dispatches responses by request ID; no duplicate connections spawned" },
        { criterion: "Requests get responses via channel", verification: "send_request returns oneshot::Receiver<Response>; caller awaits response asynchronously" },
        { criterion: "Response routing uses request ID", verification: "pending_requests map stores Sender by ID; reader routes responses to correct caller" },
        { criterion: "Existing unit tests still pass", verification: "make test passes; no regression in bridge functionality" },
      ],
      status: "done",
    },
    {
      id: "PBI-135",
      story: {
        role: "developer editing Lua files",
        capability: "have all bridge handlers use the async pool pattern",
        benefit: "no bridge request can cause hangs regardless of which LSP feature is invoked",
      },
      acceptance_criteria: [
        { criterion: "Navigation handlers use async pool", verification: "definition, declaration, implementation, typeDefinition, references handlers use async_language_server_pool" },
        { criterion: "Edit handlers use async pool", verification: "rename, codeAction, formatting handlers use async_language_server_pool" },
        { criterion: "Document handlers use async pool", verification: "inlayHint, foldingRange, documentLink handlers use async_language_server_pool" },
        { criterion: "Hierarchy handlers use async pool", verification: "callHierarchy (prepare/incoming/outgoing), typeHierarchy (prepare/supertypes/subtypes) use async_language_server_pool" },
      ],
      status: "ready",
    },
    {
      id: "PBI-136",
      story: {
        role: "developer editing Lua files",
        capability: "have the legacy synchronous bridge pool removed",
        benefit: "codebase is simpler with only one connection management pattern",
      },
      acceptance_criteria: [
        { criterion: "LanguageServerPool removed from TreeSitterLs", verification: "language_server_pool field removed; only async_language_server_pool remains" },
        { criterion: "Legacy pool module can be removed", verification: "pool.rs, connection.rs only used for async pool initialization; sync methods removed or deprecated" },
        { criterion: "All tests pass without legacy pool", verification: "make test && make check && make test_nvim all pass" },
      ],
      status: "draft",
    },
  ],

  sprint: {
    number: 112,
    pbi_id: "PBI-134",
    goal: "Implement async request/response queue pattern for bridge connections",
    status: "done",
    subtasks: [
      {
        test: "Unit test for pending_requests map routing",
        implementation: "Add pending_requests: Arc<DashMap<i64, oneshot::Sender<Response>>> to AsyncBridgeConnection",
        type: "behavioral",
        status: "completed",
        commits: [],
        notes: ["Created async_connection.rs with pending_requests map and routing test"],
      },
      {
        test: "Test send_request returns receiver that gets response",
        implementation: "send_request writes JSON-RPC, stores sender in map, returns receiver",
        type: "behavioral",
        status: "completed",
        commits: [],
        notes: ["AsyncBridgeConnection.send_request returns (id, oneshot::Receiver<ResponseResult>)"],
      },
      {
        test: "Test background reader dispatches responses correctly",
        implementation: "Background thread reads responses, looks up sender by ID, sends response",
        type: "behavioral",
        status: "completed",
        commits: [],
        notes: ["reader_loop spawned in AsyncBridgeConnection::new routes by request ID"],
      },
      {
        test: "Existing bridge tests still pass",
        implementation: "AsyncLanguageServerPool wraps AsyncBridgeConnection; old pool unchanged",
        type: "behavioral",
        status: "completed",
        commits: [],
        notes: ["make test passes; async pool tests verify concurrent request sharing"],
      },
      {
        test: "High-frequency handlers use async pool",
        implementation: "Wire hover, completion, signatureHelp, documentHighlight to async_language_server_pool",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "a6beafa", message: "feat(bridge): wire async pool to high-frequency LSP handlers", phase: "green" }],
        notes: ["4 most frequently called handlers now use shared connection pattern; remaining handlers in PBI-135"],
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

  // Historical sprints (recent 2) | Sprint 1-109: git log -- scrum.yaml, scrum.ts
  completed: [
    { number: 111, pbi_id: "PBI-134", goal: "Add per-key mutex to LanguageServerPool (cancelled - approach insufficient)", status: "cancelled", subtasks: [] },
    { number: 110, pbi_id: "PBI-133", goal: "Verify DashMap lock safety with concurrent test and add safety documentation", status: "done", subtasks: [] },
  ],

  // Recent 2 retrospectives | Sprint 1-108: modular refactoring pattern, E2E indexing waits
  retrospectives: [
    {
      sprint: 110,
      improvements: [
        { action: "Investigate root cause earlier when PBI assumes a bug exists - validate assumption before detailed implementation planning", timing: "immediate", status: "completed", outcome: "Sprint 110 refinement correctly pivoted from 'fix deadlock' to 'verify and document safety' when code was found already safe" },
        { action: "Document Rust's .and_then() pattern as key to DashMap safety - it consumes Ref guard before subsequent operations", timing: "immediate", status: "completed", outcome: "Lock safety comments added to DocumentStore methods explaining .and_then() pattern" },
        { action: "User hang issue investigation: bridge I/O timeout (Sprint 109) and DashMap (Sprint 110) ruled out - investigate tokio::spawn panics or other mutex contention as next step", timing: "product", status: "completed", outcome: "Root cause identified: concurrent LSP requests (hover, completion, etc.) spawning duplicate connections fighting over stdout. Fixed via async pool pattern in Sprint 112 (PBI-134)" },
      ],
    },
    {
      sprint: 109,
      improvements: [
        { action: "Blocking I/O timeout checks happen between read operations, not during - for truly responsive timeout would need async I/O", timing: "product", status: "completed", outcome: "30s timeout prevents indefinite hangs; async migration deferred" },
        { action: "PBIs should have ≤4 acceptance criteria to fit in one sprint; large PBIs should be split", timing: "immediate", status: "completed", outcome: "PBI-132 split from original 6 ACs to 4 ACs; DashMap fix moved to PBI-133" },
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
