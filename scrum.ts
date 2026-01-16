// ============================================================
// Dashboard Data (AI edits this section)
// ============================================================

const userStoryRoles = [
  "developer using tree-sitter-ls with multiple embedded languages",
  "editor plugin author integrating tree-sitter-ls",
] as const satisfies readonly string[]; // Must have at least one role. Avoid generic roles like "user" or "admin". Remove obsolete roles freely.

const scrum: ScrumDashboard = {
  product_goal: {
    statement: "Reliable connection lifecycle for embedded language servers",
    success_metrics: [
      { metric: "Connection state machine completeness", target: "ConnectionState includes Initializing, Ready, Failed, Closing, Closed with all Phase 1 transitions implemented" },
      { metric: "LSP shutdown handshake compliance", target: "Graceful shutdown sends shutdown request, waits for response, sends exit notification per LSP spec" },
      { metric: "Timeout hierarchy implementation", target: "Init timeout (30-60s), Liveness timeout (30-120s), Global shutdown timeout (5-15s) with correct precedence" },
      { metric: "Cancellation forwarding", target: "$/cancelRequest notifications forwarded to downstream servers while keeping pending request entries" },
    ],
  },
  product_backlog: [
    {
      id: "pbi-connection-states",
      story: {
        role: "developer using tree-sitter-ls with multiple embedded languages",
        capability: "have proper connection lifecycle management with Closing and Closed states",
        benefit: "connections shut down gracefully without race conditions or operation leaks",
      },
      acceptance_criteria: [
        { criterion: "ConnectionState enum includes Closing and Closed variants", verification: "Unit test verifies enum has all 5 states: Initializing, Ready, Failed, Closing, Closed" },
        { criterion: "Ready to Closing transition on shutdown signal", verification: "Unit test: Ready state + shutdown signal = Closing state" },
        { criterion: "Initializing to Closing transition on shutdown signal", verification: "Unit test: Initializing state + shutdown signal = Closing state" },
        { criterion: "Closing to Closed transition on completion/timeout", verification: "Unit test: Closing state completes gracefully or times out to Closed" },
        { criterion: "Failed to Closed transition (direct, no LSP handshake)", verification: "Unit test: Failed state transitions directly to Closed, bypassing Closing" },
        { criterion: "Operation gating during Closing state rejects new operations", verification: "Unit test: new requests in Closing state receive REQUEST_FAILED error" },
      ],
      status: "ready",
      refinement_notes: [
        "FILES TO MODIFY:",
        "- src/lsp/bridge/pool.rs: Add Closing and Closed variants to ConnectionState enum (line 38-45)",
        "- src/lsp/bridge/pool.rs: Update set_state() to enforce valid state transitions per ADR-0015/ADR-0017",
        "- src/lsp/bridge/pool.rs: Add operation gating for Closing/Closed states in request handling",
        "- src/lsp/bridge/text_document/did_change.rs: Update state check to handle Closing/Closed (line 49)",
        "- src/lsp/bridge/text_document/did_close.rs: Update state check to handle Closing/Closed (line 38)",
        "",
        "IMPLEMENTATION DETAILS:",
        "- ConnectionState enum currently has 3 variants: Initializing, Ready, Failed",
        "- Add Closing variant: shutdown initiated, draining operations, new ops rejected with REQUEST_FAILED",
        "- Add Closed variant: terminal state, connection fully terminated",
        "- State transitions per ADR-0015 state machine diagram:",
        "  * Initializing -> Ready (success), Failed (error), Closing (shutdown signal)",
        "  * Ready -> Closing (shutdown signal), Failed (crash/panic)",
        "  * Failed -> Closed (direct, bypass Closing)",
        "  * Closing -> Closed (completion or timeout)",
        "- Operation gating: Closing state rejects requests with REQUEST_FAILED (bridge: connection closing)",
        "- Notifications in Closing state should be dropped (writer loop stopped per ADR-0017)",
        "",
        "DEPENDENCIES:",
        "- This PBI provides state machine foundation for pbi-lsp-shutdown (LSP handshake sequence)",
        "- This PBI provides state machine foundation for pbi-global-shutdown-timeout (timeout coordination)",
        "- No external dependencies, purely additive enum change with new state transition logic",
        "",
        "RISKS:",
        "- Low: Enum extension is backward compatible - existing code handles Initializing/Ready/Failed",
        "- State transition validation may need careful testing to prevent invalid transitions",
        "- Need to ensure all call sites that check ConnectionState handle new variants",
      ],
    },
    {
      id: "pbi-lsp-shutdown",
      story: {
        role: "developer using tree-sitter-ls with multiple embedded languages",
        capability: "have downstream language servers shut down with proper LSP handshake",
        benefit: "servers flush buffers, save state, and release locks cleanly on exit",
      },
      acceptance_criteria: [
        { criterion: "Send LSP shutdown request and wait for response", verification: "Integration test: shutdown request sent, response received before exit" },
        { criterion: "Send LSP exit notification after shutdown response", verification: "Integration test: exit notification sent after shutdown response" },
        { criterion: "Three-phase writer loop synchronization", verification: "Unit test: signal stop, wait idle (2s timeout), exclusive access sequence" },
        { criterion: "Failed state bypasses LSP handshake", verification: "Unit test: Failed connections use SIGTERM/SIGKILL only, no LSP messages" },
        { criterion: "SIGTERM then SIGKILL escalation", verification: "Integration test: unresponsive process receives SIGTERM, then SIGKILL" },
      ],
      status: "draft",
    },
    {
      id: "pbi-global-shutdown-timeout",
      story: {
        role: "editor plugin author integrating tree-sitter-ls",
        capability: "have bounded shutdown time for all connections",
        benefit: "editor shutdown is predictable and never hangs indefinitely",
      },
      acceptance_criteria: [
        { criterion: "Global shutdown timeout (5-15s configurable) for entire shutdown", verification: "Unit test: shutdown completes within configured timeout" },
        { criterion: "Parallel shutdown of all connections", verification: "Integration test: multiple servers shut down concurrently under single timeout" },
        { criterion: "SIGTERM to SIGKILL escalation on timeout", verification: "Integration test: hung servers receive SIGTERM then SIGKILL when timeout expires" },
        { criterion: "Writer-idle timeout (2s fixed) within global budget", verification: "Unit test: writer loop given 2s to become idle, counts against global timeout" },
      ],
      status: "draft",
    },
    {
      id: "pbi-liveness-timeout",
      story: {
        role: "developer using tree-sitter-ls with multiple embedded languages",
        capability: "have hung downstream servers detected and recovered",
        benefit: "unresponsive servers do not block LSP features indefinitely",
      },
      acceptance_criteria: [
        { criterion: "Liveness timeout (30-120s configurable) when Ready with pending > 0", verification: "Unit test: timeout starts when pending transitions 0 to 1 in Ready state" },
        { criterion: "Timer resets on stdout activity from server", verification: "Unit test: any stdout data resets the liveness timer" },
        { criterion: "Timer stops when pending count returns to 0", verification: "Unit test: all responses received stops the timer without transition" },
        { criterion: "Ready to Failed transition on liveness timeout", verification: "Unit test: timeout expiry in Ready state triggers Failed transition" },
        { criterion: "Liveness timeout disabled during Closing state", verification: "Unit test: entering Closing state stops liveness timer" },
      ],
      status: "draft",
    },
    {
      id: "pbi-cancellation-forwarding",
      story: {
        role: "developer using tree-sitter-ls with multiple embedded languages",
        capability: "have cancellation requests forwarded to downstream servers",
        benefit: "cancelled operations release server resources and respect client intent",
      },
      acceptance_criteria: [
        { criterion: "Forward $/cancelRequest notification to downstream server", verification: "Integration test: cancel notification reaches downstream server" },
        { criterion: "Keep pending request entry after forwarding cancel", verification: "Unit test: pending map still contains request after cancel forwarded" },
        { criterion: "Forward server response (result or REQUEST_CANCELLED)", verification: "Integration test: server's cancel response (either type) reaches client" },
      ],
      status: "draft",
    },
  ],
  sprint: {
    number: 12,
    pbi_id: "pbi-connection-states",
    goal: "Add Closing and Closed states with valid transitions and operation gating",
    status: "in_progress",
    subtasks: [
      {
        test: "Unit test: ConnectionState enum has all 5 variants (Initializing, Ready, Failed, Closing, Closed)",
        implementation: "Add Closing and Closed variants to ConnectionState enum in pool.rs",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "57e5445e", message: "feat(pool): add Closing and Closed variants to ConnectionState enum", phase: "green" }],
        notes: [],
      },
      {
        test: "Unit test: Ready state + shutdown signal = Closing state (valid transition)",
        implementation: "Update set_state() to allow Ready -> Closing transition",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "0e0a0c65", message: "feat(pool): add try_transition with state machine validation", phase: "green" }],
        notes: [],
      },
      {
        test: "Unit test: Initializing state + shutdown signal = Closing state (valid transition)",
        implementation: "Update set_state() to allow Initializing -> Closing transition",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [],
      },
      {
        test: "Unit test: Closing state completes to Closed (valid transition)",
        implementation: "Update set_state() to allow Closing -> Closed transition",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [],
      },
      {
        test: "Unit test: Failed state transitions directly to Closed, bypassing Closing",
        implementation: "Update set_state() to allow Failed -> Closed transition (direct, no LSP handshake)",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [],
      },
      {
        test: "Unit test: new requests in Closing state receive REQUEST_FAILED error",
        implementation: "Add operation gating for Closing state in request handling that rejects with REQUEST_FAILED",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [],
      },
    ],
  },
  completed: [
    { number: 1, pbi_id: "pbi-document-highlight", goal: "Bridge textDocument/documentHighlight to downstream LS", status: "done", subtasks: [] },
    { number: 2, pbi_id: "pbi-rename", goal: "Bridge textDocument/rename with WorkspaceEdit transformation", status: "done", subtasks: [] },
    { number: 3, pbi_id: "pbi-document-link", goal: "Bridge textDocument/documentLink with range transformation to host coordinates", status: "done", subtasks: [] },
    { number: 4, pbi_id: "pbi-document-symbols", goal: "Bridge textDocument/documentSymbol to downstream LS with coordinate transformation", status: "done", subtasks: [] },
    { number: 5, pbi_id: "pbi-inlay-hints", goal: "Bridge textDocument/inlayHint with bidirectional coordinate transformation", status: "done", subtasks: [] },
    { number: 6, pbi_id: "pbi-color-presentation", goal: "Bridge textDocument/documentColor and textDocument/colorPresentation with coordinate transformation", status: "done", subtasks: [] },
    { number: 7, pbi_id: "pbi-moniker", goal: "Bridge textDocument/moniker with position transformation and pass-through response", status: "done", subtasks: [] },
    { number: 8, pbi_id: "pbi-symbol-info-uri-fix", goal: "Fix SymbolInformation URI transformation for LSP compliance", status: "done", subtasks: [] },
    { number: 9, pbi_id: "pbi-document-color-e2e", goal: "Add E2E test coverage for textDocument/documentColor", status: "done", subtasks: [] },
    { number: 10, pbi_id: "pbi-color-presentation-e2e", goal: "Add E2E test coverage for textDocument/colorPresentation", status: "done", subtasks: [] },
    { number: 11, pbi_id: "pbi-inlay-hint-label-part-location", goal: "Transform InlayHintLabelPart.location for full LSP compliance", status: "done", subtasks: [] },
  ],
  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_e2e" },
      { name: "E2E test exists for bridged features (test infrastructure even if downstream LS returns no data)", run: "verify tests/e2e_lsp_lua_*.rs exists for feature" },
    ],
  },
  retrospectives: [
    { sprint: 11, improvements: [
      { action: "Document pattern for handling LSP types with nested optional Location fields in array properties", timing: "product", status: "active", outcome: null },
      { action: "When changing function signatures, verify all call sites in same commit to maintain atomicity", timing: "sprint", status: "active", outcome: null },
    ] },
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
