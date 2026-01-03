// ============================================================
// Dashboard Data (AI edits this section)
// ============================================================

const userStoryRoles = [
  "Rustacean editing Markdown",
  "developer editing Lua files",
  "documentation author with Rust code blocks",
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
    number: 148,
    pbi_id: "PBI-175",
    goal: "Fix signatureHelp deadlock in pyright bridge - prevent infinite hangs on signature help requests",
    status: "in_progress",
    subtasks: [
      {
        test: "Add debug logging test to verify lock acquisition order in signature_help flow",
        implementation: "Instrument get_connection() and sync_document() with log::debug! statements showing lock acquisition attempts and completions for spawn_locks and document_open_locks",
        type: "behavioral",
        status: "completed",
        commits: [
          { hash: "73dc8d1", message: "feat(bridge): add debug logging for lock acquisition in signature_help flow", phase: "green" },
        ],
        notes: [
          "Goal: Understand actual lock acquisition patterns during signatureHelp",
          "Log before/after spawn_locks.lock().await (get_connection L153)",
          "Log before/after document_open_locks.lock().await (sync_document L865)",
          "Log entry/exit of signature_help, get_connection, sync_document_with_host",
          "This establishes observable behavior for diagnosing the hang",
        ],
      },
      {
        test: "Write unit test reproducing signatureHelp hang with concurrent operations",
        implementation: "Create tokio_async_pool test simulating concurrent signatureHelp + completion requests that trigger deadlock scenario",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [
          "Test should spawn 2+ concurrent tasks calling signature_help()",
          "Expected: All tasks complete within 30s timeout (currently hangs)",
          "Use tokio::time::timeout to fail test if deadlock occurs",
          "Model after existing concurrent tests (e.g., connection_eviction tests)",
        ],
      },
      {
        test: "Verify test reproduces hang by running with instrumentation",
        implementation: "Run new test with debug logs enabled, confirm it hangs and shows lock acquisition pattern",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [
          "This is the RED phase - test should fail/hang",
          "Analyze debug logs to identify exact deadlock sequence",
          "Document findings: which locks are held, which are awaited",
          "Hypothesis: spawn_locks held while awaiting document_open_locks creates circular wait",
        ],
      },
      {
        test: "Fix deadlock by refactoring lock scoping",
        implementation: "Restructure get_connection() or sync_document() to avoid holding spawn_locks when acquiring document_open_locks (or vice versa)",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [
          "Potential fix: Drop spawn_locks guard before calling sync_document_with_host()",
          "Ensure connection is still safely initialized before releasing lock",
          "Alternative: Combine locks into single mutex if protecting same critical section",
          "Verify fix doesn't break PBI-159 (duplicate didOpen prevention)",
        ],
      },
      {
        test: "Verify all acceptance criteria pass",
        implementation: "Run unit test (should complete), run make test, check logs for START/DONE pairing",
        type: "behavioral",
        status: "pending",
        commits: [],
        notes: [
          "AC1: signatureHelp completes within 30s timeout",
          "AC2: Subsequent completion requests receive responses",
          "AC3: Logs show paired START/DONE for all requests",
          "AC4: Instrumentation confirms no deadlock in lock acquisition",
          "Run make test_nvim for E2E validation if possible",
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

  completed: [
    { number: 147, pbi_id: "PBI-174", goal: "Audit API visibility in LanguageCoordinator - 1 method made private", status: "done", subtasks: [] },
    { number: 146, pbi_id: "PBI-173", goal: "Parameterize offset clamping tests with rstest (3→1 test)", status: "done", subtasks: [] },
    { number: 145, pbi_id: "PBI-172", goal: "Relocate smoke tests from integration to unit test location", status: "done", subtasks: [] },
    { number: 144, pbi_id: "PBI-171", goal: "Investigate $/cancelRequest handling via custom_method - blocked by tower-lsp architecture", status: "cancelled", subtasks: [] },
    { number: 143, pbi_id: "PBI-170", goal: "Investigate $/cancelRequest - deferred (tower-lsp limitation, YAGNI)", status: "cancelled", subtasks: [] },
    { number: 142, pbi_id: "PBI-169", goal: "Fix bridge bookkeeping memory leak after crashes/restarts", status: "done", subtasks: [] },
    { number: 141, pbi_id: "PBI-168", goal: "Fix concurrent parse crash recovery to correctly identify failing parsers", status: "done", subtasks: [] },
  ],

  retrospectives: [
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
