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

  // Completed PBIs: PBI-001 through PBI-140 (Sprint 1-113), PBI-144 to PBI-150 (Sprint 114-120) | History: git log -- scrum.yaml, scrum.ts
  // Deferred: PBI-091 (idle cleanup), PBI-107 (remove WorkspaceType - rust-analyzer too slow)
  product_backlog: [
    // ADR-0009 Implementation: Vertical slices with user-facing value
    // Critical concurrency fixes from review.md (new issues after Sprint 118)
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

  sprint: null,

  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_nvim" },
    ],
  },

  // Historical sprints (recent 2) | Sprint 1-119: git log -- scrum.yaml, scrum.ts
  completed: [
    { number: 120, pbi_id: "PBI-150", goal: "Make document version increments atomic to prevent duplicate versions under concurrent access", status: "done", subtasks: [] },
    { number: 119, pbi_id: "PBI-149", goal: "Serialize concurrent hover requests per connection using tokio::Mutex", status: "done", subtasks: [] },
  ],

  // Recent 2 retrospectives | Sprint 1-117: modular refactoring pattern, E2E indexing waits, vertical slice validation
  retrospectives: [
    { sprint: 120, improvements: [
      { action: "Review all sync_document callers for compatibility with new return type Option<u32> and document migration pattern", timing: "sprint", status: "active", outcome: null },
      { action: "Document atomic operations pattern for version/counter tracking in ADR and apply to similar scenarios", timing: "sprint", status: "active", outcome: null },
      { action: "Create PBI to fix flaky test spawn_and_initialize_waits_for_indexing and establish test stability monitoring", timing: "product", status: "active", outcome: null },
      { action: "Create PBI for production observability of document version tracking anomalies", timing: "product", status: "active", outcome: null },
    ] },
    { sprint: 119, improvements: [
      { action: "Merge lock infrastructure and lock usage into single subtask when implementation is simple (avoid over-decomposition)", timing: "sprint", status: "active", outcome: null },
      { action: "Investigate test parallelization issues causing flaky failures and establish parallel test stability baseline", timing: "product", status: "active", outcome: null },
      { action: "Consider test optimization strategies for slow rust-analyzer dependent tests (mocking, selective execution)", timing: "sprint", status: "active", outcome: null },
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
