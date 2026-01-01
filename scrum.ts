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
    // Completed: PBI-144 (Sprint 114), PBI-145 (Sprint 115), PBI-148 (Sprint 116), PBI-146 (Sprint 117), PBI-147 (Sprint 118)
    // Critical concurrency fixes from review.md (new issues after Sprint 118)
    {
      id: "PBI-149",
      story: {
        role: "Rustacean editing Markdown",
        capability: "get correct hover results even when moving cursor quickly between code blocks",
        benefit: "hover information always matches the code under the cursor, not stale or wrong content",
      },
      acceptance_criteria: [
        {
          criterion: "Concurrent hover requests are serialized per connection using tokio::Mutex or semaphore",
          verification: "Unit test verifies sequential execution when two hovers race",
        },
        {
          criterion: "Each hover request has exclusive access to connection during didChange+hover sequence",
          verification: "Integration test sends concurrent hovers and verifies correct responses",
        },
        {
          criterion: "Rapid cursor movement produces correct hover results",
          verification: "E2E test moves cursor quickly between code blocks, all hover results match expected content",
        },
      ],
      status: "ready",
    },
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
    number: 119,
    pbi_id: "PBI-149",
    goal: "Serialize concurrent hover requests per connection using tokio::Mutex to prevent race conditions where didChange and hover RPCs interleave, ensuring each hover request has exclusive access during the didChange+hover sequence",
    status: "in_progress",
    subtasks: [
      {
        test: "Unit test verifies that TokioAsyncLanguageServerPool wraps connection in tokio::Mutex and hover() acquires lock before sync_document+request sequence",
        implementation: "Add connection_locks: DashMap<String, Arc<tokio::sync::Mutex<()>>> to TokioAsyncLanguageServerPool and acquire lock at start of hover()",
        type: "behavioral",
        status: "completed",
        commits: [{ hash: "fb72afb", message: "test(bridge): add connection lock infrastructure for hover serialization", phase: "green" }],
        notes: ["The Mutex guards the sync_document+hover atomic sequence, not the connection itself", "Test: pool_provides_connection_lock_for_serialization", "No refactoring needed - code already clean"],
      },
      {
        test: "Unit test verifies second concurrent hover() call blocks until first completes (using tokio::time::timeout to detect blocking)",
        implementation: "Ensure lock is held for entire sync_document+send_request+await_response sequence in hover()",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["Two hover tasks started simultaneously - second should wait for first to release lock"],
      },
      {
        test: "Integration test sends two concurrent hovers with different content, verifies each receives correct response matching its content",
        implementation: "Lock scope must include response await to prevent interleaving of request A content with request B response",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["Key test: content A='fn foo() -> i32', content B='fn foo() -> String', verify hover A shows i32, hover B shows String"],
      },
      {
        test: "E2E test rapidly moves cursor between two code blocks, verifies all hover results match expected content for each block",
        implementation: "Verify serialization works end-to-end through the full hover_impl path",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: ["Simulates real-world rapid cursor movement that triggers the race condition"],
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

  // Historical sprints (recent 2) | Sprint 1-117: git log -- scrum.yaml, scrum.ts
  completed: [
    { number: 118, pbi_id: "PBI-147", goal: "Wait for rust-analyzer to complete initial indexing before serving first hover request, using $/progress notifications to detect completion, ensuring single hover request returns result", status: "done", subtasks: [] },
    { number: 117, pbi_id: "PBI-146", goal: "Track document versions per virtual URI, send didOpen on first access and didChange with incremented version on subsequent accesses, ensuring hover responses reflect the latest code", status: "done", subtasks: [] },
  ],

  // Recent 2 retrospectives | Sprint 1-116: modular refactoring pattern, E2E indexing waits, vertical slice validation
  retrospectives: [
    { sprint: 118, improvements: [
      { action: "Language server behavior varies by context - design fallback signals when primary indicators are unreliable", timing: "sprint", status: "active", outcome: null },
      { action: "Indexing wait should be part of connection initialization architecture from the start", timing: "sprint", status: "active", outcome: null },
    ] },
    { sprint: 117, improvements: [
      { action: "Study reference implementation patterns before new features - sync bridge had versioning model", timing: "sprint", status: "active", outcome: null },
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
