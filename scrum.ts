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
      id: "pbi-global-shutdown-timeout",
      story: {
        role: "editor plugin author integrating tree-sitter-ls",
        capability: "have bounded shutdown time for all connections",
        benefit: "editor shutdown is predictable and never hangs indefinitely",
      },
      acceptance_criteria: [
        { criterion: "Global shutdown timeout (5-15s configurable) wraps shutdown_all()", verification: "Unit test: shutdown_all completes within configured timeout even with hung servers" },
        { criterion: "Parallel shutdown of all connections under single global ceiling", verification: "Integration test: multiple servers shut down concurrently, total time bounded by global timeout (not N * per-server)" },
        { criterion: "force_kill_all() called when global timeout expires", verification: "Integration test: all remaining connections receive SIGTERM then SIGKILL when global timeout expires" },
        { criterion: "Writer-idle timeout (2s fixed) counts against global budget", verification: "Unit test: writer idle wait is part of graceful_shutdown(), not additional to global timeout" },
      ],
      status: "ready",
      refinement_notes: [
        "ALREADY IMPLEMENTED: graceful_shutdown() with 5s hardcoded timeout per connection (pool.rs:249)",
        "ALREADY IMPLEMENTED: shutdown_all() runs connections in parallel via JoinSet (pool.rs:697-749)",
        "ALREADY IMPLEMENTED: force_kill_with_escalation() with SIGTERM->SIGKILL (connection.rs:164-265)",
        "ALREADY IMPLEMENTED: Mutex-based writer synchronization (equivalent to writer-idle wait)",
        "TO IMPLEMENT: Wrap shutdown_all() parallel shutdowns in tokio::time::timeout(GLOBAL_TIMEOUT)",
        "TO IMPLEMENT: Add force_kill_all() fallback when global timeout expires per ADR-0017",
        "TO IMPLEMENT: Make global timeout configurable (5-15s range per ADR-0018)",
        "ARCHITECTURE NOTE: Per ADR-0017, no per-connection budget allocation - fast servers complete quickly, slow servers use remaining time",
        "ARCHITECTURE NOTE: Current per-connection 5s timeout should be removed; global timeout is the only ceiling",
      ],
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
    number: 13,
    pbi_id: "pbi-global-shutdown-timeout",
    goal: "Implement global shutdown timeout with configurable ceiling and force-kill fallback",
    status: "review",
    subtasks: [
      // Phase 1: Foundation (configurable timeout type)
      {
        test: "Unit test: GlobalShutdownTimeout type accepts 5-15s range, rejects out-of-range values",
        implementation: "Add GlobalShutdownTimeout newtype with validation in config module",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "b4f667bb", message: "feat(bridge): add GlobalShutdownTimeout newtype with 5-15s validation", phase: "green" }],
        notes: ["ADR-0018: Global Shutdown 5-15s recommended range", "Consider Duration wrapper with From/Into traits"],
      },
      // Phase 2: Core Feature (global timeout wrapper)
      {
        test: "Unit test: shutdown_all completes within configured timeout even with hung servers",
        implementation: "Wrap shutdown_all() parallel shutdowns in tokio::time::timeout(GLOBAL_TIMEOUT)",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "c0e58e62", message: "feat(bridge): add shutdown_all_with_timeout for global shutdown ceiling", phase: "green" }],
        notes: ["ADR-0017: Global timeout overrides all other timeouts", "Use JoinSet with timeout wrapper"],
      },
      {
        test: "Integration test: multiple servers shut down concurrently, total time bounded by global timeout",
        implementation: "Pass GlobalShutdownTimeout to shutdown_all() and enforce single ceiling",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "7e88b266", message: "test(bridge): add integration test for concurrent parallel shutdown", phase: "green" }],
        notes: ["Verify N servers complete in O(1) time, not O(N)", "Test with mock slow servers"],
      },
      // Phase 3: Force-kill fallback
      {
        test: "Unit test: force_kill_all() sends SIGTERM then SIGKILL to all remaining connections",
        implementation: "Add force_kill_all() method to ConnectionPool that iterates connections",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "4155548f", message: "feat(bridge): add force_kill_all() for SIGTERM->SIGKILL escalation", phase: "green" }],
        notes: ["Reuse existing force_kill_with_escalation() per connection", "Unix-only via cfg(unix)"],
      },
      {
        test: "Integration test: all remaining connections receive SIGTERM then SIGKILL when global timeout expires",
        implementation: "Wire force_kill_all() as fallback in shutdown_all() timeout handler",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "23131874", message: "refactor(bridge): use force_kill_all() in shutdown timeout handler", phase: "green" }],
        notes: ["ADR-0017: force_kill_all(connections) on timeout expiry", "Verify process termination"],
      },
      // Phase 4: Cleanup (remove per-connection timeout)
      {
        test: "Unit test: graceful_shutdown() has no internal timeout (relies on global ceiling)",
        implementation: "Remove 5s SHUTDOWN_TIMEOUT constant from graceful_shutdown()",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "aaa2954b", message: "refactor(bridge): remove per-connection SHUTDOWN_TIMEOUT from graceful_shutdown", phase: "green" }],
        notes: ["ADR-0018: Global shutdown is the only ceiling", "Current hardcoded 5s in pool.rs:249"],
      },
      // Phase 5: Robustness (writer-idle budget verification)
      {
        test: "Unit test: writer idle wait (2s) counts against global budget, not additional time",
        implementation: "Verify writer synchronization timeout is within graceful_shutdown() scope",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "b76d5878", message: "test(bridge): verify writer synchronization is within graceful_shutdown scope", phase: "green" }],
        notes: ["ADR-0017: 2s writer-idle counts against global budget", "Already implemented via Mutex-based sync"],
      },
    ],
  },
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
        commits: [{ hash: "b3a8ec61", message: "feat(bridge): add Closing and Closed states to ConnectionState enum", phase: "green" }],
        notes: ["Verify all call sites handle new variants (Sprint 11 retrospective)"],
      },
      {
        test: "Unit test: Ready state + shutdown signal = Closing state",
        implementation: "Add Ready to Closing transition in set_state() with shutdown trigger",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "b3a8ec61", message: "feat(bridge): add Closing and Closed states to ConnectionState enum", phase: "green" }],
        notes: ["ADR-0015 state machine diagram", "Implemented via begin_shutdown() method"],
      },
      {
        test: "Unit test: Initializing state + shutdown signal = Closing state",
        implementation: "Add Initializing to Closing transition for shutdown during init",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "b3a8ec61", message: "feat(bridge): add Closing and Closed states to ConnectionState enum", phase: "green" }],
        notes: ["ADR-0017: abort initialization, proceed to shutdown", "Uses same begin_shutdown() method as Ready transition"],
      },
      {
        test: "Unit test: Closing state completes gracefully or times out to Closed",
        implementation: "Add Closing to Closed transition on completion/timeout",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "b3a8ec61", message: "feat(bridge): add Closing and Closed states to ConnectionState enum", phase: "green" }],
        notes: ["Terminal state for graceful shutdown path", "Implemented via complete_shutdown() method"],
      },
      {
        test: "Unit test: Failed state transitions directly to Closed (bypass Closing)",
        implementation: "Add Failed to Closed direct transition, skip LSP handshake",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "b3a8ec61", message: "feat(bridge): add Closing and Closed states to ConnectionState enum", phase: "green" }],
        notes: ["ADR-0017: stdin unavailable in Failed state", "Uses same complete_shutdown() method, bypasses Closing"],
      },
      {
        test: "Unit test: new requests in Closing state receive REQUEST_FAILED error",
        implementation: "Add operation gating for Closing state in request handling",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "b3a8ec61", message: "feat(bridge): add Closing and Closed states to ConnectionState enum", phase: "green" }],
        notes: ["Error message: 'bridge: connection closing'", "Wired via shutdown_all() in LSP shutdown handler"],
      },
      // Phase 2: LSP Handshake (makes states reachable) - COMPLETED
      {
        test: "Integration test: shutdown request sent, response received before exit",
        implementation: "Implement shutdown() method that sends LSP shutdown request and awaits response",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "f22f2d5e", message: "feat(bridge): implement graceful shutdown with LSP handshake", phase: "green" }],
        notes: ["LSP spec: shutdown request before exit notification", "Implemented graceful_shutdown() with 5s timeout"],
      },
      {
        test: "Integration test: exit notification sent after shutdown response",
        implementation: "Send exit notification after receiving shutdown response",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "f22f2d5e", message: "feat(bridge): implement graceful shutdown with LSP handshake", phase: "green" }],
        notes: ["LSP two-phase shutdown sequence", "Exit notification sent in graceful_shutdown() after shutdown response"],
      },
      {
        test: "Unit test: signal stop, wait idle (2s timeout), exclusive access sequence",
        implementation: "Implement three-phase writer loop synchronization for shutdown",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "9f0901fc", message: "test(bridge): add writer synchronization tests for graceful shutdown", phase: "green" }],
        notes: ["ADR-0017: prevents stdin corruption during concurrent writes", "Current Mutex-based architecture provides equivalent synchronization"],
      },
      // Phase 3: Forced Shutdown (robustness)
      {
        test: "Integration test: unresponsive process receives SIGTERM, then SIGKILL",
        implementation: "Add SIGTERM/SIGKILL escalation for unresponsive servers",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "4492ef88", message: "feat(bridge): add SIGTERM/SIGKILL signal escalation for unresponsive servers", phase: "green" }],
        notes: ["Fallback when LSP handshake times out", "Unix-only via nix crate", "Wired into graceful_shutdown() flow"],
      },
      {
        test: "Integration test: in-flight requests receive REQUEST_FAILED, then shutdown completes",
        implementation: "Fail pending requests on shutdown, then complete LSP handshake",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "9ebc6b4b", message: "test(bridge): add E2E shutdown sequence integration tests", phase: "green" }],
        notes: ["End-to-end shutdown sequence with pending requests", "Verifies operation gating during Closing state"],
      },
    ],
    },
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
