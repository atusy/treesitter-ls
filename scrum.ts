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
      id: "pbi-liveness-timeout",
      story: {
        role: "developer using tree-sitter-ls with multiple embedded languages",
        capability: "have hung downstream servers detected and recovered",
        benefit: "unresponsive servers do not block LSP features indefinitely",
      },
      acceptance_criteria: [
        { criterion: "LivenessTimeout newtype (30-120s configurable) validates range", verification: "Unit test: LivenessTimeout rejects <30s and >120s, accepts 30-120s inclusive" },
        { criterion: "Liveness timer starts when pending count transitions 0 to 1 in Ready state", verification: "Unit test: first request registration starts timer; timer not running when pending=0" },
        { criterion: "Liveness timer resets on any stdout activity (response or notification)", verification: "Unit test: reader task receiving message resets timer to full duration" },
        { criterion: "Liveness timer stops when pending count returns to 0", verification: "Unit test: last response received (pending 1->0) stops timer without state transition" },
        { criterion: "Ready to Failed transition on liveness timeout expiry", verification: "Unit test: timeout fires while pending>0 triggers Ready->Failed, calls router.fail_all()" },
        { criterion: "Liveness timeout disabled during Closing state (global shutdown overrides)", verification: "Unit test: begin_shutdown() cancels active liveness timer; timer does not start in Closing state" },
      ],
      status: "ready",
      refinement_notes: [
        "ADR-0014: Liveness timeout detects zombie servers (process alive but unresponsive); state-based gating ensures timer only active when Ready with pending>0",
        "ADR-0018: Liveness is Tier 2 timeout (30-120s); Global shutdown (Tier 3) overrides - liveness STOPS when entering Closing state",
        "Pattern: Follow GlobalShutdownTimeout newtype (src/lsp/bridge/pool/shutdown_timeout.rs) - new() with validation, default(), as_duration()",
        "Implementation: Add liveness_timer field to reader task or ConnectionHandle; use tokio::time::sleep with select! for cancellation",
        "Pending count: ResponseRouter.pending_count() exists (#[cfg(test)]); consider making pub(crate) or track count in ConnectionHandle",
        "Timer reset signal: Reader task handle_message() must signal timer reset on ANY message (response OR notification), not just routed responses",
        "State transition: On timeout, set ConnectionState::Failed via handle.set_state(); router.fail_all() ensures pending requests get error responses",
        "Recovery: Failed state triggers SpawnNew action on next request (existing pattern in decide_connection_action())",
        "Testing: Use mock slow server (cat >/dev/null) with short timeout; verify timer reset with periodic stdout activity",
      ],
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
    number: 14,
    pbi_id: "pbi-liveness-timeout",
    goal: "Implement liveness timeout to detect and recover from hung downstream servers",
    status: "in_progress",
    subtasks: [
      // Phase 1: Foundation (LivenessTimeout newtype)
      {
        test: "Unit test: LivenessTimeout type accepts 30-120s range, rejects out-of-range values",
        implementation: "Add LivenessTimeout newtype with validation in pool module (follow GlobalShutdownTimeout pattern)",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["ADR-0018: Liveness Timeout is Tier 2 (30-120s)", "Follow GlobalShutdownTimeout pattern: new(), default(), as_duration()"],
      },
      // Phase 2: Timer Infrastructure (start/stop/reset mechanics)
      {
        test: "Unit test: Liveness timer starts when pending count transitions 0 to 1 in Ready state",
        implementation: "Add liveness_timer field to reader task; start timer on first request registration",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["ADR-0014: Timer starts when pending=0 transitions to pending=1", "Timer not running when pending=0"],
      },
      {
        test: "Unit test: Liveness timer resets on any stdout activity (response or notification)",
        implementation: "Reader task handle_message() signals timer reset on ANY message (response OR notification)",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["ADR-0014: Reset on any stdout activity while active", "Use tokio::time::sleep with select! for cancellation/reset"],
      },
      {
        test: "Unit test: Liveness timer stops when pending count returns to 0",
        implementation: "Stop timer when last response received (pending 1->0) without state transition",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["ADR-0014: Timer stops when pending count returns to 0", "No state transition on timer stop"],
      },
      // Phase 3: State Transitions (Ready->Failed on timeout)
      {
        test: "Unit test: Ready to Failed transition on liveness timeout expiry with router.fail_all()",
        implementation: "On timeout: set ConnectionState::Failed via handle.set_state(); call router.fail_all()",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["ADR-0014: Timeout fires while pending>0 triggers Ready->Failed", "Failed state triggers SpawnNew action on next request"],
      },
      // Phase 4: Shutdown Integration (global shutdown override)
      {
        test: "Unit test: begin_shutdown() cancels active liveness timer; timer does not start in Closing state",
        implementation: "Liveness timer disabled during Closing state (global shutdown overrides per ADR-0018)",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["ADR-0018: Global shutdown (Tier 3) overrides Liveness (Tier 2)", "Liveness STOPS when entering Closing state"],
      },
    ],
  },
  completed: [
    {
      number: 13,
      pbi_id: "pbi-global-shutdown-timeout",
      goal: "Implement global shutdown timeout with configurable ceiling and force-kill fallback",
      status: "done",
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
    { number: 12, pbi_id: "pbi-lsp-shutdown", goal: "Implement connection lifecycle with graceful LSP shutdown handshake", status: "done", subtasks: [] },
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
    { sprint: 13, improvements: [
      { action: "Add CLAUDE.md conventions: Document architecture patterns in ADRs BEFORE implementation, tests verify ADR compliance", timing: "sprint", status: "active", outcome: null },
      { action: "Create project guidelines documentation capturing phased implementation pattern (Foundation -> Core -> Robustness)", timing: "product", status: "active", outcome: null },
      { action: "Consider extraction: GlobalShutdownTimeout validation logic could be generalized as BoundedDuration(min, max) for reuse with init/liveness timeouts", timing: "product", status: "active", outcome: null },
    ] },
    { sprint: 12, improvements: [
      { action: "When adding enum variants, explicitly document whether call sites need updates or existing behavior is correct", timing: "sprint", status: "active", outcome: null },
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
