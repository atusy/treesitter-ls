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

  // Completed PBIs: PBI-001 through PBI-134 | History: git log -- scrum.yaml, scrum.ts
  // PBI-091 (idle cleanup): Deferred - infrastructure already implemented, needs wiring (low priority)
  // PBI-107 (remove WorkspaceType): Deferred - rust-analyzer linkedProjects too slow
  product_backlog: [
    // ADR-0009 Phase 1: TokioAsyncBridgeConnection foundation
    {
      id: "PBI-135",
      story: {
        role: "Rustacean editing Markdown",
        capability: "have bridge connections spawn language servers using tokio::process::Command",
        benefit: "the foundation for fully async I/O is established without blocking OS threads",
      },
      acceptance_criteria: [
        {
          criterion: "TokioAsyncBridgeConnection::spawn() uses tokio::process::Command instead of std::process::Command",
          verification: "Unit test spawns rust-analyzer with tokio::process and verifies child process is running",
        },
        {
          criterion: "Async stdin/stdout handles are obtained from the tokio Child process",
          verification: "Unit test verifies ChildStdin and ChildStdout are extracted and stored",
        },
        {
          criterion: "The struct stores tokio::sync::Mutex<ChildStdin> for async write serialization",
          verification: "Type signature compiles with tokio::sync::Mutex wrapping ChildStdin",
        },
      ],
      status: "ready",
    },
    {
      id: "PBI-136",
      story: {
        role: "Rustacean editing Markdown",
        capability: "have the async connection reader task use select! for read and shutdown",
        benefit: "shutdown signals are handled cleanly without blocking on read_line forever (fixes the shutdown bug)",
      },
      acceptance_criteria: [
        {
          criterion: "Reader task uses tokio::select! to multiplex between reading lines and receiving shutdown signal",
          verification: "Unit test sends shutdown while reader is idle and verifies task exits within 100ms",
        },
        {
          criterion: "Shutdown uses oneshot channel instead of AtomicBool polling",
          verification: "Struct has shutdown_tx: Option<oneshot::Sender<()>> field",
        },
        {
          criterion: "Reader task is spawned with tokio::spawn, not std::thread::spawn",
          verification: "JoinHandle type is tokio::task::JoinHandle, not std::thread::JoinHandle",
        },
      ],
      status: "ready",
    },
    {
      id: "PBI-137",
      story: {
        role: "Rustacean editing Markdown",
        capability: "have the async reader parse LSP JSON-RPC messages using async I/O",
        benefit: "messages are read without blocking OS threads, enabling efficient concurrent request handling",
      },
      acceptance_criteria: [
        {
          criterion: "Reader uses tokio::io::BufReader with AsyncBufReadExt for header reading",
          verification: "Unit test sends a valid LSP message and verifies it is parsed correctly",
        },
        {
          criterion: "Content-Length header is parsed and correct number of bytes are read",
          verification: "Unit test with multi-byte UTF-8 content verifies exact byte count is read",
        },
        {
          criterion: "Responses are routed to pending_requests DashMap by request ID",
          verification: "Integration test sends request, receives response, verifies oneshot channel receives it",
        },
      ],
      status: "ready",
    },
    {
      id: "PBI-138",
      story: {
        role: "Rustacean editing Markdown",
        capability: "send requests through the tokio async connection and await responses",
        benefit: "multiple concurrent requests can share one connection without blocking each other",
      },
      acceptance_criteria: [
        {
          criterion: "send_request() is async and uses tokio::sync::Mutex for stdin access",
          verification: "Function signature is pub async fn send_request(...) -> Result<...>",
        },
        {
          criterion: "send_request() returns immediately with a oneshot::Receiver for the response",
          verification: "Unit test verifies receiver is returned before response arrives",
        },
        {
          criterion: "Request ID is atomically incremented and used for response routing",
          verification: "Integration test sends two concurrent requests, verifies each gets correct response",
        },
      ],
      status: "ready",
    },
    {
      id: "PBI-139",
      story: {
        role: "Rustacean editing Markdown",
        capability: "have the async connection handle initialization handshake with language servers",
        benefit: "language servers are properly initialized before accepting requests",
      },
      acceptance_criteria: [
        {
          criterion: "spawn() sends initialize request and awaits response before returning",
          verification: "Integration test with rust-analyzer verifies initialize response is received",
        },
        {
          criterion: "spawn() sends initialized notification after initialize response",
          verification: "Integration test verifies connection is ready for textDocument/* requests",
        },
        {
          criterion: "spawn() stores virtual_file_path and returns it via get_virtual_uri()",
          verification: "Integration test verifies virtual URI ends with correct extension",
        },
      ],
      status: "ready",
    },
  ],

  sprint: {
    number: 112,
    pbi_id: "PBI-135",
    goal: "Implement TokioAsyncBridgeConnection struct using tokio::process::Command for spawning language servers, establishing the foundation for fully async I/O",
    status: "review",
    subtasks: [
      {
        test: "tokio_async_bridge_connection_struct_exists - verify the struct has required fields: stdin (tokio::sync::Mutex<ChildStdin>), pending_requests (Arc<DashMap>), next_request_id (AtomicI64), shutdown_tx, reader_handle",
        implementation: "Define TokioAsyncBridgeConnection struct in new file src/lsp/bridge/tokio_connection.rs with all required fields",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "853902a", message: "feat(bridge): add TokioAsyncBridgeConnection struct skeleton", phase: "green" }],
        notes: ["AC3: The struct stores tokio::sync::Mutex<ChildStdin> for async write serialization"],
      },
      {
        test: "spawn_uses_tokio_process_command - unit test spawns a simple process with tokio::process::Command and verifies child is created",
        implementation: "Implement spawn() async fn using tokio::process::Command with stdin/stdout piped",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "5569fa9", message: "test(bridge): add failing test for spawn() with tokio::process::Command", phase: "green" }, { hash: "36e3df1", message: "feat(bridge): implement spawn() with tokio::process::Command", phase: "green" }],
        notes: ["AC1: TokioAsyncBridgeConnection::spawn() uses tokio::process::Command instead of std::process::Command", "Obvious Implementation pattern - all spawn subtasks implemented together"],
      },
      {
        test: "spawn_extracts_stdin_stdout_from_child - verify ChildStdin and ChildStdout are obtained from tokio Child process and stdin is wrapped in tokio::sync::Mutex",
        implementation: "Take stdin/stdout from Child, wrap stdin in tokio::sync::Mutex, store stdout for reader task",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "36e3df1", message: "feat(bridge): implement spawn() with tokio::process::Command", phase: "green" }],
        notes: ["AC2: Async stdin/stdout handles are obtained from the tokio Child process", "Implemented as part of spawn() in subtask 2"],
      },
      {
        test: "spawn_creates_reader_task_handle - verify spawn returns struct with reader_handle (tokio::task::JoinHandle) and shutdown_tx (oneshot::Sender)",
        implementation: "Create placeholder reader task with tokio::spawn, store shutdown oneshot sender",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "36e3df1", message: "feat(bridge): implement spawn() with tokio::process::Command", phase: "green" }],
        notes: ["Prepare for PBI-136 reader task with select!", "Implemented as part of spawn() in subtask 2"],
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

  // Historical sprints (recent 2) | Sprint 1-110: git log -- scrum.yaml, scrum.ts
  completed: [
    { number: 111, pbi_id: "PBI-134", goal: "Store virtual_file_path in AsyncLanguageServerPool so get_virtual_uri returns valid URIs", status: "done", subtasks: [] },
    { number: 110, pbi_id: "PBI-133", goal: "Verify DashMap lock safety with concurrent test and add safety documentation", status: "done", subtasks: [] },
  ],

  // Recent 2 retrospectives | Sprint 1-109: modular refactoring pattern, E2E indexing waits
  retrospectives: [
    {
      sprint: 111,
      improvements: [
        { action: "PR review from external tools (gemini-code-assist) caught real bug - continue using automated PR review for async bridge features", timing: "immediate", status: "completed", outcome: "gemini-code-assist identified get_virtual_uri always returning None; bug fixed in Sprint 111" },
        { action: "When implementing new async connection features, always verify the full request flow including stored state (virtual URIs, document versions) before marking complete", timing: "immediate", status: "completed", outcome: "Added test async_pool_stores_virtual_uri_after_connection to verify URI storage" },
        { action: "Add E2E test for async bridge hover feature to verify end-to-end flow works (unit test exists but no E2E coverage)", timing: "product", status: "active", outcome: null },
      ],
    },
    {
      sprint: 110,
      improvements: [
        { action: "Investigate root cause earlier when PBI assumes a bug exists - validate assumption before detailed implementation planning", timing: "immediate", status: "completed", outcome: "Sprint 110 refinement correctly pivoted from 'fix deadlock' to 'verify and document safety' when code was found already safe" },
        { action: "Document Rust's .and_then() pattern as key to DashMap safety - it consumes Ref guard before subsequent operations", timing: "immediate", status: "completed", outcome: "Lock safety comments added to DocumentStore methods explaining .and_then() pattern" },
        { action: "User hang issue investigation: bridge I/O timeout (Sprint 109) and DashMap (Sprint 110) ruled out - investigate tokio::spawn panics or other mutex contention as next step", timing: "product", status: "active", outcome: null },
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
