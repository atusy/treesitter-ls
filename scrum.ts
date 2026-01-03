// ============================================================
// Dashboard Data (AI edits this section)
// ============================================================

const userStoryRoles = [
  "Rustacean editing Markdown",
  "developer editing Lua files",
  "documentation author with Rust code blocks",
  "developer editing Markdown with code blocks",
] as const satisfies readonly string[];

const scrum: ScrumDashboard = {
  product_goal: {
    statement:
      "Maintain stable async LSP bridge for core features using single-pool architecture (ADR-0006, 0007, 0008)",
    success_metrics: [
      { metric: "Bridge coverage", target: "Support hover, completion, signatureHelp, definition with fully async implementations" },
      { metric: "Modular architecture", target: "Bridge module organized with text_document/ subdirectory, single TokioAsyncLanguageServerPool" },
      { metric: "E2E test coverage", target: "Each bridged feature has E2E test verifying end-to-end async flow" },
    ],
  },

  // Deferred: PBI-091 (idle cleanup), PBI-107 (WorkspaceType), PBI-171 ($/cancelRequest - tower-lsp internals)
  product_backlog: [],

  sprint: {
    number: 149,
    pbi_id: "PBI-176",
    goal: "Add timeout protection to prevent LSP request hangs",
    status: "done",
    subtasks: [
      {
        test: "Test that is_alive() returns false after timeout when child lock is held indefinitely",
        implementation: "Wrap is_alive() child lock acquisition with tokio::time::timeout, return false on timeout",
        type: "behavioral",
        status: "completed",
        commits: [
          {
            hash: "84721b4",
            message: "feat(bridge): add 5s timeout to is_alive() to prevent indefinite hangs",
            phase: "green",
          },
        ],
        notes: [
          "Smallest scope: single method with existing Mutex",
          "Current: child_mutex.lock().await blocks indefinitely if lock held",
          "Target: tokio::time::timeout(Duration, child_mutex.lock()).await returns Err on timeout",
          "Timeout duration: 5 seconds (matches shutdown timeout pattern)",
        ],
      },
      {
        test: "Test that get_connection() returns None after timeout when spawn/initialize hangs",
        implementation: "Wrap spawn_and_initialize future with tokio::time::timeout in get_connection()",
        type: "behavioral",
        status: "completed",
        commits: [
          {
            hash: "d7a8253",
            message: "feat(bridge): reduce get_connection() timeout from 60s to 30s",
            phase: "green",
          },
        ],
        notes: [
          "Medium scope: get_connection() slow path (connection spawn)",
          "Current: spawn_and_initialize().await blocks indefinitely if server hangs",
          "Target: tokio::time::timeout(Duration, spawn_and_initialize()).await returns Err on timeout",
          "Timeout duration: 30 seconds (initialization can be slow)",
          "Fast path (cached connection) already has is_alive() timeout from Subtask 1",
        ],
      },
      {
        test: "Test that sync_document() returns None after timeout when document lock acquisition hangs",
        implementation: "Wrap document_open_locks per-URI lock acquisition with tokio::time::timeout",
        type: "behavioral",
        status: "completed",
        commits: [
          {
            hash: "811811a",
            message: "feat(bridge): add 10s timeout to sync_document() lock acquisition",
            phase: "green",
          },
        ],
        notes: [
          "Per-URI lock scope: sync_document() document opening serialization",
          "Current: document_open_locks.entry().or_default().lock().await blocks indefinitely",
          "Target: tokio::time::timeout(Duration, lock.lock()).await returns Err on timeout",
          "Timeout duration: 10 seconds (document sync should be quick)",
        ],
      },
      {
        test: "Code review: all async bridge methods (hover, completion, signature_help, definition) have paired START/DONE logging",
        implementation: "Add START/DONE logging to hover, completion, definition (signature_help already has it)",
        type: "structural",
        status: "completed",
        commits: [
          {
            hash: "54105cd",
            message: "feat(bridge): add START/DONE logging to hover, completion, goto_definition",
            phase: "green",
          },
        ],
        notes: [
          "Current: Only signature_help has START/DONE logging",
          "Target: All 4 methods log [REQUEST] method_name START/DONE with key and host_uri",
          "Pattern: log::debug at entry and before return",
          "Enables hang diagnosis: if START logged but no DONE, request is stuck",
        ],
      },
      {
        test: "E2E test: subsequent LSP requests processed after a request times out",
        implementation: "Create E2E test that triggers timeout in one request, verifies next request succeeds",
        type: "behavioral",
        status: "completed",
        commits: [
          {
            hash: "5996c0a",
            message: "test(bridge): add E2E test for timeout propagation",
            phase: "green",
          },
        ],
        notes: [
          "Integration verification: timeout errors don't block request queue",
          "Test approach: Mock slow server, send request that times out, send fast request that succeeds",
          "Verifies AC5: tower-lsp continues processing after timeout",
          "Location: tests/e2e_tests/ or src/lsp/bridge/tokio_async_pool.rs #[tokio::test]",
        ],
      },
    ],
  },

  definition_of_done: {
    checks: [
      { name: "All unit tests pass", run: "make test" },
      { name: "Code quality checks pass", run: "make check" },
      { name: "E2E tests pass", run: "make test_e2e" },
    ],
  },

  completed: [
    { number: 148, pbi_id: "PBI-175", goal: "Investigate signatureHelp deadlock - no deadlock found, added defensive logging/tests/docs", status: "done", subtasks: [] },
    { number: 147, pbi_id: "PBI-174", goal: "Audit API visibility in LanguageCoordinator - 1 method made private", status: "done", subtasks: [] },
    { number: 146, pbi_id: "PBI-173", goal: "Parameterize offset clamping tests with rstest (3→1 test)", status: "done", subtasks: [] },
    { number: 145, pbi_id: "PBI-172", goal: "Relocate smoke tests from integration to unit test location", status: "done", subtasks: [] },
    { number: 144, pbi_id: "PBI-171", goal: "Investigate $/cancelRequest handling via custom_method - blocked by tower-lsp architecture", status: "cancelled", subtasks: [] },
    { number: 143, pbi_id: "PBI-170", goal: "Investigate $/cancelRequest - deferred (tower-lsp limitation, YAGNI)", status: "cancelled", subtasks: [] },
    { number: 142, pbi_id: "PBI-169", goal: "Fix bridge bookkeeping memory leak after crashes/restarts", status: "done", subtasks: [] },
    { number: 141, pbi_id: "PBI-168", goal: "Fix concurrent parse crash recovery to correctly identify failing parsers", status: "done", subtasks: [] },
  ],

  retrospectives: [
    { sprint: 148, improvements: [
      { action: "TDD investigation disproved deadlock hypothesis. Added defensive logging/test/docs", timing: "immediate", status: "completed", outcome: "No deadlock exists - locks acquired sequentially. Created PBI-176 for timeout protection (actual root cause)" },
    ] },
    { sprint: 147, improvements: [
      { action: "Test review findings (review-tests.md) addressed: smoke tests relocated, tests parameterized, API visibility audited", timing: "immediate", status: "completed", outcome: "3 PBIs completed (172-174), test pyramid improved, rstest adopted for parameterization" },
    ] },
    { sprint: 144, improvements: [
      { action: "Investigation: LspServiceBuilder.custom_method cannot intercept $/cancelRequest because tower-lsp registers it first in generated code before custom methods", timing: "product", status: "completed", outcome: "PBI-171 deferred - tower-lsp's Router uses HashMap with first-registration-wins, blocking custom interception" },
      { action: "Current architecture already supports request superseding: new semantic token requests automatically cancel previous ones via SemanticRequestTracker", timing: "product", status: "completed", outcome: "Explicit $/cancelRequest handling deemed unnecessary (YAGNI) - existing superseding mechanism sufficient for user typing scenarios" },
    ] },
    { sprint: 143, improvements: [
      { action: "Review-codex3 findings: PBI-168, PBI-169 fixed; PBI-170 deferred (tower-lsp limitation, YAGNI)", timing: "product", status: "completed", outcome: "2/3 issues resolved, 1 deferred" },
    ] },
    { sprint: 140, improvements: [
      { action: "Flaky tests eliminated with serial_test for rust-analyzer tests", timing: "immediate", status: "completed", outcome: "373/373 tests pass consistently (10 consecutive runs verified)" },
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
