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
      "Maintain stable async LSP bridge for core features using single-pool architecture (ADR-0006, 0007, 0008)",
    success_metrics: [
      {
        metric: "Bridge coverage",
        target:
          "Support hover, completion, signatureHelp, definition with fully async implementations",
      },
      {
        metric: "Modular architecture",
        target: "Bridge module organized with text_document/ subdirectory, single TokioAsyncLanguageServerPool",
      },
      {
        metric: "E2E test coverage",
        target: "Each bridged feature has E2E test verifying end-to-end async flow",
      },
    ],
  },

  // Completed PBIs: PBI-001 through PBI-151 (Sprint 1-120) | History: git log -- scrum.yaml, scrum.ts
  // Deferred: PBI-091 (idle cleanup), PBI-107 (remove WorkspaceType - rust-analyzer too slow)
  product_backlog: [
    // ADR-0009 Implementation: Vertical slices with user-facing value
    // Completed: PBI-144 (S114), PBI-145 (S115), PBI-148 (S116), PBI-146 (S117), PBI-149 (S118), PBI-150 (S119), PBI-151 (S120)
    // Rejected: PBI-147 (wait for indexing) - replaced by PBI-149 (informative message approach)
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
      status: "done",
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
      status: "done",
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
    number: 124,
    pbi_id: "PBI-151",
    goal: "Fix race conditions in version numbering and connection spawning to ensure LSP protocol compliance and efficient resource usage",
    status: "done" as SprintStatus,
    subtasks: [
      {
        test: "Test: Concurrent sync_document calls produce monotonically increasing version numbers without duplicates",
        implementation: "Use DashMap entry API for atomic version increment in sync_document",
        type: "behavioral" as SubtaskType,
        status: "completed" as SubtaskStatus,
        commits: [],
        notes: [
          "Current: separate read (get_document_version) + compute (current_version + 1) + write (set_document_version) allows duplicates",
          "Fix: Use DashMap::entry().and_modify().or_insert() for atomic read-modify-write",
          "Files: src/lsp/bridge/tokio_async_pool.rs (sync_document method, increment_document_version helper)",
          "Implementation: Created increment_document_version() method using DashMap entry API for atomic increment"
        ],
      },
      {
        test: "Test: Concurrent get_connection calls spawn exactly one language server process",
        implementation: "Use per-key mutex to prevent concurrent spawns for the same key",
        type: "behavioral" as SubtaskType,
        status: "completed" as SubtaskStatus,
        commits: [],
        notes: [
          "Current: check (connections.get) + spawn + insert allows concurrent spawns between check and insert",
          "Fix: Use per-key Mutex with double-check locking pattern to serialize spawns per key",
          "Files: src/lsp/bridge/tokio_async_pool.rs (get_connection method, spawn_locks field)",
          "Implementation: Added spawn_locks: Mutex<HashMap<String, Arc<Mutex<()>>>> to hold lock across async spawn"
        ],
      },
      {
        test: "Test: Run concurrent operation tests to verify both fixes",
        implementation: "Add integration tests for concurrent sync_document and concurrent get_connection",
        type: "behavioral" as SubtaskType,
        status: "completed" as SubtaskStatus,
        commits: [],
        notes: [
          "Test 1: Send 10 concurrent sync_document calls, collect versions, verify sequence 1,2,3,...,10 with no duplicates",
          "Test 2: Send 10 concurrent get_connection calls, verify only one connection instance created",
          "Files: src/lsp/bridge/tokio_async_pool.rs (#[cfg(test)] mod tests)",
          "Tests added: concurrent_sync_document_produces_unique_sequential_versions, concurrent_get_connection_spawns_single_process"
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

  // Historical sprints (recent 2) | Sprint 1-121: git log -- scrum.yaml, scrum.ts
  completed: [
    { number: 120, pbi_id: "PBI-151", goal: "Migrate critical Neovim E2E tests (hover, completion, references) to Rust with snapshot verification, establishing reusable patterns and helpers for future migrations", status: "done", subtasks: [] },
    { number: 119, pbi_id: "PBI-150", goal: "Implement Rust-based E2E testing infrastructure for go-to-definition with snapshot testing, enabling faster and more reliable tests without Neovim dependency", status: "done", subtasks: [] },
  ],

  // Recent 2 retrospectives | Sprint 1-120: ADR-driven development, reusable patterns, E2E test timing
  retrospectives: [
    { sprint: 123, improvements: [
      { action: "Refactor repeated cleanup pattern (collect IDs, iterate, remove, send None) into helper method - pattern appears in 3 places: EOF (Ok/Err branches), Drop impl", timing: "immediate", status: "completed", outcome: "Created clear_all_pending_requests() helper method, replaced 3 duplicated cleanup blocks (lines 223, 235, 456 in tokio_connection.rs)" },
    ] },
    { sprint: 122, improvements: [
      { action: "Delete E2E test files for removed features (13 files)", timing: "immediate", status: "completed", outcome: "Deleted 13 obsolete test files - retained: hover, completion, definition, signature_help + infrastructure tests" },
      { action: "Document architectural simplification decision (Sprint 122: deleted 16+ handlers, 3 legacy pools, ~1000+ lines) in ADR covering rationale for retaining only async implementations (hover, completion, signatureHelp, definition)", timing: "product", status: "active", outcome: null },
    ] },
    { sprint: 120, improvements: [
      { action: "Plan helper module architecture during sprint planning - identify reusable abstractions (LspClient, test fixtures, initialization patterns) before implementation starts to avoid mid-sprint extraction", timing: "sprint", status: "active", outcome: null },
      { action: "Document snapshot testing sanitization patterns - create testing guide explaining URI replacement, range normalization, non-deterministic data handling with examples from hover/completion/references tests", timing: "sprint", status: "active", outcome: null },
      { action: "Apply LSP spec study to test design - even for test migrations, studying textDocument/hover, textDocument/completion, textDocument/references spec helps identify edge cases and sanitization needs upfront", timing: "immediate", status: "active", outcome: null },
      { action: "Consider extracting E2E test helpers into shared testing library - tests/helpers_*.rs modules (lsp_client, lsp_polling, sanitization, fixtures) could become reusable crate if more LSP features will be tested", timing: "product", status: "active", outcome: null },
    ] },
    { sprint: 119, improvements: [
      { action: "Study LSP specification sections before implementing new LSP features - JSON-RPC 2.0, notification vs request semantics, server-initiated requests", timing: "immediate", status: "active", outcome: null },
      { action: "Extract retry-with-timeout pattern into reusable test helper - poll_until or wait_for_lsp_response with configurable attempts/delay", timing: "immediate", status: "completed", outcome: "poll_until(max_attempts, delay_ms, predicate) helper created in tests/helpers_lsp_polling.rs (Sprint 120 subtask 1-2)" },
      { action: "Document snapshot testing best practices - sanitization strategies for non-deterministic data (temp paths, timestamps, PIDs)", timing: "sprint", status: "active", outcome: null },
      { action: "Establish E2E testing strategy guidelines - when to use Rust E2E (protocol verification, CI speed) vs Neovim E2E (editor integration, user workflow)", timing: "sprint", status: "active", outcome: null },
      { action: "Consider migrating critical Neovim E2E tests to Rust - evaluate hover, completion for snapshot testing benefits", timing: "product", status: "completed", outcome: "Sprint 120 successfully migrated hover, completion, references with snapshot verification - pattern proven effective" },
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

interface ACVerification {
  criterion: string;
  status: "VERIFIED" | "FAILED" | "PENDING";
  evidence: string;
}

interface SprintReview {
  date: string;
  dod_results: {
    unit_tests: string;
    code_quality: string;
    e2e_tests: string;
  };
  acceptance_criteria_verification: ACVerification[];
  increment_status: string;
}

interface Sprint {
  number: number;
  pbi_id: string;
  goal: string;
  status: SprintStatus;
  subtasks: Subtask[];
  review?: SprintReview;
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
