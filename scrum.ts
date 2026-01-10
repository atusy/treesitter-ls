// ============================================================
// Dashboard Data (AI edits this section)
// ============================================================

const userStoryRoles = [
  "Rustacean editing Markdown",
  "developer editing Lua files",
  "documentation author with Rust code blocks",
  "treesitter-ls user managing configurations",
  "developer working with embedded code blocks",
  "developer using language servers via the bridge",
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

  // Completed PBIs: PBI-001-140 (Sprint 1-113), PBI-155-161 (124-130), PBI-178-180a (133-135), PBI-184 (136), PBI-181 (137), PBI-185 (138), PBI-187 (139), PBI-180b (140), PBI-190 (141), PBI-191 (142), PBI-192 (143)
  // Deferred: PBI-091, PBI-107 | Removed: PBI-163-177 | Superseded: PBI-183 | Cancelled: Sprint 139 PBI-180b attempt
  // Sprint 139-143: All sprints DONE (Sprint 143: unit tests + code quality PASSED, E2E test infrastructure issue documented)
  // Phase 1 LSP Bridge PBIs: PBI-301 through PBI-312 (Draft)
  product_backlog: [
    // ============================================================
    // Phase 1: LSP Bridge Implementation (ADR-0013 through ADR-0018)
    // Goal: One downstream LS per language, simple routing, fail-fast
    // ============================================================

    // --- Foundation Layer (ADR-0014: Async Connection) ---
    {
      id: "PBI-301",
      story: {
        role: "developer using language servers via the bridge",
        capability:
          "spawn a downstream language server as an async child process with non-blocking I/O",
        benefit:
          "the bridge can communicate with external language servers without blocking the main event loop",
      },
      acceptance_criteria: [], // Draft - AC to be added during refinement
      status: "draft",
      refinement_notes: [
        "Foundation for all bridge communication",
        "Uses tokio::process::Command for async spawning",
        "Implements AsyncBufReadExt/AsyncWriteExt for stdin/stdout",
        "Must support clean cancellation via select!",
        "Ref: ADR-0014 Process Management section",
      ],
    },
    {
      id: "PBI-302",
      story: {
        role: "developer using language servers via the bridge",
        capability:
          "have pending requests tracked and correlated with responses from downstream servers",
        benefit:
          "multiple concurrent requests can be in-flight without losing response correlation",
      },
      acceptance_criteria: [], // Draft - AC to be added during refinement
      status: "draft",
      refinement_notes: [
        "Implements pending request map with request ID correlation",
        "Uses check-insert-check pattern for race prevention",
        "Cleanup on reader task exit (fail with INTERNAL_ERROR)",
        "Ref: ADR-0014 Pending Request Lifecycle section",
      ],
    },

    // --- Connection State Machine (ADR-0015: Message Ordering) ---
    {
      id: "PBI-303",
      story: {
        role: "developer using language servers via the bridge",
        capability:
          "have a connection state machine tracking server lifecycle (Initializing, Ready, Failed, Closing, Closed)",
        benefit:
          "operations are correctly gated based on server readiness and errors are predictable",
      },
      acceptance_criteria: [], // Draft - AC to be added during refinement
      status: "draft",
      refinement_notes: [
        "Implements ConnectionState enum with all transitions",
        "State-based operation gating (requests require Ready state)",
        "Requests during Initializing return REQUEST_FAILED immediately",
        "Ref: ADR-0015 Connection State Tracking section",
      ],
    },
    {
      id: "PBI-304",
      story: {
        role: "developer using language servers via the bridge",
        capability:
          "have all writes to downstream servers go through a single-writer actor loop",
        benefit:
          "FIFO ordering is guaranteed and protocol corruption from concurrent writes is prevented",
      },
      acceptance_criteria: [], // Draft - AC to be added during refinement
      status: "draft",
      refinement_notes: [
        "Unified order queue (FIFO) for notifications + requests",
        "Bounded queue (256 capacity) with try_send backpressure",
        "Single writer task consumes from queue",
        "Ref: ADR-0015 Single-Writer Loop section",
      ],
    },

    // --- Timeout System (ADR-0014, ADR-0018) ---
    {
      id: "PBI-305",
      story: {
        role: "developer using language servers via the bridge",
        capability:
          "have server initialization bounded by a timeout",
        benefit:
          "slow or hung servers during startup do not block the bridge indefinitely",
      },
      acceptance_criteria: [], // Draft - AC to be added during refinement
      status: "draft",
      refinement_notes: [
        "Initialization timeout (30-60s configurable)",
        "Timer starts when initialize request sent",
        "Stops when initialize response received",
        "Transitions to Failed on timeout",
        "Ref: ADR-0014 Initialization Timeout, ADR-0018 Tier 0",
      ],
    },
    {
      id: "PBI-306",
      story: {
        role: "developer using language servers via the bridge",
        capability:
          "have zombie servers detected via liveness timeout when they stop responding",
        benefit:
          "unresponsive servers are detected and can be replaced without waiting indefinitely",
      },
      acceptance_criteria: [], // Draft - AC to be added during refinement
      status: "draft",
      refinement_notes: [
        "Liveness timeout (30-120s configurable)",
        "Only active in Ready state with pending > 0",
        "Reset on any stdout activity",
        "Stops when entering Closing state",
        "Ref: ADR-0014 Liveness Timeout, ADR-0018 Tier 2",
      ],
    },

    // --- Request/Response Flow (ADR-0015, ADR-0016) ---
    {
      id: "PBI-307",
      story: {
        role: "developer working with embedded code blocks",
        capability:
          "have requests forwarded to the appropriate downstream server based on languageId",
        benefit:
          "each embedded language (Python, Lua, TOML) reaches its correct language server",
      },
      acceptance_criteria: [], // Draft - AC to be added during refinement
      status: "draft",
      refinement_notes: [
        "Simple routing: languageId -> single server",
        "No-provider returns REQUEST_FAILED with clear message",
        "Upstream request IDs used directly (no transformation)",
        "Ref: ADR-0016 Routing Phase 1 section",
      ],
    },
    {
      id: "PBI-308",
      story: {
        role: "developer working with embedded code blocks",
        capability:
          "have cancellation requests forwarded to downstream servers",
        benefit:
          "stale requests can be cancelled according to LSP protocol, freeing server resources",
      },
      acceptance_criteria: [], // Draft - AC to be added during refinement
      status: "draft",
      refinement_notes: [
        "Forward $/cancelRequest to downstream",
        "Keep pending entry (response still expected)",
        "Forward whatever server returns (result or REQUEST_CANCELLED)",
        "Ref: ADR-0015 Cancellation Forwarding section",
      ],
    },

    // --- Document Lifecycle (ADR-0016) ---
    {
      id: "PBI-309",
      story: {
        role: "developer working with embedded code blocks",
        capability:
          "have document lifecycle (didOpen, didChange, didClose) tracked per downstream server",
        benefit:
          "each server receives proper document notifications and the bridge maintains correct state",
      },
      acceptance_criteria: [], // Draft - AC to be added during refinement
      status: "draft",
      refinement_notes: [
        "Per-downstream, per-URI state: Opened | Closed",
        "didOpen only after Ready state (contains current snapshot)",
        "didChange/didSave dropped before didOpen",
        "State cleared on connection termination",
        "Ref: ADR-0016 Per-Downstream Document Lifecycle section",
      ],
    },

    // --- Graceful Shutdown (ADR-0017, ADR-0018) ---
    {
      id: "PBI-310",
      story: {
        role: "developer using language servers via the bridge",
        capability:
          "have downstream servers shut down gracefully with LSP shutdown handshake",
        benefit:
          "servers can flush buffers, save state, and release resources without data loss",
      },
      acceptance_criteria: [], // Draft - AC to be added during refinement
      status: "draft",
      refinement_notes: [
        "Two-tier: graceful (LSP shutdown/exit) then forced (SIGTERM/SIGKILL)",
        "Writer loop synchronization (3-phase: signal, wait idle, exclusive access)",
        "Failed state bypasses LSP handshake (cleanup only)",
        "Ref: ADR-0017 LSP Shutdown Handshake section",
      ],
    },
    {
      id: "PBI-311",
      story: {
        role: "developer using language servers via the bridge",
        capability:
          "have all connections shut down in parallel under a global timeout",
        benefit:
          "shutdown completes in bounded time even with multiple or slow servers",
      },
      acceptance_criteria: [], // Draft - AC to be added during refinement
      status: "draft",
      refinement_notes: [
        "Global shutdown timeout (5-15s) overrides all other timeouts",
        "Parallel shutdown of all connections",
        "Force-kill stragglers when timeout expires",
        "Fail pending operations with REQUEST_FAILED",
        "Ref: ADR-0017 Multi-Connection Shutdown, ADR-0018 Tier 3",
      ],
    },

    // --- Server Pool Management (ADR-0016) ---
    {
      id: "PBI-312",
      story: {
        role: "developer working with embedded code blocks",
        capability:
          "have multiple language servers initialize in parallel without blocking each other",
        benefit:
          "fast servers (lua-ls) can start handling requests while slow servers (rust-analyzer) are still initializing",
      },
      acceptance_criteria: [], // Draft - AC to be added during refinement
      status: "draft",
      refinement_notes: [
        "Parallel initialize requests to all configured servers",
        "No global barrier - each proceeds independently",
        "Partial failure: continue with working servers",
        "didOpen sent after individual server Ready",
        "Ref: ADR-0016 Parallel Initialization section",
      ],
    },
  ],
  sprint: null,
  completed: [],
  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_e2e" },
    ],
  },
  // Retrospectives (recent 4) | Sprints 1-139: git log -- scrum.yaml, scrum.ts
  retrospectives: [],
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
