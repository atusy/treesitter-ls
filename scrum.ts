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
    {
      id: "PBI-156",
      story: {
        role: "developer editing Lua files",
        capability: "have only my document's bridge state cleaned up when I close a file",
        benefit: "other open documents continue working with bridge features without unexpected state loss",
      },
      acceptance_criteria: [
        {
          criterion: "Host-to-bridge URI mapping tracks which bridge documents belong to each host document",
          verification: "Verify data structure maps host document URIs to their associated bridge virtual URIs",
        },
        {
          criterion: "didClose only closes bridge documents for the specific host document",
          verification: "Open two files with code blocks, close one, verify bridge state remains for the other file",
        },
        {
          criterion: "All tests pass with scoped document cleanup",
          verification: "Run `make test` and `make test_nvim` - all tests pass",
        },
      ],
      status: "ready",
    },
    {
      id: "PBI-157",
      story: {
        role: "Rustacean editing Markdown",
        capability: "have bridge LSP features auto-recover after bridge server crashes",
        benefit: "continue working without restarting entire LSP when bridge process fails",
      },
      acceptance_criteria: [
        {
          criterion: "Connection health check detects dead bridge processes",
          verification: "Verify get_connection checks process liveness before returning cached connection",
        },
        {
          criterion: "Dead connections are evicted and new processes spawned automatically",
          verification: "Kill bridge process, trigger request, verify new process spawned and request succeeds",
        },
        {
          criterion: "All tests pass with health monitoring",
          verification: "Run `make test` and `make test_nvim` - all tests pass",
        },
      ],
      status: "ready",
    },
    {
      id: "PBI-158",
      story: {
        role: "documentation author with Rust code blocks",
        capability: "have concurrent bridge requests not interfere with each other",
        benefit: "get correct hover/completion results even when multiple requests are in flight",
      },
      acceptance_criteria: [
        {
          criterion: "Each injection gets unique virtual URI to prevent content collision",
          verification: "Verify sync_document generates unique URI per host document + injection combination",
        },
        {
          criterion: "Concurrent requests for different injections don't overwrite each other's content",
          verification: "E2E test: trigger two hover requests simultaneously for different code blocks, verify both get correct results",
        },
        {
          criterion: "All tests pass with per-document URIs",
          verification: "Run `make test` and `make test_nvim` - all tests pass",
        },
      ],
      status: "ready",
    },
    {
      id: "PBI-159",
      story: {
        role: "developer editing Lua files",
        capability: "have bridge server receive exactly one didOpen per document",
        benefit: "avoid protocol errors and inconsistent state from duplicate open notifications",
      },
      acceptance_criteria: [
        {
          criterion: "Concurrent first-access requests synchronize didOpen sending",
          verification: "Verify sync_document uses proper locking to ensure only one didOpen is sent per URI",
        },
        {
          criterion: "Bridge server logs show single didOpen even under concurrent load",
          verification: "Unit test: spawn multiple concurrent requests for fresh connection, verify single didOpen sent",
        },
        {
          criterion: "All tests pass with synchronized didOpen",
          verification: "Run `make test` and `make test_nvim` - all tests pass",
        },
      ],
      status: "ready",
    },
    {
      id: "PBI-160",
      story: {
        role: "Rustacean editing Markdown",
        capability: "have no orphaned bridge processes when initialization times out",
        benefit: "prevent resource leaks and temp directory accumulation",
      },
      acceptance_criteria: [
        {
          criterion: "Timed-out initialization cancels spawn and cleans up process",
          verification: "Verify timeout handler calls kill() and waits for child process to exit",
        },
        {
          criterion: "Temp directories are removed when initialization fails",
          verification: "Trigger timeout, verify temp directory is cleaned up",
        },
        {
          criterion: "All tests pass with proper timeout cleanup",
          verification: "Run `make test` and `make test_nvim` - all tests pass",
        },
      ],
      status: "ready",
    },
    {
      id: "PBI-161",
      story: {
        role: "documentation author with Rust code blocks",
        capability: "edit files immediately after opening without server crashes",
        benefit: "reliable editing experience without timing-dependent failures",
      },
      acceptance_criteria: [
        {
          criterion: "Parser auto-install coordinates with parsing operations",
          verification: "Verify parser loader waits for downloads to complete before attempting deserialization",
        },
        {
          criterion: "Rapid edits after file open don't trigger panic",
          verification: "E2E test: open file, immediately type, verify no server crash",
        },
        {
          criterion: "All tests pass with coordinated auto-install",
          verification: "Run `make test` and `make test_nvim` - all tests pass",
        },
      ],
      status: "ready",
    },
    {
      id: "PBI-162",
      story: {
        role: "developer editing Lua files",
        capability: "have responsive hover/completion during fast typing",
        benefit: "avoid UI freezes from expensive semantic token computation",
      },
      acceptance_criteria: [
        {
          criterion: "Semantic token handlers observe cancellation tokens",
          verification: "Verify handlers check is_cancelled() and return early when request is cancelled",
        },
        {
          criterion: "Rapid edits don't queue unbounded semantic token computations",
          verification: "Add debouncing or request coalescing to prevent computation backlog",
        },
        {
          criterion: "All tests pass with cancellation support",
          verification: "Run `make test` and `make test_nvim` - all tests pass",
        },
      ],
      status: "ready",
    },
    {
      id: "PBI-163",
      story: {
        role: "Rustacean editing Markdown",
        capability: "have reliable unit test suite without flaky failures",
        benefit: "trust test results and catch real regressions",
      },
      acceptance_criteria: [
        {
          criterion: "Rust-analyzer resource contention is identified and mitigated",
          verification: "Investigate why completion and signature_help tests fail intermittently",
        },
        {
          criterion: "Unit tests pass consistently (357/357)",
          verification: "Run `make test` 10 times - all runs should pass",
        },
        {
          criterion: "Test environment properly isolates rust-analyzer instances",
          verification: "Verify tests use separate temp directories or sequential execution to avoid conflicts",
        },
      ],
      status: "ready",
    },
  ],

  sprint: {
    number: 128,
    pbi_id: "PBI-156",
    goal: "Fix close_all_documents to only close relevant bridge documents",
    status: "in_progress",
    subtasks: [
      {
        test: "Add test: TokioAsyncLanguageServerPool tracks host-to-bridge URI mapping",
        implementation: "Add DashMap<String, HashSet<String>> field host_to_bridge_uris to track which host URIs use which virtual URIs. Update sync_document to record the mapping when opening bridge documents.",
        type: "behavioral",
        status: "completed",
        commits: [
          {
            hash: "b3be582",
            message: "feat(bridge): add host-to-bridge URI mapping tracking",
            phase: "green",
          },
        ],
        notes: [
          "TDD Red: Write unit test verifying that sync_document updates host_to_bridge_uris mapping",
          "TDD Green: Add host_to_bridge_uris field and update sync_document to record host->virtual mapping",
          "Key insight: Multiple host documents may share the same bridge connection (e.g., rust-analyzer)",
          "Architecture: virtual_uris is keyed by connection key (e.g., 'rust-analyzer'), but we need to track which host URIs contributed to each virtual URI",
          "Implementation: Created sync_document_with_host() to track mappings while keeping existing sync_document() for backward compatibility",
        ],
      },
      {
        test: "Add test: close_documents_for_host only closes bridge documents for specified host URI",
        implementation: "Rename close_all_documents to close_documents_for_host, accept host_uri parameter. Look up associated virtual URIs from host_to_bridge_uris, send didClose only for those URIs, and remove the host URI from the mapping.",
        type: "behavioral",
        status: "completed",
        commits: [
          {
            hash: "034dced",
            message: "feat(bridge): implement close_documents_for_host with scoped cleanup",
            phase: "green",
          },
        ],
        notes: [
          "TDD Red: Write test opening two files with code blocks, close one, verify other's bridge state remains",
          "TDD Green: Implement scoped cleanup using host_to_bridge_uris lookup",
          "Cleanup: Remove host URI from mapping after closing its bridge documents",
          "Edge case: If virtual URI has no more host URIs, remove it from document_versions",
          "Implementation: close_documents_for_host checks if other hosts still use the bridge URI before actually sending didClose",
        ],
      },
      {
        test: "Add test: did_close handler passes host URI to close_documents_for_host",
        implementation: "Update lsp_impl.rs did_close handler to call close_documents_for_host with the closing host document's URI instead of close_all_documents().",
        type: "behavioral",
        status: "completed",
        commits: [
          {
            hash: "a4d0c70",
            message: "feat(bridge): update pool methods and did_close to use host URIs",
            phase: "green",
          },
        ],
        notes: [
          "TDD Red: Write integration test verifying correct host URI is passed to pool method",
          "TDD Green: Update did_close call site to pass host URI parameter",
          "This is the final integration point connecting host document lifecycle to scoped bridge cleanup",
          "Implementation: Updated all pool methods (hover, completion, signature_help, goto_definition) to accept host_uri and call sync_document_with_host",
          "Also updated all callers to pass host document URI instead of virtual URI",
        ],
      },
      {
        test: "Verify all acceptance criteria with make test and make test_nvim",
        implementation: "Run full test suite to ensure no behavioral regressions. Verify that existing bridge feature tests still pass with the new scoped cleanup.",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [
          "TDD: Verify all existing tests pass (regression check)",
          "AC1 verification: Host-to-bridge URI mapping tracks relationships",
          "AC2 verification: didClose only closes relevant bridge documents",
          "AC3 verification: All tests pass with scoped cleanup"
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

  // Historical sprints (recent 2) | Sprint 1-125: git log -- scrum.yaml, scrum.ts
  completed: [
    { number: 120, pbi_id: "PBI-151", goal: "Migrate critical Neovim E2E tests (hover, completion, references) to Rust with snapshot verification, establishing reusable patterns and helpers for future migrations", status: "done", subtasks: [] },
    { number: 119, pbi_id: "PBI-150", goal: "Implement Rust-based E2E testing infrastructure for go-to-definition with snapshot testing, enabling faster and more reliable tests without Neovim dependency", status: "done", subtasks: [] },
  ],

  // Recent 2 retrospectives | Sprint 1-120: ADR-driven development, reusable patterns, E2E test timing
  retrospectives: [
    { sprint: 127, improvements: [
      { action: "Fix close_all_documents to only close bridge documents for specific host document - current implementation closes ALL bridge docs on ANY host close (PBI-156)", timing: "product", status: "active", outcome: null },
      { action: "Add bridge connection health monitoring and auto-recovery - dead processes never replaced, leaving features permanently broken (PBI-157)", timing: "product", status: "active", outcome: null },
      { action: "Implement per-document virtual URIs - shared URI causes concurrent requests to overwrite each other's content (PBI-158)", timing: "product", status: "active", outcome: null },
      { action: "Fix didOpen race condition with proper synchronization - concurrent first access sends duplicate didOpen (PBI-159)", timing: "product", status: "active", outcome: null },
      { action: "Implement proper cleanup for timed-out bridge initializations - partially spawned servers leak processes and temp dirs (PBI-160)", timing: "product", status: "active", outcome: null },
      { action: "Fix auto-install race that crashes server on immediate edits after file open (PBI-161)", timing: "product", status: "active", outcome: null },
      { action: "Implement cancellation and backpressure for semantic tokens - flood during fast typing starves other requests (PBI-162)", timing: "product", status: "active", outcome: null },
      { action: "Investigate and fix flaky rust-analyzer unit tests - 2/357 tests fail intermittently due to resource contention (PBI-163)", timing: "product", status: "active", outcome: null },
    ] },
    { sprint: 126, improvements: [
      { action: "Consider investigating flaky unit tests in future sprint - 3 tests fail due to rust-analyzer resource contention", timing: "product", status: "completed", outcome: "Created PBI-163 to investigate and fix flaky rust-analyzer unit tests" },
    ] },
    { sprint: 124, improvements: [
      { action: "Continue with PBI-152 to address robustness issues (backpressure, notification overflow, resource cleanup, initialization timeout)", timing: "product", status: "completed", outcome: "PBI-152 completed in Sprint 125 with all 4 robustness improvements implemented" },
      { action: "Consider simplifying spawn_locks pattern in future if cleaner alternative emerges", timing: "product", status: "completed", outcome: "Created PBI-153 to move SPAWN_COUNTER to instance and simplify spawn_locks pattern" },
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
