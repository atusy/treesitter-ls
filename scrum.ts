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

  // Completed PBIs: PBI-001 through PBI-135 (Sprint 1-112) | History: git log -- scrum.yaml, scrum.ts
  // Deferred: PBI-091 (idle cleanup), PBI-107 (remove WorkspaceType - rust-analyzer too slow)
  // Replaced: PBI-136 through PBI-139 (infrastructure-only, never wired) -> PBI-140 through PBI-142 (vertical slices)
  product_backlog: [
    // ADR-0009 Implementation: Vertical slices with user-facing value
    {
      id: "PBI-140",
      story: {
        role: "Rustacean editing Markdown",
        capability: "have hover requests in Markdown code blocks use fully async I/O",
        benefit: "hover responses are faster and don't block other LSP requests while waiting for rust-analyzer",
      },
      acceptance_criteria: [
        {
          criterion: "TokioAsyncBridgeConnection implements full async reader task with select! for read/shutdown/timeout",
          verification: "Unit test verifies shutdown while reader is idle completes within 100ms (no blocked read_line)",
        },
        {
          criterion: "TokioAsyncLanguageServerPool wraps TokioAsyncBridgeConnection and is wired into TreeSitterLs struct",
          verification: "lsp_impl.rs uses TokioAsyncLanguageServerPool for hover requests instead of LanguageServerPool",
        },
        {
          criterion: "hover_impl uses async pool.hover() instead of spawn_blocking with sync connection",
          verification: "grep confirms no spawn_blocking in hover.rs, uses .await on async hover call",
        },
        {
          criterion: "Hover requests to rust-analyzer return valid responses through async path",
          verification: "Integration test opens Markdown with Rust code block, requests hover, receives type info",
        },
      ],
      status: "done",
    },
  ],

  sprint: {
    number: 113,
    pbi_id: "PBI-140",
    goal: "Implement fully async hover bridging with TokioAsyncBridgeConnection reader task, TokioAsyncLanguageServerPool, and wire into hover_impl to replace spawn_blocking pattern",
    status: "done",
    subtasks: [
      // Subtask 1: Implement reader task with select! for clean shutdown (AC1)
      {
        test: "Unit test: TokioAsyncBridgeConnection shutdown while reader idle completes within 100ms",
        implementation: "Implement reader task loop with tokio::select! for read_line/shutdown_rx/timeout branches, parse LSP messages, route responses by id to pending_requests DashMap",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "7a10bcd", message: "feat(bridge): add async request/response queue pattern for concurrent requests", phase: "green" }],
        notes: ["Reader uses tokio::io::BufReader on ChildStdout", "select! enables non-blocking shutdown unlike sync read_line", "Reuse ResponseResult from async_connection.rs"],
      },
      // Subtask 2: Implement send_request and send_notification async methods
      {
        test: "Unit test: send_request returns receiver that resolves when reader routes matching response",
        implementation: "Add send_request(method, params) -> Result<oneshot::Receiver<ResponseResult>> and send_notification(method, params) -> Result<()> using tokio::sync::Mutex<ChildStdin>",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "5e35b0b", message: "feat(bridge): add send_request and send_notification async methods", phase: "green" }],
        notes: ["Increment next_request_id atomically", "Insert oneshot::Sender into pending_requests before writing", "Write LSP message format (Content-Length header + JSON body)"],
      },
      // Subtask 3: Implement TokioAsyncLanguageServerPool with spawn_and_initialize
      {
        test: "Unit test: TokioAsyncLanguageServerPool::get_connection returns Arc<TokioAsyncBridgeConnection> after spawn+initialize",
        implementation: "Create tokio_async_pool.rs with spawn_and_initialize that spawns process, sends initialize request, waits for response, sends initialized notification, stores virtual_uri",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "d385de3", message: "feat(bridge): add TokioAsyncLanguageServerPool with spawn_and_initialize", phase: "green" }],
        notes: ["Similar to AsyncLanguageServerPool but uses TokioAsyncBridgeConnection", "Store connections in DashMap<String, Arc<TokioAsyncBridgeConnection>>", "Store virtual_uris in DashMap<String, String>"],
      },
      // Subtask 4: Implement hover() async method on TokioAsyncLanguageServerPool
      {
        test: "Integration test: TokioAsyncLanguageServerPool.hover() returns Hover from rust-analyzer",
        implementation: "Add pub async fn hover() that calls ensure_document_open (didOpen/didChange), sends textDocument/hover request, awaits response, parses Hover",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "b7a005a", message: "feat(bridge): add hover() async method to TokioAsyncLanguageServerPool", phase: "green" }],
        notes: ["Follows same pattern as AsyncLanguageServerPool.hover()", "Use connection.send_request + await receiver", "translate virtual URI for textDocument params"],
      },
      // Subtask 5: Wire TokioAsyncLanguageServerPool into TreeSitterLs struct
      {
        test: "Compile test: TreeSitterLs has tokio_async_pool field of type TokioAsyncLanguageServerPool",
        implementation: "Add tokio_async_pool: TokioAsyncLanguageServerPool field to TreeSitterLs, initialize in new(), add getter method",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "c22bd85", message: "feat(lsp): wire TokioAsyncLanguageServerPool into TreeSitterLs", phase: "green" }],
        notes: ["Keep existing language_server_pool for other handlers during migration", "TokioAsyncLanguageServerPool needs notification_sender channel similar to AsyncLanguageServerPool"],
      },
      // Subtask 6: Replace spawn_blocking in hover_impl with async pool.hover()
      {
        test: "Integration test: hover_impl returns valid Hover through async path (no spawn_blocking)",
        implementation: "Modify hover_impl to call self.tokio_async_pool.hover() instead of spawn_blocking with sync connection, remove spawn_blocking call, use .await",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "b4c5c52", message: "feat(hover): replace spawn_blocking with async pool.hover()", phase: "green" }],
        notes: ["AC3: grep confirms no spawn_blocking in hover.rs", "Translate position host->virtual before call, virtual->host after response", "Forward progress notifications to client"],
      },
      // Subtask 7: End-to-end integration test with rust-analyzer
      {
        test: "Integration test: Open Markdown with Rust code block, request hover in code block, receive type info",
        implementation: "Create test in tests/ that spawns treesitter-ls, opens Markdown file with ```rust block, sends textDocument/hover, verifies Hover response contains Rust type information",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "0de999b", message: "test(e2e): add hover E2E test for Markdown code blocks", phase: "green" }],
        notes: ["AC4 verification", "Can use existing test pattern from test_auto_install_integration.rs", "Verify hover content contains fn signature or type info"],
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

  // Historical sprints (recent 2) | Sprint 1-111: git log -- scrum.yaml, scrum.ts
  completed: [
    { number: 112, pbi_id: "PBI-135", goal: "Implement TokioAsyncBridgeConnection struct using tokio::process::Command for spawning language servers, establishing the foundation for fully async I/O", status: "done", subtasks: [] },
    { number: 111, pbi_id: "PBI-134", goal: "Store virtual_file_path in AsyncLanguageServerPool so get_virtual_uri returns valid URIs", status: "done", subtasks: [] },
  ],

  // Recent 2 retrospectives | Sprint 1-110: modular refactoring pattern, E2E indexing waits
  retrospectives: [
    {
      sprint: 112,
      improvements: [
        { action: "Obvious Implementation pattern validated for tightly-coupled acceptance criteria - when ACs naturally require each other (spawn -> extract handles -> wrap in Mutex), single GREEN commit is correct TDD", timing: "immediate", status: "completed", outcome: "3 ACs implemented in one commit following ADR-0009 struct specification exactly" },
        { action: "Track #[allow(dead_code)] annotations added during incremental feature implementation - remove as API surface is consumed by subsequent PBIs (PBI-140 through PBI-143)", timing: "sprint", status: "active", outcome: null },
        { action: "Continue using parallel module pattern (tokio_connection.rs alongside async_connection.rs) until full migration, then delete old implementation per ADR-0009 Phase 5", timing: "product", status: "active", outcome: null },
      ],
    },
    {
      sprint: 111,
      improvements: [
        { action: "PR review from external tools (gemini-code-assist) caught real bug - continue using automated PR review for async bridge features", timing: "immediate", status: "completed", outcome: "gemini-code-assist identified get_virtual_uri always returning None; bug fixed in Sprint 111" },
        { action: "When implementing new async connection features, always verify the full request flow including stored state (virtual URIs, document versions) before marking complete", timing: "immediate", status: "completed", outcome: "Added test async_pool_stores_virtual_uri_after_connection to verify URI storage" },
        { action: "Add E2E test for async bridge hover feature to verify end-to-end flow works (unit test exists but no E2E coverage)", timing: "product", status: "active", outcome: null },
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
