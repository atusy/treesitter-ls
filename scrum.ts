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

  // Completed: PBI-001-304 (1-147) | Walking Skeleton complete! | Deferred: PBI-091, PBI-107
  // Remaining: PBI-305-314 (ADR compliance gaps from review)
  // Priority order: P0=PBI-306 (timeout), P1=PBI-307,308,309,310 (state/shutdown/cancel/notify), P2=PBI-311,312,313 (actor), P3=PBI-314 (config)
  product_backlog: [

    // --- PBI-306: Timeout Protection for Async Bridge Loops (P0 - Critical) ---
    // ADR Compliance: ADR-0014 (Async Connection), ADR-0018 (Timeout Hierarchy)
    // Addresses: 3 infinite loops in pool.rs that can hang treesitter-ls indefinitely
    {
      id: "PBI-306",
      story: {
        role: "Lua developer editing markdown",
        capability:
          "have LSP bridge requests timeout gracefully instead of hanging forever",
        benefit:
          "treesitter-ls remains responsive even when downstream servers hang or crash",
      },
      acceptance_criteria: [
        {
          criterion:
            "Given server initialization hangs, when 60 seconds elapse without initialize response, then get_or_create_connection returns io::Error with timeout message",
          verification:
            "Unit test: mock server that never responds to initialize; verify timeout error after 60s (use tokio::time::pause for fast test)",
        },
        {
          criterion:
            "Given hover request hangs, when 30 seconds elapse without matching response, then send_hover_request returns io::Error with timeout message",
          verification:
            "Unit test: mock server that never responds to hover; verify timeout error after 30s",
        },
        {
          criterion:
            "Given completion request hangs, when 30 seconds elapse without matching response, then send_completion_request returns io::Error with timeout message",
          verification:
            "Unit test: mock server that never responds to completion; verify timeout error after 30s",
        },
        {
          criterion:
            "Given normal server response within timeout, when request completes successfully, then behavior is unchanged from current implementation",
          verification:
            "Existing E2E tests continue to pass (make test_e2e)",
        },
      ],
      status: "ready",
      refinement_notes: [
        "P0 Critical: These loops can hang treesitter-ls indefinitely if downstream server hangs",
        "Minimal fix: wrap each loop with tokio::time::timeout",
        "Timeout values per ADR-0018: Init 60s (Tier 0), Request 30s (Liveness tier approximation)",
        "Location: src/lsp/bridge/pool.rs lines 154-167, 249-258, 338-350",
        "Phase 1 scope: Simple timeout wrapping only; no actor pattern or graceful shutdown yet",
      ],
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

    // --- PBI-307: Complete State Machine (P1 - ADR-0015, ADR-0017) ---
    // Adds Closing and Closed states to ConnectionState enum per ADR-0015
    {
      id: "PBI-307",
      story: {
        role: "Lua developer editing markdown",
        capability:
          "have LSP bridge connections track their full lifecycle including shutdown states",
        benefit:
          "treesitter-ls can gracefully handle shutdown and report accurate connection status",
      },
      acceptance_criteria: [
        {
          criterion:
            "Given ConnectionState enum, when reviewed against ADR-0015, then it includes Closing and Closed variants in addition to Initializing, Ready, Failed",
          verification:
            "Unit test: ConnectionState enum has 5 variants with correct discriminant values for atomic storage",
        },
        {
          criterion:
            "Given BridgeError::for_state(), when called with Closing state, then it returns REQUEST_FAILED with message 'bridge: connection closing'",
          verification:
            "Unit test: BridgeError::for_state(Closing) returns error with code -32803 and correct message",
        },
        {
          criterion:
            "Given BridgeError::for_state(), when called with Closed state, then it returns REQUEST_FAILED with message 'bridge: connection closed'",
          verification:
            "Unit test: BridgeError::for_state(Closed) returns error with code -32803 and correct message",
        },
        {
          criterion:
            "Given StatefulBridgeConnection, when transitioning states, then valid transitions per ADR-0015 state machine are supported: Ready->Closing, Initializing->Closing, Closing->Closed, Failed->Closed",
          verification:
            "Unit tests: set_closing() and set_closed() methods work from valid source states",
        },
      ],
      status: "ready",
      refinement_notes: [
        "P1 Priority: Foundation for graceful shutdown (PBI-308)",
        "Location: src/lsp/bridge/connection.rs lines 27-55",
        "Current: Only Initializing, Ready, Failed (3 states)",
        "Target: Add Closing, Closed per ADR-0015 state machine diagram",
        "State transitions per ADR-0015: Ready->Closing (shutdown), Failed->Closed (direct), Closing->Closed",
        "Update to_u8/from_u8 for atomic storage (Closing=3, Closed=4)",
      ],
    },

    // --- PBI-308: Graceful Shutdown (P1 - ADR-0017) ---
    // Implements LSP shutdown/exit handshake before killing process
    {
      id: "PBI-308",
      story: {
        role: "Lua developer editing markdown",
        capability:
          "have downstream language servers shut down cleanly when treesitter-ls exits",
        benefit:
          "language servers can flush buffers, save state, and release resources properly",
      },
      acceptance_criteria: [
        {
          criterion:
            "Given a Ready connection, when shutdown is initiated, then LSP shutdown request is sent before exit notification",
          verification:
            "Integration test: mock server receives shutdown request followed by exit notification in correct order",
        },
        {
          criterion:
            "Given shutdown sequence, when shutdown response is received, then exit notification is sent and process termination awaited",
          verification:
            "Integration test: verify exit notification sent only after shutdown response received",
        },
        {
          criterion:
            "Given global shutdown timeout (5-15s per ADR-0018), when timeout expires before graceful completion, then SIGTERM followed by SIGKILL is sent",
          verification:
            "Unit test with tokio::time::pause: mock slow server; verify SIGTERM/SIGKILL escalation after timeout",
        },
        {
          criterion:
            "Given a Failed connection, when shutdown is initiated, then LSP handshake is skipped and process cleanup (SIGTERM/SIGKILL) proceeds directly",
          verification:
            "Unit test: Failed state connection goes directly to process cleanup without sending shutdown/exit",
        },
        {
          criterion:
            "Given multiple connections, when shutdown is initiated, then all connections shut down in parallel within global timeout",
          verification:
            "Integration test: 3 mock servers shut down concurrently; total time < global timeout",
        },
      ],
      status: "draft",
      refinement_notes: [
        "P1 Priority: Production readiness - proper LSP compliance on exit",
        "Depends on PBI-307 (Closing/Closed states)",
        "Per ADR-0017: Two-tier shutdown (graceful then forced)",
        "Per ADR-0018: Global shutdown timeout 5-15s (implementation-defined)",
        "Currently: Drop trait only calls start_kill() (SIGTERM) - no LSP handshake",
        "Location: connection.rs Drop impl, new shutdown module",
        "Phase 1 scope: Single-connection shutdown; multi-connection parallel in Phase 2",
      ],
    },

    // --- PBI-309: Cancellation Forwarding (P1 - ADR-0015, ADR-0016) ---
    // Handle upstream $/cancelRequest and forward to downstream servers
    {
      id: "PBI-309",
      story: {
        role: "Lua developer editing markdown",
        capability:
          "have cancellation requests forwarded to downstream language servers",
        benefit:
          "downstream servers can stop processing cancelled requests, reducing wasted computation",
      },
      acceptance_criteria: [
        {
          criterion:
            "Given upstream $/cancelRequest notification with request ID, when received by bridge, then $/cancelRequest is forwarded to downstream server handling that request",
          verification:
            "Integration test: send cancelRequest; verify downstream server receives it",
        },
        {
          criterion:
            "Given pending request tracking, when response received for cancelled request, then response is forwarded to upstream normally",
          verification:
            "Integration test: cancel request; server completes anyway; verify result returned to client",
        },
        {
          criterion:
            "Given cancelled request, when server returns REQUEST_CANCELLED error, then error is forwarded to upstream client",
          verification:
            "Integration test: cancel request; server honors cancellation; verify error code -32800 returned",
        },
        {
          criterion:
            "Given request not found in pending map, when cancelRequest received, then notification is silently dropped",
          verification:
            "Unit test: cancelRequest for unknown ID is logged at debug level and ignored",
        },
      ],
      status: "draft",
      refinement_notes: [
        "P1 Priority: LSP compliance - standard cancellation flow",
        "Per ADR-0015 section 5: Forward $/cancelRequest to downstream",
        "Per ADR-0016: Router forwards to all connections that received original request (Phase 3 multi-LS)",
        "Phase 1 scope: Single-server cancellation forwarding",
        "Requires pending request tracking (HashMap<RequestId, ResponseChannel>)",
        "Bridge stays thin: just forward, don't intercept",
      ],
    },

    // --- PBI-310: Notification Pass-Through (P1 - ADR-0016) ---
    // Route publishDiagnostics and other notifications from downstream to host client
    {
      id: "PBI-310",
      story: {
        role: "Lua developer editing markdown",
        capability:
          "receive diagnostics and progress notifications from downstream language servers",
        benefit:
          "I see errors, warnings, and progress indicators from language servers in my editor",
      },
      acceptance_criteria: [
        {
          criterion:
            "Given downstream server sends publishDiagnostics, when bridge receives it, then notification is transformed (virtual URI -> host URI) and forwarded to upstream client",
          verification:
            "E2E test: introduce syntax error in Lua block; verify diagnostic appears in editor",
        },
        {
          criterion:
            "Given downstream sends window/logMessage, when bridge receives it, then notification is forwarded to upstream unchanged",
          verification:
            "Integration test: downstream sends logMessage; verify upstream receives it",
        },
        {
          criterion:
            "Given downstream sends $/progress, when bridge receives it, then notification is forwarded to upstream",
          verification:
            "Integration test: downstream sends progress; verify upstream receives it",
        },
        {
          criterion:
            "Given downstream sends window/showMessage, when bridge receives it, then notification is forwarded to upstream",
          verification:
            "Integration test: downstream sends showMessage; verify upstream receives it",
        },
        {
          criterion:
            "Given URI transformation for diagnostics, when virtual URI is transformed, then host document URI with correct position mapping is returned",
          verification:
            "Unit test: virtual URI 'treesitter-ls://lua/file.md' transforms to 'file:///path/to/file.md' with position offset applied",
        },
      ],
      status: "draft",
      refinement_notes: [
        "P1 Priority: Essential for usable LSP experience",
        "Per ADR-0016: Pass-through notifications without aggregation",
        "publishDiagnostics requires URI transformation (virtual -> host)",
        "Position mapping: virtual document positions -> host document positions",
        "Other notifications (logMessage, showMessage, progress) forward as-is",
        "Requires reader task to handle incoming notifications from downstream",
        "Location: pool.rs reader loop, new notification routing module",
      ],
    },

    // --- PBI-311: Single-Writer Actor Loop (P2 - ADR-0015) ---
    // Serialize all stdin writes through bounded mpsc channel
    {
      id: "PBI-311",
      story: {
        role: "Lua developer editing markdown",
        capability:
          "have concurrent LSP requests handled without protocol corruption",
        benefit:
          "treesitter-ls remains stable under high request load without message interleaving",
      },
      acceptance_criteria: [
        {
          criterion:
            "Given bounded mpsc channel with capacity 256, when operations are enqueued, then they are dequeued and written to stdin in FIFO order",
          verification:
            "Unit test: enqueue 10 operations concurrently; verify writes to mock stdin are serialized in order",
        },
        {
          criterion:
            "Given single writer task, when multiple async tasks send operations, then no byte-level interleaving occurs on stdin",
          verification:
            "Integration test: spawn 5 concurrent requests; verify each LSP message is complete and parseable",
        },
        {
          criterion:
            "Given actor loop, when request is sent, then pending_requests map is updated with response channel before write",
          verification:
            "Unit test: send request through actor; verify entry exists in pending_requests before write completes",
        },
        {
          criterion:
            "Given actor loop shutdown, when stop signal received, then current write completes before loop exits",
          verification:
            "Unit test: signal stop mid-write; verify write completes and loop exits cleanly",
        },
      ],
      status: "draft",
      refinement_notes: [
        "P2 Priority: Robustness under concurrent load",
        "Per ADR-0015 section 1: Single-writer loop prevents protocol corruption",
        "Bounded channel capacity 256 per ADR-0015",
        "Actor pattern: dequeue from order_queue, write to stdin, track pending",
        "Location: New actor module or extend pool.rs",
        "Foundation for graceful shutdown (writer-idle synchronization per ADR-0017)",
      ],
    },

    // --- PBI-312: Reader Task with select! (P2 - ADR-0014) ---
    // Spawn dedicated async reader task per connection
    {
      id: "PBI-312",
      story: {
        role: "Lua developer editing markdown",
        capability:
          "have LSP responses routed correctly with proper timeout and shutdown handling",
        benefit:
          "treesitter-ls responds promptly and shuts down cleanly without orphaned tasks",
      },
      acceptance_criteria: [
        {
          criterion:
            "Given dedicated reader task, when response received from downstream, then response is routed to correct pending request via oneshot channel",
          verification:
            "Unit test: send request; inject response; verify caller receives response via oneshot",
        },
        {
          criterion:
            "Given reader task select! loop, when shutdown signal received, then reader exits cleanly",
          verification:
            "Unit test: spawn reader; send shutdown signal; verify task completes without panic",
        },
        {
          criterion:
            "Given reader task select! loop, when liveness timeout fires, then connection transitions to Failed state",
          verification:
            "Unit test with tokio::time::pause: no response for 30s; verify Failed state transition",
        },
        {
          criterion:
            "Given reader task with CancellationToken, when writer panics, then reader exits after token is cancelled",
          verification:
            "Unit test: simulate writer panic; verify reader receives cancellation and exits",
        },
      ],
      status: "draft",
      refinement_notes: [
        "P2 Priority: Foundation for robust async operation",
        "Per ADR-0014: select! multiplexes read, shutdown, timeout",
        "Per ADR-0015 section 6: CancellationToken for cross-task panic propagation",
        "Route responses via oneshot channels in pending_requests map",
        "Location: New reader task in connection.rs or separate module",
        "Complements writer actor (PBI-311) for complete async architecture",
      ],
    },

    // --- PBI-313: Non-Blocking Backpressure (P2 - ADR-0015) ---
    // Use try_send() on bounded queue with graceful degradation
    {
      id: "PBI-313",
      story: {
        role: "Lua developer editing markdown",
        capability:
          "have the bridge handle queue overflow gracefully without blocking or deadlock",
        benefit:
          "treesitter-ls remains responsive even under extreme load conditions",
      },
      acceptance_criteria: [
        {
          criterion:
            "Given queue is full (256 operations), when notification enqueued via try_send(), then notification is dropped with WARN log",
          verification:
            "Unit test: fill queue to capacity; enqueue didChange; verify WARN log and operation dropped",
        },
        {
          criterion:
            "Given queue is full, when request enqueued via try_send(), then REQUEST_FAILED error is returned immediately",
          verification:
            "Unit test: fill queue; send hover request; verify REQUEST_FAILED (-32803) returned",
        },
        {
          criterion:
            "Given notification drop, when logged, then log includes URI, method, and queue depth",
          verification:
            "Unit test: verify log message contains relevant debugging information",
        },
        {
          criterion:
            "Given queue has capacity, when operation enqueued, then operation proceeds normally",
          verification:
            "Unit test: queue with space; enqueue operation; verify success",
        },
      ],
      status: "draft",
      refinement_notes: [
        "P2 Priority: Prevents deadlock under backpressure",
        "Per ADR-0015 section 3: Non-blocking backpressure with try_send()",
        "Notifications: DROP with telemetry (WARN log)",
        "Requests: Return REQUEST_FAILED immediately",
        "Depends on PBI-311 (actor loop with bounded channel)",
        "Phase 2 extension: $/telemetry events for monitoring",
      ],
    },

    // --- PBI-314: Timeout Configuration (P3 - ADR-0018) ---
    // Add timeout fields to BridgeServerConfig for per-server customization
    {
      id: "PBI-314",
      story: {
        role: "Lua developer editing markdown",
        capability:
          "configure timeout values per language server",
        benefit:
          "I can tune timeouts for slow servers (e.g., rust-analyzer) or fast servers (e.g., lua-ls)",
      },
      acceptance_criteria: [
        {
          criterion:
            "Given BridgeServerConfig, when parsed from YAML, then initialization_timeout field is recognized",
          verification:
            "Unit test: parse config with initialization_timeout: 90s; verify value is 90 seconds",
        },
        {
          criterion:
            "Given BridgeServerConfig, when parsed from YAML, then liveness_timeout field is recognized",
          verification:
            "Unit test: parse config with liveness_timeout: 60s; verify value is 60 seconds",
        },
        {
          criterion:
            "Given BridgeServerConfig, when parsed from YAML, then shutdown_timeout field is recognized",
          verification:
            "Unit test: parse config with shutdown_timeout: 10s; verify value is 10 seconds",
        },
        {
          criterion:
            "Given timeout fields not specified, when config loaded, then ADR-0018 defaults are used (init: 60s, liveness: 30s, shutdown: 10s)",
          verification:
            "Unit test: parse minimal config; verify default timeout values applied",
        },
        {
          criterion:
            "Given per-server timeout config, when connection is created, then connection uses server-specific timeout values",
          verification:
            "Integration test: configure lua-ls with 30s init timeout; verify timeout applied during initialization",
        },
      ],
      status: "draft",
      refinement_notes: [
        "P3 Priority: Polish - allows per-server tuning",
        "Per ADR-0018: initialization_timeout, liveness_timeout, shutdown_timeout",
        "Recommended defaults: init 60s, liveness 30-120s, shutdown 5-15s",
        "Location: config.rs BridgeServerConfig struct",
        "Consider Duration type with human-readable parsing (e.g., '30s', '2m')",
        "Depends on PBI-306 (timeout protection) being implemented first",
      ],
    },
  ],
  sprint: null,
  // Sprint 147 (PBI-304): 8 subtasks, commits: 34c5c4c7, 9d563274, eccf6c09, b5ed4458, efbdabf9
  // Sprint 146 (PBI-303): 8/10 subtasks, commits: 55abb5e0..b5588476 (E2E blocked by lua-ls)
  // Sprint 145 (PBI-302): 9 subtasks | Sprint 144 (PBI-301): 7 subtasks
  completed: [],
  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_e2e" },
    ],
  },
  // Retrospectives: 147 (E2E success, state integration), 146 (E2E early, distribute tests), 145 (BrokenPipe)
  retrospectives: [
    { sprint: 147, improvements: [
      { action: "Mark Sprint 146 'E2E early' action as completed", timing: "immediate", status: "completed", outcome: "E2E tests ran early in Sprint 147; non-blocking behavior verified" },
      { action: "Integrate set_ready()/set_failed() state transitions in BridgeManager initialization flow", timing: "sprint", status: "active", outcome: null },
      { action: "Investigate BrokenPipe E2E issue (unresolved for 2 sprints)", timing: "product", status: "active", outcome: null },
    ]},
    { sprint: 146, improvements: [
      { action: "Run E2E smoke test early when involving external LS", timing: "sprint", status: "completed", outcome: "Applied in Sprint 147; E2E tests ran early and passed" },
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
