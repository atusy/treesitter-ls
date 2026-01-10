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
  // Removed: PBI-305 (obsolete - lua-ls completion issue fixed in 5a91bebb)
  // Consolidated: PBI-307+308 -> PBI-307, PBI-311+312+313 -> PBI-311
  // Remaining: PBI-306, PBI-307, PBI-309, PBI-310, PBI-311, PBI-314 (6 PBIs, user-focused)
  // Priority: P0=PBI-306, P1=PBI-307,309,310, P2=PBI-311, P3=PBI-314
  product_backlog: [

    // --- PBI-306: Responsive LSP Even When Servers Hang (P0 - Critical) ---
    {
      id: "PBI-306",
      story: {
        role: "Lua developer editing markdown",
        capability:
          "continue editing even when a language server stops responding",
        benefit:
          "my editor never freezes waiting for a stuck language server",
      },
      acceptance_criteria: [
        {
          criterion:
            "Given a language server that hangs during startup, when I open a file, then I see an error message within 60 seconds instead of my editor freezing",
          verification:
            "Manual test: Start treesitter-ls with mock server that never responds; verify error appears within 60s and editor remains usable",
        },
        {
          criterion:
            "Given a language server that hangs on hover request, when I hover over code, then I see an error or empty result within 30 seconds instead of indefinite wait",
          verification:
            "Manual test: Hover over Lua code with mock server that hangs; verify response within 30s",
        },
        {
          criterion:
            "Given a language server that hangs on completion request, when I trigger completion, then I see an error or empty result within 30 seconds",
          verification:
            "Manual test: Trigger completion with mock server that hangs; verify response within 30s",
        },
        {
          criterion:
            "Given a normally functioning language server, when I use LSP features, then behavior is unchanged from before",
          verification:
            "E2E tests continue to pass: make test_e2e",
        },
      ],
      status: "ready",
      refinement_notes: [
        "TECHNICAL: Wrap loops in pool.rs with tokio::time::timeout",
        "TECHNICAL: Location: src/lsp/bridge/pool.rs lines 154-167, 249-258, 338-350",
        "TECHNICAL: Timeout values per ADR-0018: Init 60s (Tier 0), Request 30s (Liveness tier)",
        "ADR Compliance: ADR-0014 (Async Connection), ADR-0018 (Timeout Hierarchy)",
        "Phase 1 scope: Simple timeout wrapping only; actor pattern in later PBIs",
      ],
    },

    // --- PBI-307: Clean Shutdown Without Orphaned Processes (P1) ---
    // Consolidated from: PBI-307 (State Machine) + PBI-308 (Graceful Shutdown)
    {
      id: "PBI-307",
      story: {
        role: "Lua developer editing markdown",
        capability:
          "have language servers shut down cleanly when I close my editor",
        benefit:
          "no orphaned lua-language-server processes accumulate on my system",
      },
      acceptance_criteria: [
        {
          criterion:
            "Given I close my editor normally, when I check running processes, then no orphaned lua-language-server processes remain",
          verification:
            "Manual test: Open file with Lua code, wait for LSP features, close editor, run 'pgrep lua-language-server' and verify no processes",
        },
        {
          criterion:
            "Given I force-quit my editor, when shutdown timeout (15s) expires, then language servers are forcefully terminated",
          verification:
            "Manual test: Force-quit editor, wait 15s, verify no orphaned processes with 'pgrep'",
        },
        {
          criterion:
            "Given a language server in error state, when I close my editor, then the process is cleaned up without hanging",
          verification:
            "Manual test: Simulate server error, close editor, verify cleanup within 5s",
        },
        {
          criterion:
            "Given multiple language servers running, when I close my editor, then all are shut down within the global timeout",
          verification:
            "Manual test: Open files using multiple language servers, close editor, verify all cleaned up",
        },
      ],
      status: "ready",
      refinement_notes: [
        "TECHNICAL: Add Closing and Closed states to ConnectionState enum (ADR-0015)",
        "TECHNICAL: Location: src/lsp/bridge/connection.rs lines 27-55",
        "TECHNICAL: Implement LSP shutdown/exit handshake before killing process",
        "TECHNICAL: Two-tier shutdown per ADR-0017: graceful (shutdown/exit) then forced (SIGTERM/SIGKILL)",
        "TECHNICAL: Global shutdown timeout 5-15s per ADR-0018",
        "TECHNICAL: State transitions: Ready->Closing (shutdown), Failed->Closed (direct), Closing->Closed",
        "ADR Compliance: ADR-0015 (State Machine), ADR-0017 (Graceful Shutdown)",
        "Consolidates former PBI-307 (State Machine) and PBI-308 (Graceful Shutdown)",
      ],
    },

    // --- PBI-309: Cancel Slow Operations (P1) ---
    {
      id: "PBI-309",
      story: {
        role: "Lua developer editing markdown",
        capability:
          "cancel LSP operations that are taking too long",
        benefit:
          "I can abort slow operations and continue working without waiting",
      },
      acceptance_criteria: [
        {
          criterion:
            "Given I start typing while a completion request is pending, when the editor sends a cancellation, then the language server stops processing the old request",
          verification:
            "Manual test: Trigger slow completion, type more characters; verify no stale completions appear",
        },
        {
          criterion:
            "Given I cancel a request, when the language server finishes anyway, then I see the result (cancellation is best-effort)",
          verification:
            "Integration test: Cancel request; server completes anyway; verify result returned to client",
        },
        {
          criterion:
            "Given I cancel a request, when the language server honors the cancellation, then I see a 'request cancelled' indication (not an error)",
          verification:
            "Integration test: Cancel request; server honors it; verify no error shown to user",
        },
      ],
      status: "draft",
      refinement_notes: [
        "TECHNICAL: Handle upstream $/cancelRequest and forward to downstream servers",
        "TECHNICAL: Per ADR-0015 section 5: Forward $/cancelRequest to downstream",
        "TECHNICAL: Requires pending request tracking (HashMap<RequestId, ResponseChannel>)",
        "TECHNICAL: Bridge stays thin: just forward, don't intercept",
        "ADR Compliance: ADR-0015 (Cancellation), ADR-0016 (Router)",
        "Phase 1 scope: Single-server cancellation; multi-LS in Phase 3",
      ],
    },

    // --- PBI-310: See Errors and Progress from Language Servers (P1) ---
    {
      id: "PBI-310",
      story: {
        role: "Lua developer editing markdown",
        capability:
          "see diagnostics, errors, and progress indicators from language servers",
        benefit:
          "I know about syntax errors, warnings, and server activity in my Lua code blocks",
      },
      acceptance_criteria: [
        {
          criterion:
            "Given I introduce a syntax error in a Lua code block, when the language server detects it, then I see the error squiggle in my editor",
          verification:
            "E2E test: Introduce syntax error in Lua block; verify diagnostic appears in editor",
        },
        {
          criterion:
            "Given the language server is indexing files, when it sends progress updates, then I see a progress indicator in my editor",
          verification:
            "Manual test: Open large project; verify progress indicator appears during indexing",
        },
        {
          criterion:
            "Given the language server logs a message, when it's important (warning/error), then I can see it in my editor's log",
          verification:
            "Manual test: Trigger server warning; verify message visible in editor output panel",
        },
        {
          criterion:
            "Given diagnostics are on a specific line in the Lua block, when shown in the editor, then the line number correctly maps to the markdown file",
          verification:
            "E2E test: Error on line 3 of Lua block maps to correct line in markdown document",
        },
      ],
      status: "draft",
      refinement_notes: [
        "TECHNICAL: Route publishDiagnostics from downstream to host client with URI transformation",
        "TECHNICAL: Transform virtual URI (treesitter-ls://lua/file.md) to host URI (file:///path/to/file.md)",
        "TECHNICAL: Apply position offset for embedded code blocks",
        "TECHNICAL: Forward window/logMessage, window/showMessage, $/progress as-is",
        "TECHNICAL: Requires reader task to handle incoming notifications from downstream",
        "TECHNICAL: Location: pool.rs reader loop, new notification routing module",
        "ADR Compliance: ADR-0016 (Notification Pass-Through)",
      ],
    },

    // --- PBI-311: Reliable Operation Under Heavy Load (P2) ---
    // Consolidated from: PBI-311 (Actor Loop) + PBI-312 (Reader Task) + PBI-313 (Backpressure)
    {
      id: "PBI-311",
      story: {
        role: "Lua developer editing markdown",
        capability:
          "have reliable LSP features even when editing rapidly or working with large files",
        benefit:
          "my editor stays responsive and stable regardless of my editing speed or project size",
      },
      acceptance_criteria: [
        {
          criterion:
            "Given I type rapidly while completions are loading, when multiple requests are queued, then my editor remains responsive (no freezing)",
          verification:
            "Manual test: Type rapidly in Lua block; verify editor remains responsive throughout",
        },
        {
          criterion:
            "Given I have many LSP requests in flight, when the queue fills up, then new requests fail gracefully with an error instead of blocking",
          verification:
            "Manual test: Trigger many rapid requests; verify errors returned rather than freezing",
        },
        {
          criterion:
            "Given a language server crashes mid-request, when I continue editing, then the editor recovers without freezing or orphaned tasks",
          verification:
            "Manual test: Kill lua-language-server during operation; verify editor recovers gracefully",
        },
        {
          criterion:
            "Given multiple concurrent LSP requests, when they complete, then responses are routed to the correct requests (no mix-ups)",
          verification:
            "Integration test: Send hover and completion concurrently; verify correct responses returned",
        },
      ],
      status: "draft",
      refinement_notes: [
        "TECHNICAL: Implement single-writer actor loop for stdin serialization (ADR-0015 section 1)",
        "TECHNICAL: Bounded mpsc channel with capacity 256",
        "TECHNICAL: Spawn dedicated reader task per connection with select! loop (ADR-0014)",
        "TECHNICAL: Use try_send() for non-blocking backpressure (ADR-0015 section 3)",
        "TECHNICAL: Notifications dropped with WARN log when queue full",
        "TECHNICAL: Requests return REQUEST_FAILED (-32803) when queue full",
        "TECHNICAL: CancellationToken for cross-task panic propagation (ADR-0015 section 6)",
        "ADR Compliance: ADR-0014 (Async), ADR-0015 (Actor Pattern)",
        "Consolidates former PBI-311 (Actor Loop), PBI-312 (Reader Task), PBI-313 (Backpressure)",
      ],
    },

    // --- PBI-314: Customizable Timeouts for Different Servers (P3) ---
    {
      id: "PBI-314",
      story: {
        role: "Lua developer editing markdown",
        capability:
          "configure timeout values for different language servers",
        benefit:
          "slow servers like rust-analyzer get longer timeouts while fast servers respond quickly",
      },
      acceptance_criteria: [
        {
          criterion:
            "Given I configure a 90-second timeout for rust-analyzer in my config, when rust-analyzer takes 70 seconds to start, then it succeeds instead of timing out",
          verification:
            "Manual test: Configure long init timeout; verify slow server startup succeeds",
        },
        {
          criterion:
            "Given I configure a 10-second timeout for lua-language-server, when it takes longer, then I get a timeout error quickly",
          verification:
            "Manual test: Configure short timeout; verify quick error on slow response",
        },
        {
          criterion:
            "Given I don't configure any timeouts, when I use LSP features, then sensible defaults apply (60s init, 30s request, 10s shutdown)",
          verification:
            "Manual test: Use default config; verify default timeouts work correctly",
        },
        {
          criterion:
            "Given I configure timeouts in my treesitter-ls config, when I save the config, then the new timeouts apply to new connections",
          verification:
            "Manual test: Change config; open new file; verify new timeout applies",
        },
      ],
      status: "draft",
      refinement_notes: [
        "TECHNICAL: Add timeout fields to BridgeServerConfig struct",
        "TECHNICAL: Fields: initialization_timeout, liveness_timeout, shutdown_timeout",
        "TECHNICAL: Location: config.rs BridgeServerConfig",
        "TECHNICAL: Use Duration type with human-readable parsing (e.g., '30s', '2m')",
        "TECHNICAL: Defaults per ADR-0018: init 60s, liveness 30s, shutdown 10s",
        "ADR Compliance: ADR-0018 (Timeout Hierarchy)",
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
