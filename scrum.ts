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
      status: "done",
      refinement_notes: [
        "ADR-0015 Section 5: $/cancelRequest is a notification (no ID), bridge forwards without interception; downstream decides to complete or cancel",
        "Key gap: No upstream-to-downstream ID mapping exists. ResponseRouter tracks downstream IDs only (HashMap<RequestId, oneshot::Sender>)",
        "Implementation approach: Add CancelMap to ResponseRouter or ConnectionHandle that maps upstream_id -> downstream_id for active requests",
        "CancelMap lifecycle: Insert on register_request(), remove on route() or remove(); use DashMap or Mutex<HashMap> for thread-safety",
        "tower-lsp notification handling: $/cancelRequest arrives as tower_lsp::jsonrpc::Request with no ID; implement via LanguageServer trait custom notification or tower Service middleware",
        "Files to modify: (1) src/lsp/bridge/actor/response_router.rs - add cancel_map field and lookup method, (2) src/lsp/bridge/pool/connection_handle.rs - store upstream ID in register_request(), (3) src/lsp/lsp_impl.rs or new notification handler - receive and route $/cancelRequest",
        "Notification forwarding pattern: Follow did_change.rs/did_close.rs - build JSON notification, call handle.writer().await.write_message(&notification)",
        "Build $/cancelRequest notification: json!({\"jsonrpc\": \"2.0\", \"method\": \"$/cancelRequest\", \"params\": {\"id\": downstream_id}})",
        "Keep pending entry: Do NOT call router.remove() after forwarding cancel; server may still respond with result or REQUEST_CANCELLED error (code -32800)",
        "Response routing unchanged: route() already handles both success and error responses; REQUEST_CANCELLED error passes through like any other response",
        "Testing: (1) Unit test CancelMap insert/lookup/remove, (2) Integration test with mock server that honors cancel, (3) Integration test with mock server that ignores cancel (returns result)",
        "Multi-connection consideration (ADR-0016): When fan-out to multiple servers exists, forward cancel to ALL servers that received the original request (future work, not in scope for single-LS-per-language)",
      ],
    },
  ],
  sprint: null,
  completed: [
    // Sprint 15: 3 phases, 6 subtasks, key commits: a874a7d9, 52d7c3d0, a0c16e97, (code quality fixes)
    {
      number: 15,
      pbi_id: "pbi-cancellation-forwarding",
      goal: "Implement $/cancelRequest notification forwarding to downstream servers while preserving pending request entries",
      status: "done",
      subtasks: [
        {
          test: "Unit test that CancelMap stores upstream->downstream mapping on register, returns downstream ID on lookup, removes on route",
          implementation: "Add cancel_map field to ResponseRouter, update register() to accept upstream_id, add lookup_downstream_id() method",
          type: "behavioral",
          status: "completed",
          commits: [{ hash: "a874a7d9", message: "feat(bridge): add CancelMap to ResponseRouter for cancel forwarding", phase: "green" }],
          notes: ["Phase 1: Foundation - Add CancelMap to ResponseRouter", "No refactor needed - retain is O(n) but acceptable for typical request counts"],
        },
        {
          test: "Unit test that register_request() passes upstream ID to ResponseRouter",
          implementation: "Modify ConnectionHandle::register_request() to capture and pass upstream ID",
          type: "behavioral",
          status: "completed",
          commits: [{ hash: "52d7c3d0", message: "feat(bridge): add register_request_with_upstream for cancel forwarding", phase: "green" }],
          notes: ["Phase 1: Foundation - Wire upstream ID through register_request()", "Existing register_request() delegates to new method with None"],
        },
        {
          test: "Integration test that cancel notification reaches downstream server with correct downstream ID",
          implementation: "Add forward_cancel() to LanguageServerPool, route through BridgeCoordinator",
          type: "behavioral",
          status: "completed",
          commits: [{ hash: "a0c16e97", message: "feat(bridge): add forward_cancel to LanguageServerPool and BridgeCoordinator", phase: "green" }],
          notes: ["Phase 2: Core - Forward $/cancelRequest to downstream server"],
        },
        {
          test: "Integration test that Kakehashi receives cancel notification and forwards it",
          implementation: "Add RequestIdCapture middleware with CancelForwarder in main.rs",
          type: "behavioral",
          status: "completed",
          commits: [{ hash: "a0c16e97", message: "feat(bridge): add forward_cancel to LanguageServerPool and BridgeCoordinator", phase: "green" }],
          notes: ["Phase 2: Core - Handle $/cancelRequest in tower-lsp middleware layer", "Implemented via RequestIdCapture in request_id.rs"],
        },
        {
          test: "Unit test that pending map still contains request after cancel forwarded",
          implementation: "Ensure forward_cancel() does NOT call router.remove()",
          type: "behavioral",
          status: "completed",
          commits: [{ hash: "a0c16e97", message: "feat(bridge): add forward_cancel to LanguageServerPool and BridgeCoordinator", phase: "green" }],
          notes: ["Phase 3: Response Handling - Verify pending entry preserved after cancel", "Test: forward_cancel_does_not_remove_pending_entry in pool.rs"],
        },
        {
          test: "Integration test that both normal response and REQUEST_CANCELLED (-32800) reach client",
          implementation: "No changes needed - existing route() handles all response types",
          type: "behavioral",
          status: "completed",
          commits: [],
          notes: ["Phase 3: Response Handling - Verify response forwarding works for cancelled requests", "Test: response_forwarding_still_works_after_cancel_notification in pool.rs"],
        },
      ]
    },
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
