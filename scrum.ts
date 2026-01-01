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

  // Completed PBIs: PBI-001 through PBI-140 (Sprint 1-113) | History: git log -- scrum.yaml, scrum.ts
  // Deferred: PBI-091 (idle cleanup), PBI-107 (remove WorkspaceType - rust-analyzer too slow)
  product_backlog: [
    // ADR-0009 Implementation: Vertical slices with user-facing value
    // Completed: PBI-144 to PBI-149 (Sprint 114-119) | History: git log -- scrum.yaml, scrum.ts
    // Critical concurrency fixes from review.md (new issues after Sprint 118)
    {
      id: "PBI-150",
      story: {
        role: "Rustacean editing Markdown",
        capability: "have document edits always reach the language server in order",
        benefit: "hover and other LSP features show up-to-date information even during rapid editing",
      },
      acceptance_criteria: [
        {
          criterion: "Document version increments use atomic fetch_add or per-URI lock",
          verification: "Unit test verifies versions are strictly monotonic under concurrent access",
        },
        {
          criterion: "Concurrent sync_document calls produce sequential version numbers",
          verification: "Integration test with parallel sync_document calls verifies no duplicate versions",
        },
        {
          criterion: "LSP server never sees decreasing or duplicate version numbers",
          verification: "Trace log confirms monotonically increasing versions in didChange notifications",
        },
      ],
      status: "ready",
    },
    {
      id: "PBI-151",
      story: {
        role: "Rustacean editing Markdown",
        capability: "see language server progress indicators while working on code",
        benefit: "feedback about rust-analyzer activity (rebuilding, checking) stays visible throughout the session",
      },
      acceptance_criteria: [
        {
          criterion: "Background task continuously drains notification receivers and forwards $/progress",
          verification: "Unit test verifies forwarding continues after initial indexing wait",
        },
        {
          criterion: "$/progress notifications reach client during hover requests (not just initial indexing)",
          verification: "Integration test triggers rebuild and verifies $/progress forwarded",
        },
        {
          criterion: "Editor shows progress indicators when rust-analyzer rebuilds after code changes",
          verification: "E2E test edits code, verifies progress notifications received by client",
        },
      ],
      status: "ready",
    },
    {
      id: "PBI-152",
      story: {
        role: "developer editing Lua files",
        capability: "get first hover result quickly even with minimal language server activity",
        benefit: "hover works within seconds, not waiting up to 60 seconds for timeout",
      },
      acceptance_criteria: [
        {
          criterion: "Indexing wait treats single completion signal as sufficient (or is configurable)",
          verification: "Unit test with mock server emitting 1 notification verifies fast completion",
        },
        {
          criterion: "wait_for_indexing returns promptly when server is ready but quiet",
          verification: "Integration test with lua-language-server returns hover within 5 seconds",
        },
        {
          criterion: "First hover request returns quickly for simple files",
          verification: "E2E test with simple Lua code block verifies hover returns within 5 seconds",
        },
      ],
      status: "ready",
    },
    {
      id: "PBI-141",
      story: {
        role: "developer editing Lua files",
        capability: "have go-to-definition requests in Markdown code blocks use fully async I/O",
        benefit: "definition responses are faster and don't block other LSP requests while waiting for lua-language-server",
      },
      acceptance_criteria: [
        {
          criterion: "TokioAsyncLanguageServerPool.goto_definition() method implemented with async request/response pattern",
          verification: "Unit test verifies goto_definition returns valid Location response",
        },
        {
          criterion: "definition_impl uses async pool.goto_definition() instead of spawn_blocking",
          verification: "grep confirms no spawn_blocking in definition.rs for bridged requests",
        },
        {
          criterion: "Go-to-definition requests to lua-language-server return valid responses through async path",
          verification: "E2E test opens Markdown with Lua code block, requests definition, receives location",
        },
      ],
      status: "ready",
    },
    {
      id: "PBI-142",
      story: {
        role: "Rustacean editing Markdown",
        capability: "have completion requests in Markdown code blocks use fully async I/O",
        benefit: "completion responses are faster and don't block other LSP requests while waiting for rust-analyzer",
      },
      acceptance_criteria: [
        {
          criterion: "TokioAsyncLanguageServerPool.completion() method implemented with async request/response pattern",
          verification: "Unit test verifies completion returns valid CompletionList response",
        },
        {
          criterion: "completion handler uses async pool.completion() for bridged requests",
          verification: "grep confirms async completion path in lsp_impl.rs",
        },
        {
          criterion: "Completion requests to rust-analyzer return valid responses through async path",
          verification: "E2E test opens Markdown with Rust code block, requests completion, receives items",
        },
      ],
      status: "ready",
    },
    {
      id: "PBI-143",
      story: {
        role: "Rustacean editing Markdown",
        capability: "have signatureHelp requests in Markdown code blocks use fully async I/O",
        benefit: "signature help responses are faster and show parameter hints without blocking",
      },
      acceptance_criteria: [
        {
          criterion: "TokioAsyncLanguageServerPool.signature_help() method implemented with async request/response pattern",
          verification: "Unit test verifies signature_help returns valid SignatureHelp response",
        },
        {
          criterion: "signatureHelp handler uses async pool.signature_help() for bridged requests",
          verification: "grep confirms async signature_help path in lsp_impl.rs",
        },
        {
          criterion: "SignatureHelp requests to rust-analyzer return valid responses through async path",
          verification: "E2E test opens Markdown with Rust code block, requests signatureHelp, receives signatures",
        },
      ],
      status: "ready",
    },
  ],

  sprint: {
    number: 120,
    pbi_id: "PBI-150",
    goal: "Make document version increments atomic to prevent duplicate versions under concurrent access",
    status: "in_progress",
    subtasks: [
      {
        test: "Unit test: concurrent increment_document_version calls produce strictly monotonic versions (verifies AtomicU32)",
        implementation: "Changed document_versions from DashMap<String, u32> to DashMap<String, AtomicU32>, use fetch_add(1, SeqCst)",
        type: "behavioral",
        status: "completed",
        commits: [
          { hash: "9cb0159", message: "fix(bridge): use AtomicU32 for document version tracking", phase: "green" },
        ],
        notes: [
          "Test: concurrent_version_increments_produce_monotonic_versions spawns 100 tasks with barrier",
          "AtomicU32::fetch_add atomically increments and returns previous value",
          "No separate read-then-write, so no race window",
        ],
      },
      {
        test: "Integration test: parallel sync_document calls produce sequential version numbers with no duplicates",
        implementation: "Modify sync_document to return Option<u32> (version sent), verify concurrent calls produce unique versions",
        type: "behavioral",
        status: "completed",
        commits: [
          { hash: "9e35779", message: "test(bridge): add integration test for parallel sync_document version tracking", phase: "green" },
        ],
        notes: [
          "Uses real TokioAsyncLanguageServerPool with rust-analyzer connection",
          "Spawns N concurrent sync_document calls with barrier for simultaneous start",
          "Collects versions via return value (sync_document returns Option<u32>)",
          "Asserts: count(unique_versions) == N and versions are consecutive",
        ],
      },
      {
        test: "Trace log confirms monotonically increasing versions in didChange notifications",
        implementation: "Add trace logging to sync_document showing version numbers sent",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [
          "Add log::trace! in sync_document showing uri and version",
          "E2E or integration test captures logs",
          "Parse logs to verify strictly increasing sequence",
        ],
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

  // Historical sprints (recent 2) | Sprint 1-118: git log -- scrum.yaml, scrum.ts
  completed: [
    { number: 119, pbi_id: "PBI-149", goal: "Serialize concurrent hover requests per connection using tokio::Mutex", status: "done", subtasks: [] },
    { number: 118, pbi_id: "PBI-147", goal: "Wait for rust-analyzer indexing before first hover request", status: "done", subtasks: [] },
  ],

  // Recent 2 retrospectives | Sprint 1-116: modular refactoring pattern, E2E indexing waits, vertical slice validation
  retrospectives: [
    { sprint: 119, improvements: [
      { action: "Merge lock infrastructure and lock usage into single subtask when implementation is simple (avoid over-decomposition)", timing: "sprint", status: "active", outcome: null },
      { action: "Investigate test parallelization issues causing flaky failures and establish parallel test stability baseline", timing: "product", status: "active", outcome: null },
      { action: "Consider test optimization strategies for slow rust-analyzer dependent tests (mocking, selective execution)", timing: "sprint", status: "active", outcome: null },
    ] },
    { sprint: 118, improvements: [
      { action: "Language server behavior varies by context - design fallback signals when primary indicators are unreliable", timing: "sprint", status: "active", outcome: null },
      { action: "Indexing wait should be part of connection initialization architecture from the start", timing: "sprint", status: "active", outcome: null },
    ] },
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
