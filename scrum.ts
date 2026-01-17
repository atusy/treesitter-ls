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
      id: "pbi-lsp-shutdown",
      story: {
        role: "developer using tree-sitter-ls with multiple embedded languages",
        capability: "have proper connection lifecycle with graceful LSP shutdown",
        benefit: "connections shut down gracefully with proper LSP handshake, flushing buffers and releasing locks cleanly",
      },
      acceptance_criteria: [
        // Connection state machine (foundation)
        { criterion: "ConnectionState enum includes Closing and Closed variants", verification: "Unit test verifies enum has all 5 states: Initializing, Ready, Failed, Closing, Closed" },
        { criterion: "Ready to Closing transition on shutdown signal", verification: "Unit test: Ready state + shutdown signal = Closing state" },
        { criterion: "Initializing to Closing transition on shutdown signal", verification: "Unit test: Initializing state + shutdown signal = Closing state" },
        { criterion: "Closing to Closed transition on completion/timeout", verification: "Unit test: Closing state completes gracefully or times out to Closed" },
        { criterion: "Failed to Closed transition (direct, no LSP handshake)", verification: "Unit test: Failed state transitions directly to Closed, bypassing Closing" },
        { criterion: "Operation gating during Closing state rejects new operations", verification: "Unit test: new requests in Closing state receive REQUEST_FAILED error" },
        // LSP shutdown handshake (makes states reachable)
        { criterion: "Send LSP shutdown request and wait for response", verification: "Integration test: shutdown request sent, response received before exit" },
        { criterion: "Send LSP exit notification after shutdown response", verification: "Integration test: exit notification sent after shutdown response" },
        { criterion: "Three-phase writer loop synchronization", verification: "Unit test: signal stop, wait idle (2s timeout), exclusive access sequence" },
        { criterion: "SIGTERM then SIGKILL escalation for unresponsive servers", verification: "Integration test: unresponsive process receives SIGTERM, then SIGKILL" },
        // End-to-end integration
        { criterion: "Complete shutdown sequence with pending requests", verification: "Integration test: in-flight requests receive REQUEST_FAILED error, then LSP shutdown/exit handshake completes, then process terminates" },
      ],
      status: "done",
      refinement_notes: [
        "MERGED: pbi-connection-states + pbi-lsp-shutdown for viable increment",
        "",
        "FILES TO MODIFY:",
        "- src/lsp/bridge/pool.rs: Add Closing and Closed variants to ConnectionState enum (line 38-45)",
        "- src/lsp/bridge/pool.rs: Update set_state() to enforce valid state transitions per ADR-0015/ADR-0017",
        "- src/lsp/bridge/pool.rs: Add operation gating for Closing/Closed states in request handling",
        "- src/lsp/bridge/pool.rs: Implement shutdown() method with LSP handshake sequence",
        "- src/lsp/bridge/text_document/did_change.rs: Update state check to handle Closing/Closed (line 49)",
        "- src/lsp/bridge/text_document/did_close.rs: Update state check to handle Closing/Closed (line 38)",
        "",
        "IMPLEMENTATION PHASES:",
        "Phase 1 - State Machine:",
        "- ConnectionState enum: add Closing, Closed variants",
        "- State transitions per ADR-0015 diagram",
        "- Operation gating: Closing rejects requests with REQUEST_FAILED",
        "",
        "Phase 2 - LSP Handshake:",
        "- shutdown() method sends LSP shutdown request, awaits response",
        "- Send exit notification after shutdown response",
        "- Three-phase writer loop: signal stop, wait idle (2s), exclusive access",
        "",
        "Phase 3 - Forced Shutdown:",
        "- Failed state bypasses LSP handshake, goes directly to Closed",
        "- SIGTERM -> SIGKILL escalation for unresponsive processes",
        "",
        "DEPENDENCIES:",
        "- ADR-0015: Connection State Machine specification",
        "- ADR-0017: Graceful Shutdown Sequence specification",
        "",
        "RISKS:",
        "- State transition validation needs careful testing",
        "- Writer loop synchronization timing is critical",
        "- Need to ensure all ConnectionState call sites handle new variants",
      ],
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
  sprint: null,
  completed: [
    {
      number: 12,
      pbi_id: "pbi-lsp-shutdown",
      goal: "Implement connection lifecycle with graceful LSP shutdown handshake",
      status: "done",
      subtasks: [
      // Phase 1: State Machine (foundation) - COMPLETED
      {
        test: "Unit test: ConnectionState enum has 5 states (Initializing, Ready, Failed, Closing, Closed)",
        implementation: "Add Closing and Closed variants to ConnectionState enum in pool.rs",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "816fd5d3", message: "feat(bridge): add Closing and Closed states to ConnectionState enum", phase: "green" }],
        notes: ["Verify all call sites handle new variants (Sprint 11 retrospective)"],
      },
      {
        test: "Unit test: Ready state + shutdown signal = Closing state",
        implementation: "Add Ready to Closing transition in set_state() with shutdown trigger",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "816fd5d3", message: "feat(bridge): add Closing and Closed states to ConnectionState enum", phase: "green" }],
        notes: ["ADR-0015 state machine diagram", "Implemented via begin_shutdown() method"],
      },
      {
        test: "Unit test: Initializing state + shutdown signal = Closing state",
        implementation: "Add Initializing to Closing transition for shutdown during init",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "816fd5d3", message: "feat(bridge): add Closing and Closed states to ConnectionState enum", phase: "green" }],
        notes: ["ADR-0017: abort initialization, proceed to shutdown", "Uses same begin_shutdown() method as Ready transition"],
      },
      {
        test: "Unit test: Closing state completes gracefully or times out to Closed",
        implementation: "Add Closing to Closed transition on completion/timeout",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "816fd5d3", message: "feat(bridge): add Closing and Closed states to ConnectionState enum", phase: "green" }],
        notes: ["Terminal state for graceful shutdown path", "Implemented via complete_shutdown() method"],
      },
      {
        test: "Unit test: Failed state transitions directly to Closed (bypass Closing)",
        implementation: "Add Failed to Closed direct transition, skip LSP handshake",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "816fd5d3", message: "feat(bridge): add Closing and Closed states to ConnectionState enum", phase: "green" }],
        notes: ["ADR-0017: stdin unavailable in Failed state", "Uses same complete_shutdown() method, bypasses Closing"],
      },
      {
        test: "Unit test: new requests in Closing state receive REQUEST_FAILED error",
        implementation: "Add operation gating for Closing state in request handling",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "816fd5d3", message: "feat(bridge): add Closing and Closed states to ConnectionState enum", phase: "green" }],
        notes: ["Error message: 'bridge: connection closing'", "Wired via shutdown_all() in LSP shutdown handler"],
      },
      // Phase 2: LSP Handshake (makes states reachable) - COMPLETED
      {
        test: "Integration test: shutdown request sent, response received before exit",
        implementation: "Implement shutdown() method that sends LSP shutdown request and awaits response",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "65235928", message: "feat(bridge): implement graceful shutdown with LSP handshake", phase: "green" }],
        notes: ["LSP spec: shutdown request before exit notification", "Implemented graceful_shutdown() with 5s timeout"],
      },
      {
        test: "Integration test: exit notification sent after shutdown response",
        implementation: "Send exit notification after receiving shutdown response",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "65235928", message: "feat(bridge): implement graceful shutdown with LSP handshake", phase: "green" }],
        notes: ["LSP two-phase shutdown sequence", "Exit notification sent in graceful_shutdown() after shutdown response"],
      },
      {
        test: "Unit test: signal stop, wait idle (2s timeout), exclusive access sequence",
        implementation: "Implement three-phase writer loop synchronization for shutdown",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "fb3c99b3", message: "test(bridge): add writer synchronization tests for graceful shutdown", phase: "green" }],
        notes: ["ADR-0017: prevents stdin corruption during concurrent writes", "Current Mutex-based architecture provides equivalent synchronization"],
      },
      // Phase 3: Forced Shutdown (robustness)
      {
        test: "Integration test: unresponsive process receives SIGTERM, then SIGKILL",
        implementation: "Add SIGTERM/SIGKILL escalation for unresponsive servers",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "1705df83", message: "feat(bridge): add SIGTERM/SIGKILL signal escalation for unresponsive servers", phase: "green" }],
        notes: ["Fallback when LSP handshake times out", "Unix-only via nix crate", "Wired into graceful_shutdown() flow"],
      },
      {
        test: "Integration test: in-flight requests receive REQUEST_FAILED, then shutdown completes",
        implementation: "Fail pending requests on shutdown, then complete LSP handshake",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "7802fede", message: "test(bridge): add E2E shutdown sequence integration tests", phase: "green" }],
        notes: ["End-to-end shutdown sequence with pending requests", "Verifies operation gating during Closing state"],
      },
    ],
    },
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
    { sprint: 12, improvements: [
      { action: "Document pattern for phased feature implementation (Foundation -> Core -> Robustness) in project guidelines", timing: "product", status: "active", outcome: null },
      { action: "When adding enum variants, explicitly document whether call sites need updates or existing behavior is correct", timing: "sprint", status: "active", outcome: null },
      { action: "Document pattern for platform-conditional feature implementation (Unix-only signals, etc.)", timing: "product", status: "active", outcome: null },
    ] },
    { sprint: 11, improvements: [
      { action: "Document pattern for handling LSP types with nested optional Location fields in array properties", timing: "product", status: "active", outcome: null },
      { action: "When changing function signatures, verify all call sites in same commit to maintain atomicity", timing: "sprint", status: "completed", outcome: "Applied in Sprint 12: subtask notes referenced this action, call sites for ConnectionState enum variants were verified for correct operation gating behavior" },
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
