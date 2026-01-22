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
      status: "done",
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
  sprint: null,
  completed: [
    // Sprint 14: 4 phases, 6 acceptance criteria, key commits: eefa609a, 67b9db3d, b2721d65, cfe5cd33
    { number: 14, pbi_id: "pbi-liveness-timeout", goal: "Implement liveness timeout to detect and recover from hung downstream servers", status: "done", subtasks: [] },
    // Sprint 13: 5 phases, 7 subtasks, key commits: b4f667bb, c0e58e62, 7e88b266, 4155548f, 23131874, aaa2954b, b76d5878
    { number: 13, pbi_id: "pbi-global-shutdown-timeout", goal: "Implement global shutdown timeout with configurable ceiling and force-kill fallback", status: "done", subtasks: [] },
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
    { sprint: 14, improvements: [
      { action: "Pattern validated: LivenessTimeout reused GlobalShutdownTimeout newtype pattern; BoundedDuration extraction viable for future", timing: "immediate", status: "completed", outcome: "Sprint 14 reused validation pattern from Sprint 13 without issues." },
      { action: "Timer infrastructure in reader task scales well - select! multiplexing isolates liveness from connection lifecycle", timing: "immediate", status: "completed", outcome: "Clean separation of concerns validated." },
      { action: "Test gap: No integration test for timer reset on stdout activity", timing: "product", status: "active", outcome: null },
      { action: "Documentation: Add Sprint 14 as phased implementation case study to ADR-0013", timing: "product", status: "active", outcome: null },
    ] },
    { sprint: 13, improvements: [
      { action: "ADR-first approach: Document patterns BEFORE implementation, tests verify ADR compliance", timing: "sprint", status: "completed", outcome: "Sprint 14 followed ADR-0014/ADR-0018 precisely." },
      { action: "Document phased implementation pattern (Foundation -> Core -> Robustness)", timing: "product", status: "active", outcome: null },
      { action: "Consider BoundedDuration(min, max) extraction for timeout newtypes", timing: "product", status: "active", outcome: null },
    ] },
    { sprint: 12, improvements: [
      { action: "Document enum variant call site update requirements when adding variants", timing: "sprint", status: "active", outcome: null },
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
